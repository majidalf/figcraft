//! Outline form of an inspected tree: one line per layer, indented by depth, for reading a screen's
//! structure at a fraction of its JSON. Built from the slimmed JSON (see `slim`), so what a line
//! leaves out is a default there too. Values the line has no short token for are named after a
//! `+` (`+effects,interactions`): `get_node` on the layer reads them.
//!
//! ```text
//! 2 FRAME "Login" 100,0 390x844 #FFFFFF vertical gap16 pad24 h:hug
//!   3 TEXT "Title" 24,24 150x34 "Welcome back" Inter/Bold/28
//! ```

use serde_json::{Map, Value};

/// Longest text content or name shown before it is cut with `…`.
const MAX_TEXT: usize = 40;

/// Keys a layer line spells out (or that say nothing an agent acts on); the rest are named after `+`.
const SHOWN: &[&str] = &[
    "id",
    "name",
    "type",
    "x",
    "y",
    "width",
    "height",
    "absoluteBounds",
    "rotation",
    "visible",
    "locked",
    "opacity",
    "fills",
    "strokes",
    "strokeWeight",
    "strokeAlign",
    "cornerRadius",
    "layout",
    "sizing",
    "clip",
    "constraints",
    "characters",
    "fontFamily",
    "fontStyle",
    "fontSize",
    "textAlign",
    "textAutoResize",
    "mainComponent",
    "mainComponentName",
    "background",
    "children",
    "childCount",
];

/// A slimmed `document.inspect` result as outline lines.
pub fn document(d: &Value) -> Vec<String> {
    let s = |k: &str| d.get(k).map(Value::to_string).unwrap_or_default();
    let mut head = format!("document {} selection {}", quoted(d.get("name").and_then(Value::as_str).unwrap_or("")), s("selection"));
    if d.get("dirty") == Some(&Value::Bool(true)) {
        head.push_str(" dirty");
    }
    for k in ["variables", "images"] {
        if let Some(n) = d.get(k).and_then(Value::as_u64).filter(|n| *n > 0) {
            head.push_str(&format!(" {k}{n}"));
        }
    }
    let mut out = vec![head];
    if let Some(h) = d.get("history") {
        out.push(format!(
            "history undo {} redo {}",
            h.get("undo").map(Value::to_string).unwrap_or_default(),
            h.get("redo").map(Value::to_string).unwrap_or_default()
        ));
    }
    for st in d.get("styles").and_then(Value::as_array).into_iter().flatten() {
        let g = |k: &str| st.get(k).map(num).unwrap_or_default();
        out.push(format!("style {} {} {}", g("id"), g("type"), quoted(st.get("name").and_then(Value::as_str).unwrap_or(""))));
    }
    let current = d.get("currentPage");
    for p in d.get("pages").and_then(Value::as_array).into_iter().flatten() {
        let mut line =
            format!("page {} {}", p.get("id").map(Value::to_string).unwrap_or_default(), quoted(p.get("name").and_then(Value::as_str).unwrap_or("")));
        if let Some(bg) = p.get("background").and_then(Value::as_str) {
            line.push_str(&format!(" bg{bg}"));
        }
        if p.get("id").is_some() && p.get("id") == current {
            line.push_str(" current");
        }
        if let Some(n) = p.get("childCount").and_then(Value::as_u64) {
            line.push_str(&format!(" ({n} children)"));
        }
        out.push(line);
        for c in p.get("children").and_then(Value::as_array).into_iter().flatten() {
            tree_into(c, 1, &mut out);
        }
    }
    out
}

/// A slimmed layer and its children as outline lines.
pub fn tree(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    tree_into(v, 0, &mut out);
    out
}

fn tree_into(v: &Value, depth: usize, out: &mut Vec<String>) {
    let Some(o) = v.as_object() else { return };
    out.push(format!("{}{}", "  ".repeat(depth), line(o)));
    for c in o.get("children").and_then(Value::as_array).into_iter().flatten() {
        tree_into(c, depth + 1, out);
    }
}

