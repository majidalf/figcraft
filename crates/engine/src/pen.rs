//! `import.pen`: pen.dev layers into FigCraft. A `.pen` file is encrypted and pen.dev only opens it
//! through its own tools, so this reads the node JSON those tools hand out (the `.pen` schema pen.dev
//! publishes for agents): an agent reads a design with pen.dev's `execute` → `Get`, with variables
//! and instances resolved, and passes the result here. The layers are rebuilt as one batch of
//! `node.create` steps (see [`crate::layer_tree`]), one undo step.
//!
//! Each node may carry `b: [x, y, width, height]`, its resolved box from `Get`'s visitor, which sizes
//! what the schema leaves to the layout (`fit_content`, `fill_container`, text, groups). What has no
//! FigCraft equivalent (image fills, shaders, scripts, live browsers) is left out and listed in
//! `warnings`. Icons come from FigCraft's own copies of the same libraries ([`crate::icon_sets`]).

use serde_json::{Map, Value, json};

use crate::cmd::CommandSpec;
use crate::params::{get, num};
use crate::{EngineError, Result, Session, bad, spec};

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        "import.pen",
        "Import from pen.dev",
        "{nodes (pen.dev node JSON from its MCP `execute` Get, variables and instances resolved; one node or an array; optional `b: [x, y, w, h]` resolved boxes), parent? (id; default the page), x?, y? (where the first root goes; default its own x/y)}  one undo step  → {ids, layers, warnings}",
        false,
        import
    )]
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let nodes = get(p, "nodes").ok_or_else(|| bad("import.pen", "`nodes` is required: pen.dev node JSON (read_guide pen)"))?;
    let list: Vec<&Value> = match nodes {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![nodes],
        _ => return Err(bad("import.pen", "`nodes` must be a pen.dev node or an array of them")),
    };
    let mut cx = Convert::default();
    let mut layers = Vec::new();
    for n in list {
        if let Some(l) = cx.node(n, false, 0) {
            layers.push(l);
        }
    }
    if layers.is_empty() {
        return Err(bad("import.pen", format!("nothing to import ({})", cx.warnings.join("; "))));
    }
    place_roots(&mut layers, num(p, "x"), num(p, "y"));
    let plan = crate::layer_tree::plan(&Value::Array(layers), get(p, "parent")).map_err(|e| bad("import.pen", e))?;
    let steps: Vec<(String, Value)> =
        plan.steps.iter().map(|st| (st["command"].as_str().unwrap_or("node.create").to_string(), st["params"].clone())).collect();
    let results = s.batch("Import from pen.dev", &steps).map_err(|e| EngineError::Other(crate::layer_tree::locate(&e.to_string(), &plan)))?;
    let ids: Vec<Value> = plan.roots.iter().filter_map(|i| results.get(*i)?.get("id").cloned()).collect();
    let roots: Vec<_> = ids.iter().filter_map(crate::params::id_of).collect();
    crate::cmd::edit::select_and_page(s, roots)?;
    Ok(json!({"ids": ids, "layers": plan.steps.len(), "warnings": cx.warnings}))
}

