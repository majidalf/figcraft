//! Compact inspection output for agents. Every byte a tool returns is read by the model as tokens,
//! so the engine's full layer JSON (made for exactness, not for reading) is cut down here before it
//! is sent: values a new layer has anyway are left out, a solid fill becomes its "#hex", uniform
//! padding becomes one number and whole floats lose their `.0`. Each left-out value is one a reader
//! can assume (see `docs/mcp.md`, "Compact output"); `verbose: true` on a tool skips all of this.
//!
//! Only the MCP layer slims: the engine, the control channel and the CLI keep the full JSON.

use serde_json::{Map, Number, Value};

/// Auto layout fields left out when they hold these values (the ones every new auto layout has).
const LAYOUT_DEFAULTS: &[(&str, &str)] = &[("mainAlign", "min"), ("crossAlign", "min"), ("contentAlign", "auto")];
const LAYOUT_FLAGS: &[(&str, bool)] = &[("wrap", false), ("reverseZ", false), ("strokesIncluded", true)];
/// Only meaningful for `mode: "grid"`.
const GRID_KEYS: &[&str] = &["gridColumns", "gridRows", "columnTracks", "rowTracks"];
/// Text fields left out when they hold a new text layer's value.
const TEXT_DEFAULTS: &[(&str, &str)] = &[("lineHeight", "Auto"), ("letterSpacing", "0%"), ("textAlign", "left")];
/// Document fields left out when they hold these values.
const DOC_DEFAULTS: &[(&str, &str)] = &[("colorProfile", "srgb")];

/// Slim a `document.inspect` result in place.
pub fn document(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        for k in ["path", "textEdit"] {
            if o.get(k).is_some_and(Value::is_null) {
                o.remove(k);
            }
        }
        if o.get("styles").and_then(Value::as_array).is_some_and(Vec::is_empty) {
            o.remove("styles");
        }
        drop_str_defaults(o, DOC_DEFAULTS);
        if let Some(Value::Array(pages)) = o.get_mut("pages") {
            pages.iter_mut().for_each(node);
        }
    }
    whole_numbers(v);
}

/// Slim a `node.get` result (a layer and its children) in place.
pub fn tree(v: &mut Value) {
    node(v);
    whole_numbers(v);
}

/// One layer and, recursively, its children.
fn node(v: &mut Value) {
    let Some(o) = v.as_object_mut() else { return };
    if bounds_match(o) {
        o.remove("absoluteBounds");
    }
    for k in ["fills", "strokes"] {
        if let Some(Value::Array(paints)) = o.get_mut(k) {
            paints.iter_mut().for_each(solid_to_hex);
        }
    }
    if let Some(Value::Object(l)) = o.get_mut("layout") {
        layout(l);
    }
    let fixed = |axis: &str| o.get("sizing").and_then(|s| s.get(axis)).and_then(Value::as_str) == Some("fixed");
    if fixed("horizontal") && fixed("vertical") {
        o.remove("sizing");
    }
    // Frames clip by default; only `clip: false` is news.
    if o.get("clip") == Some(&Value::Bool(true)) {
        o.remove("clip");
    }
    if o.get("type").and_then(Value::as_str) == Some("TEXT") {
        drop_str_defaults(o, TEXT_DEFAULTS);
    }
    if let Some(Value::Array(children)) = o.get_mut("children") {
        children.iter_mut().for_each(node);
    }
}

fn layout(l: &mut Map<String, Value>) {
    drop_str_defaults(l, LAYOUT_DEFAULTS);
    let wraps = l.get("wrap") == Some(&Value::Bool(true));
    for (k, default) in LAYOUT_FLAGS {
        if l.get(*k) == Some(&Value::Bool(*default)) {
            l.remove(*k);
        }
    }
    if !wraps {
        l.remove("crossGap");
    }
    if l.get("mode").and_then(Value::as_str) != Some("grid") {
        for k in GRID_KEYS {
            l.remove(*k);
        }
    } else {
        for k in ["columnTracks", "rowTracks"] {
            if l.get(k).and_then(Value::as_array).is_some_and(Vec::is_empty) {
                l.remove(k);
            }
        }
    }
    // `[24, 24, 24, 24]` → `24` (node.set takes either).
    if let Some(Value::Array(p)) = l.get("padding")
        && let Some(first) = p.first().cloned()
        && p.len() == 4
        && p.iter().all(|x| *x == first)
    {
        l.insert("padding".into(), first);
    }
}

/// `{"type": "solid", "color": "#hex"}` with nothing else set → `"#hex"` (fills/strokes take either).
fn solid_to_hex(p: &mut Value) {
    let Some(o) = p.as_object() else { return };
    if o.len() == 2
        && o.get("type").and_then(Value::as_str) == Some("solid")
        && let Some(c) = o.get("color").and_then(Value::as_str)
    {
        *p = Value::String(c.to_string());
    }
}

/// `absoluteBounds` repeats `x`/`y`/`width`/`height` for a layer on the page (or in a group on it).
fn bounds_match(o: &Map<String, Value>) -> bool {
    let Some(Value::Object(b)) = o.get("absoluteBounds") else { return false };
    ["x", "y", "width", "height"]
        .iter()
        .all(|k| matches!((b.get(*k).and_then(Value::as_f64), o.get(*k).and_then(Value::as_f64)), (Some(a), Some(c)) if a == c))
}

fn drop_str_defaults(o: &mut Map<String, Value>, defaults: &[(&str, &str)]) {
    for (k, default) in defaults {
        if o.get(*k).and_then(Value::as_str) == Some(*default) {
            o.remove(*k);
        }
    }
}