/// One layer: `id TYPE "name" x,y WxH` then whatever differs from a new layer.
fn line(o: &Map<String, Value>) -> String {
    let n = |k: &str| o.get(k).map(num).unwrap_or_default();
    let mut t = vec![o.get("id").map(Value::to_string).unwrap_or_default(), o.get("type").and_then(Value::as_str).unwrap_or("?").to_string()];
    t.push(quoted(o.get("name").and_then(Value::as_str).unwrap_or("")));
    if o.contains_key("x") {
        t.push(format!("{},{} {}x{}", n("x"), n("y"), n("width"), n("height")));
    }
    if let Some(r) = o.get("rotation") {
        t.push(format!("rot{}", num(r)));
    }
    if o.get("visible") == Some(&Value::Bool(false)) {
        t.push("hidden".into());
    }
    if o.get("locked") == Some(&Value::Bool(true)) {
        t.push("locked".into());
    }
    if let Some(op) = o.get("opacity") {
        t.push(format!("opacity{}", num(op)));
    }
    if let Some(f) = o.get("fills") {
        t.push(paints(f));
    }
    if let Some(s) = o.get("strokes") {
        let w = o.get("strokeWeight").map(num).unwrap_or_default();
        let align = o.get("strokeAlign").and_then(Value::as_str).filter(|a| *a != "inside").map(|a| format!("/{a}")).unwrap_or_default();
        t.push(format!("stroke{}/{w}{align}", paints(s)));
    }
    if let Some(r) = o.get("cornerRadius") {
        t.push(format!("r{}", list(r)));
    }
    if let Some(Value::Object(l)) = o.get("layout") {
        layout(l, &mut t);
    }
    let text = o.get("characters").and_then(Value::as_str);
    if let Some(Value::Object(s)) = o.get("sizing") {
        let axis = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("fixed");
        // Auto-width text hugs both ways anyway.
        let hugs_as_text = text.is_some() && axis("horizontal") == "hug" && axis("vertical") == "hug";
        for (label, k) in [("w", "horizontal"), ("h", "vertical")] {
            if axis(k) != "fixed" && !hugs_as_text {
                t.push(format!("{label}:{}", axis(k)));
            }
        }
    }
    if o.get("clip") == Some(&Value::Bool(false)) {
        t.push("noclip".into());
    }
    if let Some(c) = text {
        // A text layer is named after its characters until renamed.
        if o.get("name").and_then(Value::as_str) != Some(c) {
            t.push(quoted(c));
        }
        let g = |k: &str| o.get(k).map(num).unwrap_or_default();
        t.push(format!("{}/{}/{}", g("fontFamily"), g("fontStyle"), g("fontSize")));
        if let Some(a) = o.get("textAlign").and_then(Value::as_str) {
            t.push(format!("align:{a}"));
        }
        if let Some(r) = o.get("textAutoResize").and_then(Value::as_str).filter(|r| *r != "widthAndHeight") {
            t.push(format!("resize:{r}"));
        }
    }
    if let Some(Value::Object(c)) = o.get("constraints") {
        let h = c.get("horizontal").and_then(Value::as_str).unwrap_or("left");
        let v = c.get("vertical").and_then(Value::as_str).unwrap_or("top");
        if (h, v) != ("left", "top") {
            t.push(format!("pin:{h}/{v}"));
        }
    }
    if let Some(m) = o.get("mainComponent") {
        let name = o.get("mainComponentName").and_then(Value::as_str).map(|s| format!(" {}", quoted(s))).unwrap_or_default();
        t.push(format!("of:{}{name}", num(m)));
    }
    if let Some(c) = o.get("childCount") {
        t.push(format!("({} children)", num(c)));
    }
    let more: Vec<&str> = o.keys().map(String::as_str).filter(|k| !SHOWN.contains(k)).collect();
    if !more.is_empty() {
        t.push(format!("+{}", more.join(",")));
    }
    t.join(" ")
}

fn layout(l: &Map<String, Value>, t: &mut Vec<String>) {
    let s = |k: &str| l.get(k).and_then(Value::as_str);
    t.push(s("mode").unwrap_or("layout").to_string());
    let zero = |v: &&Value| v.as_f64() == Some(0.0);
    if let Some(g) = l.get("gap").filter(|g| !zero(g)) {
        t.push(format!("gap{}", num(g)));
    }
    if let Some(g) = l.get("crossGap").filter(|g| !zero(g)) {
        t.push(format!("crossGap{}", num(g)));
    }
    if let Some(p) = l.get("padding").filter(|p| !zero(p)) {
        t.push(format!("pad{}", list(p)));
    }
    for (k, label) in [("mainAlign", "main"), ("crossAlign", "cross"), ("contentAlign", "content")] {
        if let Some(v) = s(k) {
            t.push(format!("{label}:{v}"));
        }
    }
    if l.get("wrap") == Some(&Value::Bool(true)) {
        t.push("wrap".into());
    }
    for k in ["gridColumns", "gridRows"] {
        if let Some(v) = l.get(k) {
            t.push(format!("{k}{}", num(v)));
        }
    }
    let rest: Vec<&str> = l
        .keys()
        .map(String::as_str)
        .filter(|k| !["mode", "gap", "crossGap", "padding", "mainAlign", "crossAlign", "contentAlign", "wrap", "gridColumns", "gridRows"].contains(k))
        .collect();
    if !rest.is_empty() {
        t.push(format!("+layout.{}", rest.join(",layout.")));
    }
}