/// Move the roots so the first sits at `x`/`y` and the rest keep their places relative to it.
fn place_roots(layers: &mut [Value], x: Option<f64>, y: Option<f64>) {
    let at = |l: &Value, k: &str| l.get("props").and_then(|p| p.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
    let Some(first) = layers.first() else { return };
    let (dx, dy) = (x.map_or(0.0, |x| x - at(first, "x")), y.map_or(0.0, |y| y - at(first, "y")));
    for l in layers.iter_mut() {
        let (lx, ly) = (at(l, "x"), at(l, "y"));
        if let Some(Value::Object(props)) = l.get_mut("props") {
            props.insert("x".into(), json!(lx + dx));
            props.insert("y".into(), json!(ly + dy));
        }
    }
}

/// Most warnings kept; past this the rest are counted.
const MAX_WARNINGS: usize = 50;

#[derive(Default)]
struct Convert {
    warnings: Vec<String>,
    dropped: usize,
}

impl Convert {
    fn warn(&mut self, w: String) {
        if self.warnings.len() < MAX_WARNINGS {
            if !self.warnings.contains(&w) {
                self.warnings.push(w);
            }
        } else {
            self.dropped += 1;
            let more = format!("… and {} more", self.dropped);
            if let Some(last) = self.warnings.last_mut() {
                *last = more;
            }
        }
    }

    /// One pen.dev node as a `layer_tree` layer. `in_layout`: the parent lays its children out.
    fn node(&mut self, n: &Value, in_layout: bool, depth: usize) -> Option<Value> {
        let o = n.as_object()?;
        let ty = o.get("type").and_then(Value::as_str).unwrap_or("");
        let name = o.get("name").and_then(Value::as_str).unwrap_or(ty);
        if depth > crate::layer_tree::MAX_DEPTH {
            self.warn(format!("\"{name}\": nested deeper than {} levels, left out", crate::layer_tree::MAX_DEPTH));
            return None;
        }
        let kind = match ty {
            "frame" if o.get("reusable") == Some(&Value::Bool(true)) => "component",
            "frame" | "group" => "frame",
            "rectangle" => "rectangle",
            "ellipse" => "ellipse",
            "polygon" => "polygon",
            "path" => "vector",
            "text" => "text",
            // Sized like a frame here, then swapped for the library icon at the end.
            "icon" => "frame",
            "ref" => {
                self.warn(format!("\"{name}\" is an unresolved instance: read the design with resolveInstances: true"));
                return None;
            }
            "note" | "prompt" | "context" | "script" | "browser" => {
                self.warn(format!("{ty} \"{name}\" has no FigCraft equivalent, left out"));
                return None;
            }
            other => {
                self.warn(format!("unknown node type `{other}` (\"{name}\"), left out"));
                return None;
            }
        };
        let mut props = Map::new();
        let mut layer = Map::new();
        layer.insert("type".into(), json!(kind));
        layer.insert("name".into(), json!(name));
        let b = boxed(o);
        let absolute = str_of(o, "layoutPosition") == Some("absolute");
        if absolute && in_layout {
            props.insert("absolute".into(), json!(true));
        }
        // Rotation goes in before x/y (node.set applies it first), so x/y stay the top-left corner
        // pen.dev rotates about.
        if let Some(r) = self.number(o, "rotation", name).filter(|r| *r != 0.0) {
            props.insert("rotation".into(), json!(r));
        }
        if !in_layout || absolute {
            let x = self.number(o, "x", name).or(b.map(|b| b[0])).unwrap_or(0.0);
            let y = self.number(o, "y", name).or(b.map(|b| b[1])).unwrap_or(0.0);
            props.insert("x".into(), json!(x));
            props.insert("y".into(), json!(y));
        }
        if o.get("enabled") == Some(&Value::Bool(false)) {
            props.insert("visible".into(), json!(false));
        }
        if let Some(op) = self.number(o, "opacity", name).filter(|v| *v < 1.0) {
            props.insert("opacity".into(), json!(op.clamp(0.0, 1.0)));
        }
        for k in ["flipX", "flipY"] {
            if o.get(k) == Some(&Value::Bool(true)) {
                self.warn(format!("\"{name}\": {k} is not imported"));
            }
        }
        let is_frame = ty == "frame" || ty == "group" || ty == "icon";
        let lays_out = ty == "frame" && str_of(o, "layout") != Some("none");
        self.size(o, b, kind, lays_out, in_layout && !absolute, name, &mut layer, &mut props);
        // Explicit even when empty: FigCraft's new frames and shapes have a fill of their own.
        let fills = if ty == "icon" || ty == "group" { Vec::new() } else { self.paints(o.get("fill"), name) };
        props.insert("fills".into(), Value::Array(fills));
        if ty != "group" {
            self.stroke(o, name, &mut props);
            if let Some(r) = o.get("cornerRadius").filter(|r| corner_ok(r)) {
                props.insert("cornerRadius".into(), r.clone());
            }
        }
        self.effects(o.get("effect"), name, &mut props);
        if is_frame {
            props.insert("clip".into(), json!(o.get("clip") == Some(&Value::Bool(true))));
        }
        if lays_out {
            self.layout(o, name, &mut props);
        }
        match ty {
            "text" => self.text(o, name, &mut layer, &mut props),
            "ellipse" => {
                for (from, to) in [("innerRadius", "arcInner"), ("startAngle", "arcStart"), ("sweepAngle", "arcSweep")] {
                    if let Some(v) = self.number(o, from, name) {
                        props.insert(to.into(), json!(v));
                    }
                }
            }
            "polygon" => {
                if let Some(c) = self.number(o, "polygonCount", name) {
                    props.insert("pointCount".into(), json!(c.clamp(3.0, 60.0).round()));
                }
            }
            "path" => self.path(o, name, &mut layer, &mut props),
            "icon" => self.icon(o, name, &mut layer),
            _ => {}
        }
        if let Some(Value::Array(children)) = o.get("children").filter(|_| ty == "frame" || ty == "group") {
            let kids: Vec<Value> = children.iter().filter_map(|c| self.node(c, lays_out, depth + 1)).collect();
            if !kids.is_empty() {
                layer.insert("children".into(), Value::Array(kids));
            }
        }
        layer.insert("props".into(), Value::Object(props));
        Some(Value::Object(layer))
    }

    /// A number, or nothing; a `$variable` left unresolved is a warning.
    fn number(&mut self, o: &Map<String, Value>, k: &str, name: &str) -> Option<f64> {
        match o.get(k)? {
            Value::Number(n) => n.as_f64().filter(|v| v.is_finite()),
            Value::String(s) if s.starts_with('$') => {
                self.warn(format!("\"{name}\": `{k}` is the variable {s}; read the design with resolveVariables: true"));
                None
            }
            _ => None,
        }
    }

    /// Width and height: numbers are fixed; `fit_content` hugs (frames with layout, text);
    /// `fill_container` fills the parent's layout. The resolved box sizes the rest.
    #[allow(clippy::too_many_arguments)]
    fn size(
        &mut self,
        o: &Map<String, Value>,
        b: Option<[f64; 4]>,
        kind: &str,
        lays_out: bool,
        in_layout: bool,
        name: &str,
        layer: &mut Map<String, Value>,
        props: &mut Map<String, Value>,
    ) {
        let text = kind == "text";
        let growth = str_of(o, "textGrowth").unwrap_or("auto");
        for (k, axis, i) in [("width", "sizingH", 2usize), ("height", "sizingV", 3usize)] {
            // Frames hug by default; other nodes need a size.
            let v = o.get(k);
            let resolved = b.map(|b| b[i]);
            let sizing = match v {
                Some(Value::Number(n)) => {
                    layer.insert(k.into(), json!(n.as_f64().unwrap_or(0.0).max(0.0)));
                    "fixed"
                }
                Some(Value::String(s)) if s.starts_with("fill_container") && in_layout => "fill",
                Some(Value::String(s)) if s.starts_with("fit_content") && lays_out => "hug",
                Some(Value::String(s)) if s.starts_with('$') => {
                    self.warn(format!("\"{name}\": `{k}` is the variable {s}; read the design with resolveVariables: true"));
                    "fixed"
                }
                None if lays_out => "hug",
                _ => "fixed",
            };
            // Text sizes follow its growth, set in `text`.
            if text {
                let fixed_axis = (i == 2 && growth != "auto") || (i == 3 && growth == "fixed-width-height");
                if fixed_axis {
                    if sizing == "fill" {
                        props.insert(axis.into(), json!("fill"));
                    }
                    if let Some(w) = layer.get(k).cloned().or(resolved.map(|r| json!(r))) {
                        layer.insert(k.into(), w);
                    }
                }
                continue;
            }
            if !layer.contains_key(k)
                && let Some(r) = resolved
            {
                layer.insert(k.into(), json!(r.max(0.0)));
            }
            if sizing != "fixed" {
                props.insert(axis.into(), json!(sizing));
            }
        }
    }

    fn layout(&mut self, o: &Map<String, Value>, name: &str, props: &mut Map<String, Value>) {
        props.insert("layout".into(), json!(if str_of(o, "layout") == Some("vertical") { "vertical" } else { "horizontal" }));
        if let Some(g) = self.number(o, "gap", name) {
            props.insert("gap".into(), json!(g));
        }
        match o.get("padding") {
            Some(Value::Number(p)) => {
                props.insert("padding".into(), json!(p));
            }
            Some(Value::Array(p)) => {
                let n: Vec<f64> = p.iter().filter_map(Value::as_f64).collect();
                let pad = match n.as_slice() {
                    [v, h] => Some(json!([v, h, v, h])),
                    [t, r, b, l] => Some(json!([t, r, b, l])),
                    _ => None,
                };
                match pad {
                    Some(pad) => {
                        props.insert("padding".into(), pad);
                    }
                    None => self.warn(format!("\"{name}\": padding {} is not imported (variables? resolveVariables: true)", Value::Array(p.clone()))),
                }
            }
            Some(Value::String(s)) => self.warn(format!("\"{name}\": padding is the variable {s}; read the design with resolveVariables: true")),
            _ => {}
        }
        let align = |v: Option<&str>| match v {
            Some("center") => Some("center"),
            Some("end") => Some("max"),
            Some("space_between") => Some("spaceBetween"),
            Some("space_around") => Some("spaceAround"),
            _ => None,
        };
        if let Some(a) = align(str_of(o, "justifyContent")) {
            props.insert("mainAlign".into(), json!(a));
        }
        if let Some(a) = align(str_of(o, "alignItems")).filter(|a| ["center", "max"].contains(a)) {
            props.insert("crossAlign".into(), json!(a));
        }
        // pen.dev lays children out inside the padding only; FigCraft counts an inside stroke too
        // unless told otherwise (other strokes take no room either way).
        let inside = props.get("strokeAlign").and_then(Value::as_str) == Some("inside");
        if inside && o.get("layoutIncludeStroke") != Some(&Value::Bool(true)) {
            props.insert("strokesIncluded".into(), json!(false));
        }
    }

    fn text(&mut self, o: &Map<String, Value>, name: &str, layer: &mut Map<String, Value>, props: &mut Map<String, Value>) {
        match o.get("content") {
            Some(Value::String(s)) if s.starts_with('$') => {
                self.warn(format!("\"{name}\": its text is the variable {s}; read the design with resolveVariables: true"));
            }
            Some(Value::String(s)) => {
                layer.insert("text".into(), json!(s));
            }
            _ => {}
        }
        if let Some(f) = str_of(o, "fontFamily").filter(|f| !f.starts_with('$')) {
            props.insert("fontFamily".into(), json!(f));
        }
        if let Some(sz) = self.number(o, "fontSize", name) {
            props.insert("fontSize".into(), json!(sz));
        }
        let weight = match o.get("fontWeight") {
            Some(Value::String(s)) => s.parse::<f64>().ok().or(match s.to_ascii_lowercase().as_str() {
                "bold" => Some(700.0),
                "normal" | "regular" => Some(400.0),
                _ => None,
            }),
            Some(Value::Number(n)) => n.as_f64(),
            _ => None,
        };
        let italic = str_of(o, "fontStyle").is_some_and(|s| s.eq_ignore_ascii_case("italic"));
        if weight.is_some() || italic {
            props.insert("fontStyle".into(), json!(font_style(weight.unwrap_or(400.0), italic)));
        }
        if let Some(lh) = self.number(o, "lineHeight", name) {
            props.insert("lineHeight".into(), json!(format!("{}%", (lh * 10000.0).round() / 100.0)));
        }
        if let Some(ls) = self.number(o, "letterSpacing", name) {
            props.insert("letterSpacing".into(), json!(ls));
        }
        if let Some(a) = str_of(o, "textAlign") {
            props.insert("textAlign".into(), json!(if a == "justify" { "justified" } else { a }));
        }
        if let Some(a) = str_of(o, "textAlignVertical") {
            props.insert("textAlignVertical".into(), json!(a));
        }
        let resize = match str_of(o, "textGrowth") {
            Some("fixed-width") => "height",
            Some("fixed-width-height") => "none",
            _ => "widthAndHeight",
        };
        props.insert("textAutoResize".into(), json!(resize));
        if o.get("underline") == Some(&Value::Bool(true)) {
            props.insert("textDecoration".into(), json!("underline"));
        } else if o.get("strikethrough") == Some(&Value::Bool(true)) {
            props.insert("textDecoration".into(), json!("strikethrough"));
        }
        if o.contains_key("href") {
            self.warn(format!("\"{name}\": its link is not imported"));
        }
    }

    /// A pen.dev icon as the same icon from FigCraft's libraries (see [`crate::icon_sets`]), in
    /// its fill colour; an icon not found stays an empty frame named after it.
    fn icon(&mut self, o: &Map<String, Value>, name: &str, layer: &mut Map<String, Value>) {
        let library = str_of(o, "library").unwrap_or("lucide");
        let icon = str_of(o, "icon").unwrap_or("");
        let Some((set, found, style)) = pen_icon(library, icon) else {
            self.warn(format!("\"{name}\": no {library} icon `{icon}` in FigCraft's libraries; an empty frame named after it"));
            layer.insert("name".into(), json!(format!("{name} (icon {icon})")));
            return;
        };
        let color = match o.get("fill") {
            Some(Value::Array(a)) => a.first(),
            other => other,
        }
        .and_then(|f| match f {
            Value::String(s) => Some(s.as_str()),
            Value::Object(p) => str_of(p, "color"),
            _ => None,
        })
        .and_then(|c| self.color(c, name))
        .unwrap_or_else(|| "#000000".into());
        layer.insert("type".into(), json!("icon"));
        layer.insert("set".into(), json!(set));
        layer.insert("icon".into(), json!(found));
        if let Some(st) = style {
            layer.insert("style".into(), json!(st));
        }
        layer.insert("color".into(), json!(color));
        layer.remove("children");
    }

    /// A path's SVG data, mapped from its `viewBox` onto the node's box as pen.dev draws it.
    fn path(&mut self, o: &Map<String, Value>, name: &str, layer: &mut Map<String, Value>, props: &mut Map<String, Value>) {
        let Some(d) = str_of(o, "geometry") else {
            self.warn(format!("\"{name}\": a path without geometry (read with includePathGeometry: true) is an empty vector"));
            return;
        };
        let Ok(mut path) = figcraft_geom::BezPath::from_svg(d) else {
            self.warn(format!("\"{name}\": its path data could not be read, left empty"));
            return;
        };
        use figcraft_geom::Shape as _;
        let own = path.bounding_box();
        let vb = o.get("viewBox").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>());
        let (vx, vy, vw, vh) = match vb.as_deref() {
            Some([x, y, w, h]) if *w > 0.0 && *h > 0.0 => (*x, *y, *w, *h),
            _ => (own.x0, own.y0, own.width().max(1e-9), own.height().max(1e-9)),
        };
        let w = layer.get("width").and_then(Value::as_f64).unwrap_or(vw);
        let h = layer.get("height").and_then(Value::as_f64).unwrap_or(vh);
        path.apply_affine(figcraft_geom::Affine::new([w / vw, 0.0, 0.0, h / vh, -vx * w / vw, -vy * h / vh]));
        // The vector takes its path's own box; offset it to where that box sits in the node.
        let placed = path.bounding_box();
        if let (Some(x), Some(y)) = (props.get("x").and_then(Value::as_f64), props.get("y").and_then(Value::as_f64)) {
            props.insert("x".into(), json!(x + placed.x0));
            props.insert("y".into(), json!(y + placed.y0));
        }
        layer.remove("width");
        layer.remove("height");
        layer.insert("path".into(), json!(path.to_svg()));
        if str_of(o, "fillRule") == Some("evenodd") {
            self.warn(format!("\"{name}\": the even-odd fill rule is not imported"));
        }
    }

    fn paints(&mut self, v: Option<&Value>, name: &str) -> Vec<Value> {
        let list: Vec<&Value> = match v {
            None | Some(Value::Null) => return Vec::new(),
            Some(Value::Array(a)) => a.iter().collect(),
            Some(one) => vec![one],
        };
        list.into_iter().filter_map(|p| self.paint(p, name)).collect()
    }

    fn paint(&mut self, p: &Value, name: &str) -> Option<Value> {
        let o = match p {
            Value::String(s) => return self.color(s, name).map(Value::String),
            Value::Object(o) => o,
            _ => return None,
        };
        let mut out = match str_of(o, "type")? {
            "color" => json!({"type": "solid", "color": self.color(str_of(o, "color")?, name)?}),
            "gradient" => self.gradient(o, name)?,
            other => {
                let what = if other == "image" { format!("image fill {}", str_of(o, "url").unwrap_or("")) } else { format!("{other} fill") };
                self.warn(format!("\"{name}\": the {} is not imported", what.trim_end()));
                return None;
            }
        };
        if o.get("enabled") == Some(&Value::Bool(false)) {
            out["visible"] = json!(false);
        }
        if str_of(o, "blendMode").is_some_and(|m| m != "normal") {
            self.warn(format!("\"{name}\": paint blend modes are not imported"));
        }
        Some(out)
    }

    /// A `#RGB`, `#RRGGBB` or `#RRGGBBAA` colour as `#RRGGBB[AA]`.
    fn color(&mut self, s: &str, name: &str) -> Option<String> {
        if s.starts_with('$') {
            self.warn(format!("\"{name}\": colour {s} is a variable; read the design with resolveVariables: true"));
            return None;
        }
        let hex = s.strip_prefix('#')?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            3 | 4 => Some(format!("#{}", hex.chars().flat_map(|c| [c, c]).collect::<String>())),
            6 | 8 => Some(format!("#{hex}")),
            _ => None,
        }
    }

    /// pen.dev gradients are a centre, a size and a rotation in the unit box (0° up, counter-
    /// clockwise); FigCraft's are handles in the same box.
    fn gradient(&mut self, o: &Map<String, Value>, name: &str) -> Option<Value> {
        let stops: Vec<Value> = o
            .get("colors")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(|s| {
                let c = str_of(s.as_object()?, "color")?.to_string();
                let pos = s.get("position").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                Some((c, pos))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|(c, pos)| Some(json!({"position": pos, "color": self.color(&c, name)?})))
            .collect();
        if stops.is_empty() {
            return None;
        }
        let pt = |k: &str, d: (f64, f64), xk: &str, yk: &str| {
            o.get(k).map_or(d, |v| (v.get(xk).and_then(Value::as_f64).unwrap_or(d.0), v.get(yk).and_then(Value::as_f64).unwrap_or(d.1)))
        };
        let (cx, cy) = pt("center", (0.5, 0.5), "x", "y");
        let (w, h) = pt("size", (1.0, 1.0), "width", "height");
        let rot = o.get("rotation").and_then(Value::as_f64).unwrap_or(0.0).to_radians();
        let (kind, from, to) = match str_of(o, "gradientType").unwrap_or("linear") {
            "linear" => {
                // The axis runs from the first stop to the last, `h` long, pointing up at 0°.
                let (dx, dy) = (-rot.sin() * h / 2.0, -rot.cos() * h / 2.0);
                ("linearGradient", [cx - dx, cy - dy], [cx + dx, cy + dy])
            }
            t @ ("radial" | "angular") => (if t == "radial" { "radialGradient" } else { "angularGradient" }, [cx, cy], [cx, cy + h / 2.0]),
            other => {
                self.warn(format!("\"{name}\": {other} gradients are not imported"));
                return None;
            }
        };
        let mut g = json!({"type": kind, "stops": stops, "from": from, "to": to});
        if kind != "linearGradient" {
            g["width"] = json!([cx + w / 2.0, cy]);
        }
        if let Some(op) = o.get("opacity").and_then(Value::as_f64).filter(|v| *v < 1.0) {
            g["opacity"] = json!(op.clamp(0.0, 1.0));
        }
        Some(g)
    }

    fn stroke(&mut self, o: &Map<String, Value>, name: &str, props: &mut Map<String, Value>) {
        let strokes = self.paints(o.get("stroke"), name);
        if strokes.is_empty() {
            return;
        }
        props.insert("strokes".into(), Value::Array(strokes));
        match o.get("strokeWidth") {
            Some(Value::Number(w)) => {
                props.insert("strokeWeight".into(), json!(w));
            }
            Some(Value::Object(sides)) => {
                for (side, key) in
                    [("top", "strokeTopWeight"), ("right", "strokeRightWeight"), ("bottom", "strokeBottomWeight"), ("left", "strokeLeftWeight")]
                {
                    props.insert(key.into(), json!(sides.get(side).and_then(Value::as_f64).unwrap_or(0.0)));
                }
            }
            _ => {}
        }
        let align = match str_of(o, "strokeAlignment") {
            Some("inner") => "inside",
            Some("outer") => "outside",
            _ => "center",
        };
        props.insert("strokeAlign".into(), json!(align));
        if let Some(j) = str_of(o, "strokeLinejoin") {
            props.insert("strokeJoin".into(), json!(j));
        }
        if str_of(o, "strokeLinecap").is_some_and(|c| c != "butt") {
            self.warn(format!("\"{name}\": stroke caps are not imported"));
        }
    }

    fn effects(&mut self, v: Option<&Value>, name: &str, props: &mut Map<String, Value>) {
        let list: Vec<&Value> = match v {
            None | Some(Value::Null) => return,
            Some(Value::Array(a)) => a.iter().collect(),
            Some(one) => vec![one],
        };
        let mut out = Vec::new();
        for e in list {
            let Some(o) = e.as_object() else { continue };
            let radius = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
            let mut fx = match str_of(o, "type") {
                Some("shadow") => {
                    let kind = if str_of(o, "shadowType") == Some("inner") { "innerShadow" } else { "dropShadow" };
                    let color = str_of(o, "color").and_then(|c| self.color(c, name)).unwrap_or_else(|| "#00000040".into());
                    let off = |k: &str| o.get("offset").and_then(|v| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
                    json!({"type": kind, "color": color, "offset": [off("x"), off("y")], "radius": radius("blur")})
                }
                Some("blur") => json!({"type": "layerBlur", "radius": radius("radius")}),
                Some("background_blur") => json!({"type": "backgroundBlur", "radius": radius("radius")}),
                other => {
                    self.warn(format!("\"{name}\": the {} effect is not imported", other.unwrap_or("unknown")));
                    continue;
                }
            };
            if o.get("enabled") == Some(&Value::Bool(false)) {
                fx["visible"] = json!(false);
            }
            out.push(fx);
        }
        if !out.is_empty() {
            props.insert("effects".into(), Value::Array(out));
        }
    }
}

fn str_of<'a>(o: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}

/// The resolved box `b: [x, y, width, height]`, when it is four finite numbers.
fn boxed(o: &Map<String, Value>) -> Option<[f64; 4]> {
    let a = o.get("b")?.as_array()?;
    let n: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).collect();
    match n.as_slice() {
        [x, y, w, h] => Some([*x, *y, *w, *h]),
        _ => None,
    }
}

