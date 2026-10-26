//! What an agent changed last (`Session::note_agent_edit`): the layers a command named, the new
//! layers it selected, the edited layer rather than the auto layout frame that re-flowed around
//! it, nothing for commands that change nothing; and `pref.agentCursor`.

use serde_json::{Value, json};

use crate::Session;
use crate::params::id_of;
use figcraft_doc::NodeId;

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn id(v: &Value) -> NodeId {
    id_of(&v["id"]).unwrap()
}

/// Run `cmd` as an agent would (through the control channel) and return what it pointed at.
fn agent(s: &mut Session, cmd: &str, p: Value) -> Vec<NodeId> {
    let before = s.agent_before();
    ex(s, cmd, p.clone());
    s.note_agent_edit(before, &p);
    s.agent.as_ref().map(|a| a.nodes.clone()).unwrap_or_default()
}

#[test]
fn new_layers_are_pointed_at() {
    let mut s = Session::new();
    let before = s.agent_before();
    let r = ex(&mut s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 40}));
    s.note_agent_edit(before, &json!({}));
    let edit = s.agent.clone().unwrap();
    assert_eq!(edit.nodes, vec![id(&r)]);
    assert_eq!(edit.page, s.page().unwrap());
    assert_eq!(edit.serial, 1);
}

#[test]
fn the_named_layer_not_the_selection() {
    let mut s = Session::new();
    let a = id(&ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})));
    let b = id(&ex(&mut s, "shape.rectangle", json!({"x": 50, "y": 0, "width": 10, "height": 10})));
    ex(&mut s, "select.set", json!({"ids": [b]}));
    assert_eq!(agent(&mut s, "node.set", json!({"id": a, "props": {"x": 5}})), vec![a]);
    assert_eq!(agent(&mut s, "node.set", json!({"ids": [b], "props": {"opacity": 0.5}})), vec![b]);
}

#[test]
fn the_edited_child_not_the_frame_that_reflowed() {
    let mut s = Session::new();
    let f = id(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 100})));
    ex(&mut s, "node.set", json!({"id": f, "props": {"layout": "vertical", "sizingH": "hug", "sizingV": "hug", "padding": 8, "gap": 4}}));
    let c = id(&ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 40, "height": 20, "parent": f})));
    ex(&mut s, "select.set", json!({"ids": []}));
    // A script with no `ids` (a batch) and nothing selected: the edited child, not its frame.
    let p = json!({"commands": [{"command": "node.set", "params": {"id": c, "props": {"height": 60}}}]});
    assert_eq!(agent(&mut s, "command.batch", p), vec![c]);
}

#[test]
fn a_command_that_changes_nothing_keeps_the_last_edit() {
    let mut s = Session::new();
    let a = id(&ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})));
    assert_eq!(agent(&mut s, "node.set", json!({"id": a, "props": {"x": 20}})), vec![a]);
    let serial = s.agent.as_ref().unwrap().serial;
    agent(&mut s, "select.set", json!({"ids": []}));
    agent(&mut s, "view.zoomIn", json!({}));
    assert_eq!(s.agent.as_ref().unwrap().serial, serial);
    // The same layer edited again is a new edit.
    agent(&mut s, "node.set", json!({"id": a, "props": {"x": 30}}));
    assert_eq!(s.agent.as_ref().unwrap().serial, serial + 1);
}

#[test]
fn a_deleted_layer_points_at_its_parent() {
    let mut s = Session::new();
    let f = id(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 100})));
    let c = id(&ex(&mut s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 20, "parent": f})));
    let nodes = agent(&mut s, "edit.delete", json!({"ids": [c]}));
    assert!(nodes.iter().all(|n| s.doc().unwrap().contains(*n)), "{nodes:?}");
}

#[test]
fn pref_agent_cursor() {
    let mut s = Session::new();
    assert!(s.prefs.agent_cursor);
    assert_eq!(s.prefs.agent_name, "Claude");
    assert_eq!(ex(&mut s, "pref.agentCursor", json!({}))["on"], json!(false));
    assert_eq!(ex(&mut s, "pref.agentCursor", json!({}))["on"], json!(true));
    let r = ex(&mut s, "pref.agentCursor", json!({"name": "  Helper\n ", "color": "#12ab34"}));
    assert_eq!(r, json!({"on": true, "name": "Helper", "color": "#12AB34"}));
    assert!(s.execute("pref.agentCursor", &json!({"color": "red"})).is_err());
    assert_eq!(s.prefs.agent_color, "#12AB34");
    let r = ex(&mut s, "pref.agentCursor", json!({"on": false, "name": "", "color": ""}));
    assert_eq!(r, json!({"on": false, "name": "Claude", "color": crate::agent::DEFAULT_COLOR}));
}