/// Exactly representable whole floats as integers: `24.0` → `24`.
fn whole_numbers(v: &mut Value) {
    const EXACT: f64 = 9_007_199_254_740_992.0; // 2^53
    match v {
        Value::Number(n) if n.is_f64() => {
            if let Some(f) = n.as_f64()
                && f.is_finite()
                && f.fract() == 0.0
                && f.abs() < EXACT
            {
                *n = Number::from(f as i64);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(whole_numbers),
        Value::Object(o) => o.values_mut().for_each(whole_numbers),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame() -> Value {
        json!({
            "absoluteBounds": {"height": 844.0, "width": 390.0, "x": 0.0, "y": 0.0},
            "clip": true,
            "fills": [{"color": "#FFFFFF", "type": "solid"}],
            "height": 844.0, "id": 2, "name": "Login", "type": "FRAME", "width": 390.0, "x": 0.0, "y": 0.0,
            "layout": {
                "columnTracks": [], "contentAlign": "auto", "crossAlign": "min", "crossGap": 10.0, "gap": 16.0,
                "gridColumns": 2, "gridRows": 2, "mainAlign": "min", "mode": "vertical",
                "padding": [24.0, 24.0, 24.0, 24.0], "reverseZ": false, "rowTracks": [], "strokesIncluded": true, "wrap": false
            },
            "sizing": {"horizontal": "fixed", "vertical": "fixed"},
        })
    }

    #[test]
    fn a_default_frame_keeps_only_what_differs_from_a_new_one() {
        let mut v = frame();
        tree(&mut v);
        assert_eq!(
            v,
            json!({"id": 2, "name": "Login", "type": "FRAME", "x": 0, "y": 0, "width": 390, "height": 844,
                   "fills": ["#FFFFFF"], "layout": {"mode": "vertical", "gap": 16, "padding": 24}})
        );
        let full = serde_json::to_string(&frame()).unwrap().len();
        let slim = serde_json::to_string(&v).unwrap().len();
        assert!(slim * 3 < full, "{slim} of {full} bytes");
    }

    #[test]
    fn values_that_differ_from_the_defaults_stay() {
        let mut v = frame();
        v["clip"] = json!(false);
        v["absoluteBounds"]["x"] = json!(40.5);
        v["sizing"]["vertical"] = json!("hug");
        v["fills"] = json!([{"color": "#FFFFFF", "type": "solid", "opacity": 0.5}]);
        v["layout"]["wrap"] = json!(true);
        v["layout"]["mainAlign"] = json!("spaceBetween");
        v["layout"]["padding"] = json!([24.0, 16.0, 24.0, 16.0]);
        tree(&mut v);
        assert_eq!(v["clip"], false);
        assert_eq!(v["absoluteBounds"]["x"], 40.5);
        assert_eq!(v["sizing"]["vertical"], "hug");
        assert_eq!(v["fills"][0]["opacity"], 0.5, "a paint with more than a colour stays an object");
        assert_eq!((v["layout"]["wrap"].clone(), v["layout"]["crossGap"].clone()), (json!(true), json!(10)));
        assert_eq!(v["layout"]["mainAlign"], "spaceBetween");
        assert_eq!(v["layout"]["padding"], json!([24, 16, 24, 16]));
    }

    #[test]
    fn grid_layouts_keep_their_grid_fields() {
        let mut v = frame();
        v["layout"]["mode"] = json!("grid");
        v["layout"]["columnTracks"] = json!([120.0, "1fr"]);
        tree(&mut v);
        assert_eq!(v["layout"]["gridColumns"], 2);
        assert_eq!(v["layout"]["columnTracks"], json!([120, "1fr"]));
        assert!(v["layout"].get("rowTracks").is_none());
    }

    #[test]
    fn text_defaults_go_and_children_are_slimmed_too() {
        let mut v = frame();
        v["children"] = json!([{
            "id": 3, "type": "TEXT", "characters": "Hi", "x": 24.0, "y": 24.0, "width": 20.0, "height": 19.0,
            "absoluteBounds": {"x": 24.0, "y": 24.0, "width": 20.0, "height": 19.0},
            "lineHeight": "Auto", "letterSpacing": "0%", "textAlign": "center", "fontSize": 16.0,
        }]);
        tree(&mut v);
        let t = &v["children"][0];
        assert!(t.get("lineHeight").is_none() && t.get("letterSpacing").is_none() && t.get("absoluteBounds").is_none(), "{t}");
        assert_eq!((t["textAlign"].clone(), t["fontSize"].clone()), (json!("center"), json!(16)));
    }

    #[test]
    fn documents_drop_empty_fields_and_slim_their_pages() {
        let mut d = json!({"name": "Untitled", "path": null, "textEdit": null, "styles": [], "colorProfile": "srgb",
                           "pages": [{"id": 1, "type": "PAGE", "children": [frame()]}]});
        document(&mut d);
        assert_eq!(
            d,
            json!({"name": "Untitled", "pages": [{"id": 1, "type": "PAGE", "children": [{
            "id": 2, "name": "Login", "type": "FRAME", "x": 0, "y": 0, "width": 390, "height": 844,
            "fills": ["#FFFFFF"], "layout": {"mode": "vertical", "gap": 16, "padding": 24}}]}]})
        );
    }

    #[test]
    fn odd_values_are_left_alone() {
        for mut v in [json!(null), json!([1.5, 1e300, "x"]), json!({"fills": "nope", "layout": 3, "children": {}})] {
            let before = v.clone();
            tree(&mut v);
            assert!(v.to_string().len() <= before.to_string().len(), "{v}");
        }
        let mut big = json!(1e300);
        whole_numbers(&mut big);
        assert_eq!(big, json!(1e300), "a float past 2^53 is not made an integer");
    }
}
