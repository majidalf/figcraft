//! `import.pen` on synthetic pen.dev node JSON, written from the schema pen.dev publishes.
use serde_json::{Value, json};

use crate::Session;

fn import(s: &mut Session, nodes: Value) -> Value {
    s.execute("import.pen", &json!({"nodes": nodes})).unwrap()
}

fn node(s: &Session, id: &Value) -> Value {
    crate::inspect::node_json(s.doc().unwrap(), crate::params::id_of(id).unwrap(), 8)
}

/// A login card as pen.dev's `Get` returns it (defaults left out), with resolved boxes.
fn card() -> Value {
    json!({"type": "frame", "name": "Card", "x": 40, "y": 60, "width": 360, "layout": "vertical", "gap": 12, "padding": [24, 20],
    "fill": "#FFFFFF", "cornerRadius": 16, "stroke": "#E4E7EC", "strokeWidth": 1, "strokeAlignment": "inner",
    "effect": {"type": "shadow", "offset": {"x": 0, "y": 4}, "blur": 12, "color": "#0000001A"},
    "b": [40, 60, 360, 170],
    "children": [
        {"type": "text", "name": "Title", "content": "Welcome back", "fontFamily": "Inter", "fontSize": 24, "fontWeight": "700", "fill": "#101828", "b": [20, 24, 170, 29]},
        {"type": "text", "name": "Body", "content": "Sign in to continue", "textGrowth": "fixed-width", "width": "fill_container", "fill": "#475467", "lineHeight": 1.5, "b": [20, 65, 320, 21]},
        {"type": "frame", "name": "Button", "width": "fill_container", "height": 44, "fill": "#2F5BE6", "cornerRadius": 8,
         "justifyContent": "center", "alignItems": "center", "b": [20, 98, 320, 44],
         "children": [{"type": "text", "name": "Label", "content": "Sign in", "fontSize": 14, "fontWeight": "500", "fill": "#FFFFFF", "b": [134, 13, 52, 18]}]},
    ]})
}

#[test]
fn a_card_comes_in_as_auto_layout_with_its_paints_and_text() {
    let mut s = Session::new();
    let r = import(&mut s, card());
    assert_eq!(r["layers"], 5, "{r}");
    assert_eq!(r["warnings"], json!([]), "{r}");
    let c = node(&s, &r["ids"][0]);
    assert_eq!((c["type"].as_str(), c["name"].as_str()), (Some("FRAME"), Some("Card")));
    assert_eq!((c["x"].as_f64(), c["y"].as_f64(), c["width"].as_f64()), (Some(40.0), Some(60.0), Some(360.0)));
    assert_eq!(c["layout"]["mode"], "vertical");
    assert_eq!(c["layout"]["gap"], 12.0);
    assert_eq!(c["layout"]["padding"], json!([24.0, 20.0, 24.0, 20.0]));
    assert_eq!(c["sizing"]["vertical"], "hug", "no height in pen.dev: fit_content");
    assert_eq!(c["clip"], false, "pen.dev frames don't clip by default");
    assert_eq!(c["fills"][0]["color"], "#FFFFFF");
    assert_eq!((c["strokes"][0]["color"].as_str(), c["strokeAlign"].as_str()), (Some("#E4E7EC"), Some("inside")));
    assert_eq!(c["effects"][0]["type"], "dropShadow");
    assert_eq!(c["cornerRadius"], 16.0);

    assert_eq!(c["layout"]["strokesIncluded"], false, "pen.dev's stroke doesn't push children in");
    assert_eq!((c["children"][0]["x"].as_f64(), c["children"][0]["y"].as_f64()), (Some(20.0), Some(24.0)));
    let title = &c["children"][0];
    assert_eq!(
        (title["characters"].as_str(), title["fontStyle"].as_str(), title["fontSize"].as_f64()),
        (Some("Welcome back"), Some("Bold"), Some(24.0))
    );
    assert_eq!(title["fills"][0]["color"], "#101828");
    let body = &c["children"][1];
    assert_eq!((body["textAutoResize"].as_str(), body["sizing"]["horizontal"].as_str()), (Some("height"), Some("fill")));
    assert_eq!(body["lineHeight"], "150%");
    let button = &c["children"][2];
    assert_eq!(
        (button["layout"]["mode"].as_str(), button["layout"]["mainAlign"].as_str()),
        (Some("horizontal"), Some("center")),
        "no `layout`: horizontal"
    );
    assert_eq!((button["sizing"]["horizontal"].as_str(), button["height"].as_f64()), (Some("fill"), Some(44.0)));
    assert_eq!(button["children"][0]["fontStyle"], "Medium");

    // One undo step, and the root is selected.
    assert_eq!(s.ds().unwrap().selection, vec![crate::params::id_of(&r["ids"][0]).unwrap()]);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().node(crate::params::id_of(&r["ids"][0]).unwrap()).is_none());
}

