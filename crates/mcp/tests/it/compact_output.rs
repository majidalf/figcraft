//! Tool results are read by the model as tokens, so they are compact by default: no indentation,
//! no values a new layer has anyway, short replies to edits. `verbose: true` gives the full JSON.
use crate::common;

use common::Client;
use serde_json::{Value, json};

fn text(c: &mut Client, tool: &str, args: Value) -> String {
    let r = c.call(tool, args.clone());
    assert_eq!(r["isError"], false, "{tool} {args}: {}", r["content"][0]["text"]);
    r["content"][0]["text"].as_str().unwrap().to_string()
}

fn login(c: &mut Client) -> Value {
    let frame = c.create(json!({"type": "frame", "name": "Login", "x": 100, "width": 390, "height": 844,
        "props": {"fill": "#FFFFFF", "layout": "vertical", "padding": 24, "gap": 16}}));
    c.create(json!({"type": "text", "parent": frame, "text": "Welcome back", "props": {"fontSize": 28}}));
    frame
}

#[test]
fn create_node_replies_with_the_layers_box() {
    let mut c = Client::new();
    let frame = login(&mut c);
    let r = c.ok("create_node", json!({"type": "rectangle", "parent": frame, "width": 342, "height": 48, "props": {"fill": "#3B5BDB"}}));
    let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys.len(), 5, "only the id and the box: {r}");
    // The box is where the layout put it (under the title, 16 apart), not where it was asked for.
    let title = c.node(&frame)["children"][0].clone();
    let below = title["y"].as_f64().unwrap() + title["height"].as_f64().unwrap() + 16.0;
    assert_eq!((r["x"].as_f64(), r["y"].as_f64(), r["width"].as_f64()), (Some(24.0), Some(below), Some(342.0)), "{r}");

    let full = c.ok("create_node", json!({"type": "rectangle", "verbose": true}));
    assert!(full["node"]["fills"].is_array(), "verbose returns the whole layer: {full}");
}

#[test]
fn set_properties_replies_with_the_boxes() {
    let mut c = Client::new();
    let frame = login(&mut c);
    let r = c.ok("set_properties", json!({"ids": [frame], "props": {"sizingV": "hug"}}));
    assert_eq!(r["ok"], true);
    assert_eq!(r["nodes"][0]["id"], frame);
    let title_h = c.node(&frame)["children"][0]["height"].as_f64().unwrap();
    assert_eq!(r["nodes"][0]["height"].as_f64(), Some(24.0 + title_h + 24.0), "the hugged height is read back: {r}");
    assert!(r["nodes"][0].get("layout").is_none(), "{r}");
    let full = c.ok("set_properties", json!({"ids": [frame], "props": {"gap": 8}, "verbose": true}));
    assert_eq!(full["nodes"][0]["layout"]["gap"], 8.0, "{full}");
}

#[test]
fn inspect_is_compact_and_verbose_spells_it_out() {
    let mut c = Client::new();
    let frame = login(&mut c);
    let compact = text(&mut c, "get_node", json!({"id": frame, "depth": 1}));
    let verbose = text(&mut c, "get_node", json!({"id": frame, "depth": 1, "verbose": true}));
    assert!(!compact.contains('\n') && !verbose.contains('\n'), "no indentation");
    // Against what was sent before: the full JSON, indented.
    let before = |v: &str| serde_json::to_string_pretty(&serde_json::from_str::<Value>(v).unwrap()).unwrap().len();
    assert!(compact.len() * 3 < before(&verbose), "{} vs {} bytes: {compact}", compact.len(), before(&verbose));

    let n: Value = serde_json::from_str(&compact).unwrap();
    assert_eq!(n["fills"], json!(["#FFFFFF"]));
    assert_eq!(n["layout"], json!({"mode": "vertical", "gap": 16, "padding": 24}));
    assert!(n.get("clip").is_none() && n.get("sizing").is_none() && n.get("absoluteBounds").is_none(), "{n}");
    // A child keeps its page position: it is not the same as its x/y in the frame.
    assert_eq!(n["children"][0]["x"], 24, "{n}");
    assert_eq!(n["children"][0]["absoluteBounds"]["x"], 124, "{n}");

    let d = text(&mut c, "inspect_document", json!({}));
    let dv = text(&mut c, "inspect_document", json!({"verbose": true}));
    assert!(d.len() < dv.len() && d.len() * 2 < before(&dv), "{} vs {} bytes", d.len(), before(&dv));
}