/// `"#hex"` for one plain solid paint, else the paint types joined by `+` (`linearGradient+#FFFFFF`).
fn paints(v: &Value) -> String {
    let Some(a) = v.as_array() else { return num(v) };
    if a.is_empty() {
        return "none".into();
    }
    a.iter()
        .map(|p| match p {
            Value::String(s) => s.clone(),
            _ => p.get("type").and_then(Value::as_str).unwrap_or("paint").to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// A number as written in JSON, or a list as `a,b,c,d`.
fn list(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(num).collect::<Vec<_>>().join(","),
        other => num(other),
    }
}

/// A scalar without quotes (strings bare), anything else as JSON.
fn num(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A JSON-quoted string, cut to [`MAX_TEXT`] characters with `…`.
fn quoted(s: &str) -> String {
    let cut: String = s.chars().take(MAX_TEXT).collect();
    let cut = if cut.len() < s.len() { format!("{cut}…") } else { cut };
    serde_json::to_string(&cut).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_frame_and_its_text_are_two_short_lines() {
        let v = json!({"id": 2, "name": "Login", "type": "FRAME", "x": 100, "y": 0, "width": 390, "height": 844,
            "fills": ["#FFFFFF"], "layout": {"mode": "vertical", "gap": 16, "padding": 24}, "sizing": {"horizontal": "fixed", "vertical": "hug"},
            "children": [{"id": 3, "name": "Title", "type": "TEXT", "x": 24, "y": 24, "width": 150, "height": 34,
                "absoluteBounds": {"x": 124, "y": 24, "width": 150, "height": 34},
                "characters": "Welcome back", "fontFamily": "Inter", "fontStyle": "Bold", "fontSize": 28, "textAutoResize": "widthAndHeight"}]});
        assert_eq!(
            tree(&v),
            vec![
                r##"2 FRAME "Login" 100,0 390x844 #FFFFFF vertical gap16 pad24 h:hug"##,
                r#"  3 TEXT "Title" 24,24 150x34 "Welcome back" Inter/Bold/28"#,
            ]
        );
    }

    #[test]
    fn what_a_new_layer_has_anyway_is_left_out() {
        let v = json!({"id": 6, "name": "Button", "type": "FRAME", "x": 0, "y": 0, "width": 342, "height": 48,
            "layout": {"mode": "horizontal", "gap": 0, "padding": 0, "mainAlign": "center"},
            "children": [{"id": 7, "name": "Sign in", "type": "TEXT", "x": 1, "y": 2, "width": 37, "height": 14,
                "sizing": {"horizontal": "hug", "vertical": "hug"}, "characters": "Sign in", "fontFamily": "Inter", "fontStyle": "Regular", "fontSize": 12}]});
        assert_eq!(tree(&v), vec![r#"6 FRAME "Button" 0,0 342x48 horizontal main:center"#, r#"  7 TEXT "Sign in" 1,2 37x14 Inter/Regular/12"#]);
    }

    #[test]
    fn values_without_a_token_are_named() {
        let v = json!({"id": 5, "name": "Card", "type": "INSTANCE", "x": 0, "y": 0, "width": 10, "height": 10,
            "effects": [{"type": "dropShadow"}], "mainComponent": 4, "mainComponentName": "Button", "overrides": {},
            "fills": [{"type": "linearGradient"}, "#000000"], "strokes": ["#111111"], "strokeWeight": 2, "strokeAlign": "center",
            "cornerRadius": [4, 4, 0, 0], "layout": {"mode": "grid", "gridColumns": 2, "columnTracks": [120, "1fr"]}, "childCount": 3});
        let mut v = v;
        v["constraints"] = json!({"horizontal": "leftRight", "vertical": "top"});
        let l = line(v.as_object().unwrap());
        assert_eq!(
            l,
            r##"5 INSTANCE "Card" 0,0 10x10 linearGradient+#000000 stroke#111111/2/center r4,4,0,0 grid gridColumns2 +layout.columnTracks pin:leftRight/top of:4 "Button" (3 children) +effects,overrides"##
        );
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary_and_quotes_are_escaped() {
        let s = "é\"".repeat(30);
        let q = quoted(&s);
        assert!(q.ends_with("…\""), "{q}");
        assert_eq!(q.matches('é').count(), 20);
        assert_eq!(quoted("a\"b"), r#""a\"b""#);
    }

    #[test]
    fn a_document_lists_its_pages_and_marks_the_current_one() {
        let d = json!({"name": "Untitled", "dirty": true, "currentPage": 1, "selection": [2], "variables": 0, "images": 0,
            "history": {"undo": ["Create frame"], "redo": 0},
            "styles": [{"id": "S:1", "name": "Brand", "type": "PAINT"}],
            "pages": [{"id": 1, "name": "Page 1", "type": "PAGE", "background": "#F5F5F5", "children": [{"id": 2, "name": "A", "type": "RECTANGLE", "x": 0, "y": 0, "width": 1, "height": 1}]},
                      {"id": 9, "name": "Page 2", "type": "PAGE", "childCount": 4}]});
        assert_eq!(
            document(&d),
            vec![
                r#"document "Untitled" selection [2] dirty"#,
                r#"history undo ["Create frame"] redo 0"#,
                r#"style S:1 PAINT "Brand""#,
                r##"page 1 "Page 1" bg#F5F5F5 current"##,
                r#"  2 RECTANGLE "A" 0,0 1x1"#,
                r#"page 9 "Page 2" (4 children)"#,
            ]
        );
    }

    #[test]
    fn odd_values_do_not_panic() {
        for v in [json!(null), json!([1]), json!({"children": "x", "layout": 3, "fills": {}, "id": [1]})] {
            let _ = tree(&v);
            let _ = document(&v);
        }
    }
}