#[test]
fn absolute_children_shapes_and_groups_keep_their_places() {
    let mut s = Session::new();
    let r = import(
        &mut s,
        json!({"type": "frame", "name": "Canvas", "layout": "none", "width": 400, "height": 300, "fill": "#F2F4F7", "clip": true, "children": [
            {"type": "ellipse", "name": "Ring", "x": 10, "y": 20, "width": 80, "height": 80, "fill": "#12B76A", "innerRadius": 0.6},
            {"type": "polygon", "name": "Tri", "x": 120, "y": 20, "width": 60, "height": 60, "polygonCount": 3, "fill": "#F79009"},
            {"type": "group", "name": "Pair", "b": [200, 150, 120, 40], "children": [
                {"type": "rectangle", "name": "A", "x": 0, "y": 0, "width": 40, "height": 40, "fill": "#000"},
                {"type": "rectangle", "name": "B", "x": 80, "y": 0, "width": 40, "height": 40, "fill": {"type": "color", "color": "#F00", "enabled": false}}]},
        ]}),
    );
    let c = node(&s, &r["ids"][0]);
    assert!(c.get("layout").is_none(), "layout none: {c}");
    assert_eq!(c["clip"], true);
    let ring = &c["children"][0];
    assert_eq!((ring["x"].as_f64(), ring["y"].as_f64(), ring["arc"]["inner"].as_f64()), (Some(10.0), Some(20.0), Some(0.6)));
    assert_eq!(c["children"][1]["pointCount"], 3);
    let pair = &c["children"][2];
    assert_eq!((pair["x"].as_f64(), pair["width"].as_f64(), pair["fills"].as_array().map(Vec::len)), (Some(200.0), Some(120.0), None));
    assert_eq!(pair["children"][1]["x"], 80.0);
    assert_eq!(pair["children"][0]["fills"][0]["color"], "#000000", "#RGB is expanded");
    assert_eq!(pair["children"][1]["fills"][0]["visible"], false);
}

#[test]
fn a_paths_view_box_maps_onto_its_box() {
    let mut s = Session::new();
    // A 12×12 square in the middle of a 24×24 viewBox, drawn into a 48×48 box at (100, 100).
    let r = import(
        &mut s,
        json!({"type": "path", "name": "Glyph", "x": 100, "y": 100, "width": 48, "height": 48, "geometry": "M6 6 H18 V18 H6 Z", "viewBox": [0, 0, 24, 24], "fill": "#111111"}),
    );
    let v = node(&s, &r["ids"][0]);
    assert_eq!(v["type"], "VECTOR");
    assert_eq!(
        (v["x"].as_f64(), v["y"].as_f64(), v["width"].as_f64(), v["height"].as_f64()),
        (Some(112.0), Some(112.0), Some(24.0), Some(24.0)),
        "{v}"
    );
}

#[test]
fn gradients_point_where_pen_dev_draws_them() {
    let mut s = Session::new();
    let fill = |rot: f64| json!({"type": "gradient", "gradientType": "linear", "rotation": rot, "colors": [{"color": "#000000", "position": 0}, {"color": "#FFFFFF", "position": 1}]});
    let r = import(
        &mut s,
        json!([
            {"type": "rectangle", "name": "Up", "width": 10, "height": 10, "fill": fill(0.0)},
            {"type": "rectangle", "name": "Down", "x": 20, "width": 10, "height": 10, "fill": fill(180.0)},
        ]),
    );
    let ids = r["ids"].as_array().unwrap().clone();
    let doc = s.doc().unwrap();
    let ends = |id: &Value| {
        let n = doc.node(crate::params::id_of(id).unwrap()).unwrap();
        let p = serde_json::to_value(&n.fills[0]).unwrap();
        (p["from"][1].as_f64().unwrap(), p["to"][1].as_f64().unwrap())
    };
    let (up_from, up_to) = ends(&ids[0]);
    assert!(up_from > up_to, "0° runs upward: from y {up_from} to y {up_to}");
    let (down_from, down_to) = ends(&ids[1]);
    assert!(down_from < down_to, "180° runs downward");
}

