//! `create_tree` builds a nested layer tree in one call and one undo step.
use crate::common;

use common::Client;
use serde_json::{Value, json};

fn login() -> Value {
    json!({"type": "frame", "name": "Login", "width": 390, "height": 844,
    "props": {"fill": "#FFFFFF", "layout": "vertical", "padding": 24, "gap": 16},
    "children": [
        {"type": "text", "text": "Welcome back", "props": {"fontSize": 28}},
        {"type": "rectangle", "name": "Email", "width": 342, "height": 48, "props": {"fill": "#F1F3F5", "cornerRadius": 8}},
        {"type": "frame", "name": "Button", "width": 342, "height": 48,
         "props": {"fill": "#3B5BDB", "layout": "horizontal", "mainAlign": "center", "crossAlign": "center"},
         "children": [{"type": "text", "text": "Sign in", "props": {"fill": "#FFFFFF"}}]},
    ]})
}

#[test]
fn a_screen_is_one_call_and_one_undo_step() {
    let mut c = Client::new();
    let out = c.ok("create_tree", json!({"nodes": login()}));
    let out = out.as_str().unwrap().to_string();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5, "{out}");
    assert!(lines[0].contains(r#"FRAME "Login" 0,0 390x844 #FFFFFF vertical gap16 pad24"#), "{out}");
    assert!(lines[2].starts_with("  ") && lines[2].contains(r#"RECTANGLE "Email" 24,"#), "laid out by the frame: {out}");
    assert!(lines[4].starts_with("    ") && lines[4].contains(r#"TEXT "Sign in""#), "{out}");

    let root = c.page_children();
    assert_eq!(root.len(), 1);
    let frame = &root[0];
    assert_eq!(frame["children"][2]["children"][0]["characters"], "Sign in");
    let d = c.doc();
    assert_eq!(d["selection"], json!([frame["id"]]), "the root is selected");
    assert_eq!(d["history"]["undo"].as_array().unwrap().last().unwrap(), "Create layers");

    c.ok("undo", json!({}));
    assert!(c.page_children().is_empty(), "one undo removes the whole tree");
}

#[test]
fn roots_go_in_a_parent() {
    let mut c = Client::new();
    let host = c.create(json!({"type": "frame", "width": 800, "height": 600}));
    c.ok("create_tree", json!({"nodes": [{"type": "rectangle", "x": 10}, {"type": "ellipse", "x": 200}], "parent": host, "label": "Shapes"}));
    let n = c.node(&host);
    assert_eq!(n["children"].as_array().unwrap().len(), 2);
    assert_eq!(c.doc()["selection"].as_array().unwrap().len(), 2);
}

#[test]
fn a_failing_layer_creates_nothing_and_is_named() {
    let mut c = Client::new();
    let mut t = login();
    t["children"][1]["props"] = json!({"noSuchProperty": 1});
    let e = c.err("create_tree", json!({"nodes": t}));
    assert!(e.contains("noSuchProperty") && e.contains("at nodes[0].children[1]; nothing was created"), "{e}");
    assert!(c.page_children().is_empty(), "rolled back: {e}");

    assert!(c.err("create_tree", json!({"nodes": {"type": "frame", "fill": "#000"}})).contains("unknown key `fill`"));
    assert!(c.err("create_tree", json!({})).contains("`nodes`"));
    assert!(c.err("create_tree", json!({"nodes": {"type": "frame"}, "parent": "nope"})).contains("not a layer id"));
}
