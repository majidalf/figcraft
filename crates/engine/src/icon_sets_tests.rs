//! The bundled icon libraries: packs read, search, insert as editable vectors.
use serde_json::{Value, json};

use crate::Session;
use crate::icon_sets::{search, set, sets};

fn node(s: &Session, id: &Value) -> Value {
    crate::inspect::node_json(s.doc().unwrap(), crate::params::id_of(id).unwrap(), 4)
}

#[test]
fn the_three_libraries_load_with_their_licences() {
    let ids: Vec<&str> = sets().iter().map(|s| s.id).collect();
    assert_eq!(ids, ["lucide", "phosphor", "material"]);
    let want = [("lucide", "ISC", 2000), ("phosphor", "MIT", 1500), ("material", "Apache-2.0", 3800)];
    for (id, licence, at_least) in want {
        let s = set(id).unwrap();
        assert_eq!(s.licence, licence);
        assert!(s.icons.len() >= at_least, "{id}: {}", s.icons.len());
        assert!(s.icons.windows(2).all(|w| w[0].name < w[1].name), "{id} is sorted for lookup");
    }
    assert_eq!(set("phosphor").unwrap().styles, ["regular", "thin", "light", "bold", "fill", "duotone"]);
    assert!(set("lucide").unwrap().styles.is_empty());
    // `=` in the pack is the style before it: Material's search is the same outlined and filled.
    let m = set("material").unwrap();
    let i = m.icon("search").unwrap();
    assert_eq!(m.body(i, Some("outlined")), m.body(i, Some("outlined-fill")));
    assert!(m.body(i, Some("outlined")).unwrap().starts_with("<path"));
}

#[test]
fn search_ranks_names_before_tags() {
    let hits = search("search", Some("lucide"), 5);
    assert_eq!(hits.first().map(|(_, i)| i.name.as_str()), Some("search"));
    // "magnifying glass" is a tag of Lucide's search, and the name of Phosphor's.
    let hits = search("magnifying glass", None, 50);
    assert!(hits.iter().any(|(s, i)| s.id == "phosphor" && i.name == "magnifying-glass"));
    assert!(hits.iter().any(|(s, i)| s.id == "lucide" && i.name == "search"));
    assert!(search("zzzz-no-such-icon", None, 5).is_empty());
    assert_eq!(search("", Some("lucide"), 7).len(), 7);
}

#[test]
fn an_inserted_icon_is_an_editable_frame_at_its_size_and_colour() {
    let mut s = Session::new();
    let r = s.execute("icon.insert", &json!({"set": "lucide", "name": "search", "size": 48, "color": "#3366FF", "x": 10, "y": 20})).unwrap();
    let n = node(&s, &r["id"]);
    assert_eq!((n["name"].as_str(), n["x"].as_f64(), n["y"].as_f64()), (Some("lucide/search"), Some(10.0), Some(20.0)));
    assert_eq!((n["width"].as_f64(), n["height"].as_f64()), (Some(48.0), Some(48.0)));
    assert!(n.get("fills").is_none() && n["clip"] == false, "{n}");
    // Lucide draws with strokes: the circle and the handle, in the colour asked for.
    let text = n.to_string();
    assert!(text.contains("#3366FF"), "{text}");
    assert!(n["children"].as_array().is_some_and(|c| !c.is_empty()), "{n}");
    assert_eq!(s.ds().unwrap().selection, vec![crate::params::id_of(&r["id"]).unwrap()]);

    let r = s.execute("icon.insert", &json!({"set": "phosphor", "name": "magnifying-glass", "style": "duotone", "color": "#000"})).unwrap();
    assert_eq!(node(&s, &r["id"])["width"], 24.0);
    let r = s.execute("icon.insert", &json!({"set": "material", "name": "search", "style": "rounded-fill", "width": 32, "height": 20})).unwrap();
    assert_eq!((node(&s, &r["id"])["width"].as_f64(), node(&s, &r["id"])["height"].as_f64()), (Some(32.0), Some(20.0)));
}

#[test]
fn icons_go_into_auto_layout_and_undo_in_one_step() {
    let mut s = Session::new();
    let frame = s.execute("node.create", &json!({"type": "frame", "props": {"layout": "horizontal", "gap": 8, "padding": 8}})).unwrap()["id"].clone();
    s.execute("icon.insert", &json!({"set": "lucide", "name": "plus", "parent": frame})).unwrap();
    let f = node(&s, &frame);
    assert_eq!(f["children"][0]["name"], "lucide/plus");
    assert_eq!((f["children"][0]["x"].as_f64(), f["children"][0]["y"].as_f64()), (Some(8.0), Some(8.0)));
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(node(&s, &frame).get("children").is_none());
}

#[test]
fn bad_requests_say_what_is_wrong() {
    let mut s = Session::new();
    for (p, want) in [
        (json!({}), "`set`"),
        (json!({"set": "fontawesome", "name": "x"}), "no icon set"),
        (json!({"set": "lucide"}), "`name`"),
        (json!({"set": "lucide", "name": "no-such-icon"}), "no icon `no-such-icon`"),
        (json!({"set": "lucide", "name": "search", "style": "bold"}), "one style"),
        (json!({"set": "phosphor", "name": "acorn", "style": "heavy"}), "no style `heavy`"),
        (json!({"set": "lucide", "name": "search", "size": -1}), "positive"),
        (json!({"set": "lucide", "name": "search", "color": "not a colour"}), "`color`"),
        (json!({"set": "lucide", "name": "search", "parent": 999_999}), "999999"),
    ] {
        let e = s.execute("icon.insert", &p).unwrap_err().to_string();
        assert!(e.contains(want), "{p}: {e}");
    }
    assert!(s.execute("icon.search", &json!({"set": "nope"})).is_err());
}

#[test]
fn create_tree_layers_can_be_icons() {
    let mut s = Session::new();
    let plan = crate::layer_tree::plan(
        &json!({"type": "frame", "props": {"layout": "horizontal"}, "children": [
            {"type": "icon", "set": "lucide", "icon": "check", "size": 16, "color": "#12B76A", "name": "Done", "props": {"opacity": 0.5}},
            {"type": "text", "text": "Saved"}]}),
        None,
    )
    .unwrap();
    let steps: Vec<(String, Value)> = plan.steps.iter().map(|st| (st["command"].as_str().unwrap().to_string(), st["params"].clone())).collect();
    assert_eq!(steps.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>(), ["node.create", "icon.insert", "node.set", "node.create"]);
    let results = s.batch("Row", &steps).unwrap();
    let row = node(&s, &results[0]["id"]);
    assert_eq!((row["children"][0]["name"].as_str(), row["children"][0]["opacity"].as_f64()), (Some("Done"), Some(0.5)));
    assert!(crate::layer_tree::plan(&json!({"type": "icon", "set": "lucide", "icon": "x", "children": []}), None).is_err(), "icons have no children");
}