/// The compact form uses the keys and values `set_properties` takes, so an agent can copy it.
#[test]
fn compact_values_can_be_set_back() {
    let mut c = Client::new();
    let frame = login(&mut c);
    let n = c.ok("get_node", json!({"id": frame}));
    let other = c.create(json!({"type": "frame", "x": 600, "width": 390, "height": 844}));
    c.ok(
        "set_properties",
        json!({"ids": [other], "props": {"fills": n["fills"], "layout": n["layout"]["mode"],
        "padding": n["layout"]["padding"], "gap": n["layout"]["gap"]}}),
    );
    let (a, b) = (c.node(&frame), c.node(&other));
    assert_eq!(a["fills"], b["fills"]);
    assert_eq!(a["layout"], b["layout"]);
}

#[test]
fn outline_is_one_line_per_layer() {
    let mut c = Client::new();
    let frame = login(&mut c);
    let o = text(&mut c, "get_node", json!({"id": frame, "depth": 1, "format": "outline"}));
    let lines: Vec<&str> = o.lines().collect();
    assert_eq!(lines.len(), 2, "{o}");
    assert_eq!(lines[0], format!(r##"{frame} FRAME "Login" 100,0 390x844 #FFFFFF vertical gap16 pad24"##));
    assert!(lines[1].starts_with("  ") && lines[1].contains(r#"TEXT "Welcome back" 24,24"#) && lines[1].contains("\"Welcome back\" "), "{o}");
    let json = text(&mut c, "get_node", json!({"id": frame, "depth": 1}));
    assert!(o.len() * 2 < json.len(), "{} vs {} bytes", o.len(), json.len());

    let d = text(&mut c, "inspect_document", json!({"format": "outline"}));
    assert!(d.starts_with("document ") && d.contains(" current\n") && d.contains(&format!("\n  {frame} FRAME \"Login\"")), "{d}");
}

#[test]
fn outline_pages_children_and_stays_within_max_bytes() {
    let mut c = Client::new();
    let frame = c.create(json!({"type": "frame", "width": 400, "height": 400}));
    for i in 0..60 {
        c.create(json!({"type": "rectangle", "parent": frame, "name": format!("r{i}")}));
    }
    let o = text(&mut c, "get_node", json!({"id": frame, "format": "outline", "offset": 58}));
    assert_eq!(o.lines().count(), 4, "the frame, the skipped note and two children: {o}");
    assert!(o.contains("58 children before") && o.contains("\"r59\""), "{o}");

    let cut = text(&mut c, "get_node", json!({"id": frame, "format": "outline", "maxBytes": 2048}));
    assert!(cut.len() <= 2048 && cut.lines().last().unwrap().contains("lines left out"), "{cut}");
}

#[test]
fn outline_rejects_bad_format_and_verbose() {
    let mut c = Client::new();
    assert!(c.err("inspect_document", json!({"format": "yaml"})).contains("`format`"));
    assert!(c.err("inspect_document", json!({"format": "outline", "verbose": true})).contains("`verbose`"));
    assert!(c.err("get_node", json!({"id": 999_999, "format": "outline"})).contains("999999"));
}

#[test]
fn verbose_must_be_a_boolean() {
    let mut c = Client::new();
    assert!(c.err("inspect_document", json!({"verbose": "yes"})).contains("`verbose`"));
}