/// pen.dev's icon `library` and `icon` as (set, icon name, style) in FigCraft's libraries.
/// Feather's icons live on in Lucide under the same names; a Phosphor name may carry its weight
/// (`plus-bold`); Material names use `_` where pen.dev may write `-`.
fn pen_icon(library: &str, icon: &str) -> Option<(&'static str, String, Option<String>)> {
    use crate::icon_sets::set;
    let lib = library.to_ascii_lowercase();
    let (set_id, style) = match lib.as_str() {
        "lucide" | "feather" => ("lucide", None),
        "phosphor" => ("phosphor", None),
        l if l.starts_with("material symbols") => ("material", Some(l.trim_start_matches("material symbols").trim().to_string())),
        _ => return None,
    };
    let s = set(set_id)?;
    let candidates = [icon.to_string(), icon.replace('-', "_"), icon.replace('_', "-"), icon.to_lowercase()];
    if let Some(found) = candidates.iter().find(|c| s.icon(c).is_some()) {
        return Some((set_id, found.clone(), style.filter(|st| s.styles.contains(st))));
    }
    // `plus-bold` → plus in the bold style.
    let (base, weight) = icon.rsplit_once('-')?;
    (s.styles.iter().any(|st| st == weight) && s.icon(base).is_some()).then(|| (set_id, base.to_string(), Some(weight.to_string())))
}

/// A corner radius FigCraft takes: a number or four.
fn corner_ok(r: &Value) -> bool {
    r.is_number() || r.as_array().is_some_and(|a| a.len() == 4 && a.iter().all(Value::is_number))
}

/// A CSS-style weight (100-900) as a font style name.
fn font_style(weight: f64, italic: bool) -> String {
    let w = (weight / 100.0).round().clamp(1.0, 9.0) as u32;
    let base = ["Thin", "ExtraLight", "Light", "Regular", "Medium", "SemiBold", "Bold", "ExtraBold", "Black"]
        .get(w as usize - 1)
        .copied()
        .unwrap_or("Regular");
    match (italic, base) {
        (false, b) => b.to_string(),
        (true, "Regular") => "Italic".to_string(),
        (true, b) => format!("{b} Italic"),
    }
}
