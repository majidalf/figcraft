//! The icon libraries FigCraft ships: Lucide (ISC), Phosphor (MIT) and Material Symbols
//! (Apache-2.0), packed by `cargo xtask icon-sets` into `assets/icon-sets/` (format in
//! `xtask/src/icon_sets.rs`; sources and licences in ATTRIBUTION.md). A pack is read the first time
//! its set is used. `icon.insert` turns an icon into an editable frame of vectors through the SVG
//! importer, at the size and colour asked for.

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::params::{boolean, get, id, num, string};
use crate::{EngineError, Result, Session, bad, spec};

/// Set ids and their packs, in the order pickers list them.
const PACKS: &[(&str, &[u8])] = &[
    ("lucide", include_bytes!("../../../assets/icon-sets/lucide.txt.gz")),
    ("phosphor", include_bytes!("../../../assets/icon-sets/phosphor.txt.gz")),
    ("material", include_bytes!("../../../assets/icon-sets/material.txt.gz")),
];

/// Largest decompressed pack accepted (the biggest is under 9 MB).
const MAX_PACK: u64 = 64 << 20;
/// Default and largest number of search results.
const SEARCH_LIMIT: usize = 50;
const SEARCH_MAX: usize = 2000;

pub struct IconSet {
    pub id: &'static str,
    pub title: String,
    pub package: String,
    pub licence: String,
    pub homepage: String,
    pub view_box: String,
    attrs: String,
    /// Style names; empty for a set with one style.
    pub styles: Vec<String>,
    pub icons: Vec<Icon>,
}

pub struct Icon {
    pub name: String,
    pub tags: Vec<String>,
    /// One SVG body per style (the single body when the set has no styles); empty: no such style.
    bodies: Vec<String>,
}

impl IconSet {
    /// The icon's SVG body in `style` (default: the set's first).
    pub fn body<'a>(&self, icon: &'a Icon, style: Option<&str>) -> Option<&'a str> {
        let i = match style.filter(|s| !s.is_empty()) {
            None => 0,
            Some(s) => self.styles.iter().position(|x| x == s)?,
        };
        icon.bodies.get(i).map(String::as_str).filter(|b| !b.is_empty())
    }

    pub fn icon(&self, name: &str) -> Option<&Icon> {
        self.icons.binary_search_by(|i| i.name.as_str().cmp(name)).ok().and_then(|i| self.icons.get(i))
    }

    /// A standalone SVG of the icon at `w`×`h` in `color` (`#RRGGBB[AA]`).
    pub fn svg(&self, icon: &Icon, style: Option<&str>, w: f64, h: f64, color: &str) -> Option<String> {
        let body = self.body(icon, style)?;
        let attrs = self.attrs.replace("currentColor", color);
        let body = body.replace("currentColor", color);
        Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{}" {attrs}>{body}</svg>"#, self.view_box))
    }
}

/// A one-icon document for a picker's preview: the icon at `px` pixels in `color`, and its frame.
pub fn preview(set: &IconSet, icon: &Icon, style: Option<&str>, px: f64, color: &str) -> Option<(figcraft_doc::Document, figcraft_doc::NodeId)> {
    let svg = set.svg(icon, style, px, px, color)?;
    let mut doc = figcraft_doc::Document::new("icon");
    let page = doc.first_page()?;
    let frame = figcraft_svg::import_svg_with_notes(&mut doc, svg.as_bytes(), page, 0.0, 0.0, &icon.name).ok()?.frame;
    if let Some(n) = doc.node_mut(frame) {
        n.clip = false;
        n.fills.clear();
    }
    Some((doc, frame))
}

/// Every set, read on first use. A pack that fails to read is left out (and logged).
pub fn sets() -> &'static [IconSet] {
    static SETS: OnceLock<Vec<IconSet>> = OnceLock::new();
    SETS.get_or_init(|| {
        PACKS
            .iter()
            .filter_map(|(id, bytes)| match parse(id, bytes) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("icon set {id}: {e}");
                    None
                }
            })
            .collect()
    })
}

pub fn set(id: &str) -> Option<&'static IconSet> {
    sets().iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

fn parse(id: &'static str, gz: &[u8]) -> std::result::Result<IconSet, String> {
    use std::io::Read as _;
    let mut text = String::new();
    flate2::read::GzDecoder::new(gz).take(MAX_PACK).read_to_string(&mut text).map_err(|e| e.to_string())?;
    let (head, body) = text.split_once("\n---\n").ok_or("no `---` after the header")?;
    let field = |k: &str| head.lines().find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('\t'))).unwrap_or("").to_string();
    let styles: Vec<String> = field("styles").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    let mut icons = Vec::new();
    for line in body.lines() {
        let mut cols = line.split('\t');
        let (Some(name), Some(tags)) = (cols.next(), cols.next()) else { continue };
        let mut bodies: Vec<String> = Vec::new();
        for c in cols {
            let b = if c == "=" { bodies.last().cloned().unwrap_or_default() } else { c.to_string() };
            bodies.push(b);
        }
        icons.push(Icon { name: name.to_string(), tags: tags.split(',').filter(|t| !t.is_empty()).map(str::to_string).collect(), bodies });
    }
    icons.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(IconSet {
        id,
        title: field("title"),
        package: field("package"),
        licence: field("licence"),
        homepage: field("homepage"),
        view_box: field("viewBox"),
        attrs: field("attrs"),
        styles,
        icons,
    })
}