#[test]
fn what_has_no_equivalent_is_named_in_warnings() {
    let mut s = Session::new();
    let r = import(
        &mut s,
        json!({"type": "frame", "name": "Screen", "width": 100, "height": 100, "children": [
            {"type": "icon", "name": "Logo", "width": 16, "height": 16, "library": "lucide", "icon": "no-such-icon", "fill": "#000"},
            {"type": "rectangle", "name": "Photo", "width": 40, "height": 40, "fill": {"type": "image", "url": "./hero.jpg"}},
            {"type": "ref", "ref": "abc", "name": "Button"},
            {"type": "note", "name": "Todo", "content": "x"},
            {"type": "text", "name": "Hi", "content": "Hi", "fill": "$text-primary"},
        ]}),
    );
    let w = r["warnings"].to_string();
    for want in ["lucide icon `no-such-icon`", "image fill ./hero.jpg", "resolveInstances", "note \\\"Todo\\\"", "$text-primary"] {
        assert!(w.contains(want), "{want} in {w}");
    }
    let c = node(&s, &r["ids"][0]);
    assert_eq!(c["children"].as_array().unwrap().len(), 3, "the icon, the photo and the text; the ref and the note are left out");
    assert_eq!(c["children"][0]["name"], "Logo (icon no-such-icon)");
    assert_eq!(c["children"][2].get("fills"), None, "an unresolved fill is no fill");
}

#[test]
fn pen_dev_icons_come_in_from_the_bundled_libraries() {
    let mut s = Session::new();
    let r = import(
        &mut s,
        json!({"type": "frame", "name": "Toolbar", "gap": 8, "children": [
            {"type": "icon", "name": "Find", "width": 20, "height": 20, "library": "lucide", "icon": "search", "fill": "#3366FF"},
            {"type": "icon", "name": "Edit", "width": 20, "height": 20, "library": "feather", "icon": "edit-2", "fill": "#000"},
            {"type": "icon", "name": "Add", "width": 20, "height": 20, "library": "phosphor", "icon": "plus-bold", "fill": "#000"},
            {"type": "icon", "name": "Back", "width": 24, "height": 24, "library": "Material Symbols Rounded", "icon": "arrow-back", "fill": [{"type": "color", "color": "#101828"}]},
        ]}),
    );
    assert_eq!(r["warnings"], json!([]), "{r}");
    let c = node(&s, &r["ids"][0]);
    let kids = c["children"].as_array().unwrap();
    let names: Vec<&str> = kids.iter().map(|k| k["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Find", "Edit", "Add", "Back"]);
    assert_eq!((kids[0]["width"].as_f64(), kids[3]["width"].as_f64()), (Some(20.0), Some(24.0)));
    assert!(kids[0].to_string().contains("#3366FF"), "the icon takes its fill colour: {}", kids[0]);
    assert!(kids.iter().all(|k| k["children"].as_array().is_some_and(|c| !c.is_empty())), "drawn, not empty frames");
}

#[test]
fn roots_go_where_asked_and_bad_input_is_an_error() {
    let mut s = Session::new();
    let r = s
        .execute(
            "import.pen",
            &json!({"x": 0, "y": 0, "nodes": [
            {"type": "rectangle", "name": "A", "x": 500, "y": 500, "width": 10, "height": 10},
            {"type": "rectangle", "name": "B", "x": 520, "y": 500, "width": 10, "height": 10}]}),
        )
        .unwrap();
    let xs: Vec<f64> = r["ids"].as_array().unwrap().iter().map(|id| node(&s, id)["x"].as_f64().unwrap()).collect();
    assert_eq!(xs, vec![0.0, 20.0]);
    for bad in [json!({}), json!({"nodes": 3}), json!({"nodes": [{"type": "note"}]}), json!({"nodes": {"type": "frame"}, "parent": 999_999})] {
        assert!(s.execute("import.pen", &bad).is_err(), "{bad}");
    }
    // Odd values are skipped, not fatal.
    let r = s
        .execute(
            "import.pen",
            &json!({"nodes": {"type": "frame", "children": [
            {"type": "text", "content": "x", "fontSize": "huge", "b": [1, "x"]},
            {"type": "rectangle", "cornerRadius": [1, 2], "fill": [3, {"type": "gradient", "colors": []}], "effect": [{"type": "shader"}, 4]},
            {"type": "path", "geometry": "not a path"},
            {"type": "frame", "padding": [1, 2, 3], "children": "x"}]}}),
        )
        .unwrap();
    assert_eq!(r["layers"], 5, "{r}");
}

#[test]
fn the_reserved_step_key_in_pen_data_cannot_reach_the_batch() {
    // A pen.dev text that happens to hold `{"$step": …}`-like data is only ever a string here.
    let mut s = Session::new();
    let r = import(&mut s, json!({"type": "text", "name": "T", "content": "{\"$step\": 0}", "fill": "#000"}));
    assert_eq!(node(&s, &r["ids"][0])["characters"], "{\"$step\": 0}");
}