/// Icons matching `query` (all words, in names or tags), best first: an exact name, then names
/// starting with the query, then names containing it, then tags.
pub fn search(query: &str, set_id: Option<&str>, limit: usize) -> Vec<(&'static IconSet, &'static Icon)> {
    let q = query.trim().to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let slug = words.join("-");
    let mut hits: Vec<(u8, &IconSet, &Icon)> = Vec::new();
    for s in sets().iter().filter(|s| set_id.is_none_or(|id| s.id.eq_ignore_ascii_case(id))) {
        for i in &s.icons {
            let name = i.name.replace('_', "-");
            let rank = if words.is_empty() || name == slug {
                0
            } else if name.starts_with(&slug) {
                1
            } else if name.contains(&slug) {
                2
            } else if words.iter().all(|w| name.contains(w) || i.tags.iter().any(|t| t.contains(w))) {
                3
            } else {
                continue;
            };
            hits.push((rank, s, i));
        }
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.name.len().cmp(&b.2.name.len())).then(a.2.name.cmp(&b.2.name)));
    hits.into_iter().take(limit).map(|(_, s, i)| (s, i)).collect()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("icon.sets", "Icon libraries", "{}  → {sets: [{id, title, package, licence, homepage, styles, count}]}", false, |_, _| {
            let sets: Vec<Value> = sets()
                .iter()
                .map(|s| json!({"id": s.id, "title": s.title, "package": s.package, "licence": s.licence, "homepage": s.homepage, "styles": s.styles, "count": s.icons.len()}))
                .collect();
            Ok(json!({"sets": sets}))
        }),
        spec!(
            "icon.search",
            "Search icons",
            "{query? (words matched against names and tags; empty: every icon), set? (lucide|phosphor|material), limit? (default 50, max 2000)}  → {icons: [{set, name, tags, styles}]}",
            false,
            |_, p| {
                let limit = num(p, "limit").map_or(SEARCH_LIMIT, |l| l.clamp(1.0, SEARCH_MAX as f64) as usize);
                let set_id = string(p, "set");
                if let Some(id) = &set_id
                    && set(id).is_none()
                {
                    return Err(bad("icon.search", format!("no icon set `{id}` (lucide, phosphor, material)")));
                }
                let icons: Vec<Value> = search(&string(p, "query").unwrap_or_default(), set_id.as_deref(), limit)
                    .into_iter()
                    .map(|(s, i)| {
                        let styles: Vec<&String> = s.styles.iter().filter(|st| s.body(i, Some(st)).is_some()).collect();
                        json!({"set": s.id, "name": i.name, "tags": i.tags, "styles": styles})
                    })
                    .collect();
                Ok(json!({"icons": icons}))
            }
        ),
        spec!(
            "icon.insert",
            "Insert icon",
            "{set (lucide|phosphor|material), name, style? (phosphor: regular|thin|light|bold|fill|duotone; material: outlined|rounded|sharp, each with -fill; default the first), size? (default 24) | width?, height?, color? (#RRGGBB[AA] or a CSS colour; default #000000), x?, y?, parent? (id; default the page), select? (default true)}  an editable frame of vectors named set/name → {id}",
            true,
            insert
        ),
    ]
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "icon.insert";
    let set_id = string(p, "set").ok_or_else(|| bad(CMD, "`set` is required (lucide, phosphor, material)"))?;
    let set = set(&set_id).ok_or_else(|| bad(CMD, format!("no icon set `{set_id}` (lucide, phosphor, material)")))?;
    let name = string(p, "name").ok_or_else(|| bad(CMD, "`name` is required (icon.search finds names)"))?;
    let icon = set.icon(&name).ok_or_else(|| bad(CMD, format!("no icon `{name}` in {} (icon.search finds names)", set.id)))?;
    let style = string(p, "style");
    if let Some(st) = &style
        && !st.is_empty()
        && !set.styles.contains(st)
    {
        return Err(bad(
            CMD,
            format!("{} has no style `{st}` ({})", set.id, if set.styles.is_empty() { "it has one style".into() } else { set.styles.join(", ") }),
        ));
    }
    let size = num(p, "size").unwrap_or(24.0);
    let (w, h) = (num(p, "width").unwrap_or(size), num(p, "height").unwrap_or(size));
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 && w <= 100_000.0 && h <= 100_000.0) {
        return Err(bad(CMD, "`size`, `width` and `height` must be positive"));
    }
    let color = string(p, "color").unwrap_or_else(|| "#000000".into());
    let color = crate::params::color_of(&json!(color))
        .map(|c| format!("#{}", c.to_hex()))
        .ok_or_else(|| bad(CMD, "`color` must be #RRGGBB, #RRGGBBAA or a CSS colour"))?;
    let svg = set
        .svg(icon, style.as_deref(), w, h, &color)
        .ok_or_else(|| bad(CMD, format!("`{name}` has no {} style in {}", style.as_deref().unwrap_or("default"), set.id)))?;
    let parent = match get(p, "parent").filter(|v| !v.is_null()) {
        Some(_) => id(p, "parent").ok_or_else(|| bad(CMD, "`parent` must be a layer id"))?,
        None => s.page()?,
    };
    let (x, y) = (num(p, "x").unwrap_or(0.0), num(p, "y").unwrap_or(0.0));
    let doc = s.doc_mut()?;
    if !doc.contains(parent) {
        return Err(EngineError::NoNode(parent));
    }
    let label = format!("{}/{}", set.id, icon.name);
    let imported = figcraft_svg::import_svg_with_notes(doc, svg.as_bytes(), parent, x, y, &label).map_err(|e| EngineError::Other(e.to_string()))?;
    if let Some(n) = doc.node_mut(imported.frame) {
        // An icon's frame shows its whole drawing (strokes reach past the box at the edges).
        n.clip = false;
        n.fills.clear();
    }
    if boolean(p, "select").unwrap_or(true) {
        s.set_selection(vec![imported.frame]);
    }
    Ok(json!({"id": imported.frame}))
}
