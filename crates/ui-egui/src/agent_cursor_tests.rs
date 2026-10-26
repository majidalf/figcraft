//! The agent's cursor: an edit through the control channel shows it at the lower right of the
//! changed layer, it glides to the next one, fades a few seconds after the last edit, and stays
//! away while Preferences turn it off or the edit is on another page. Edits made in the window
//! don't show it.

use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::layers_drag_tests::Ui;

fn execute(u: &mut Ui, command: &str, params: Value) -> Value {
    crate::control::handle(&mut u.app, &egui::Context::default(), "engine.execute", &json!({"command": command, "params": params})).unwrap()
}

fn shown(u: &Ui) -> Option<(f64, f64)> {
    let v = &u.app.inspect()["agentCursor"];
    v.as_object().map(|_| (v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()))
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) < 1.0
}

/// Frames until the cursor stops moving (at most two seconds of them).
fn settle(u: &mut Ui) {
    for _ in 0..120 {
        u.frame(vec![]);
    }
}

#[test]
fn follows_the_agent_and_fades() {
    let mut u = Ui::new(Session::new());
    u.frame(vec![]);
    assert_eq!(shown(&u), None);
    execute(&mut u, "shape.rectangle", json!({"x": 10, "y": 20, "width": 100, "height": 50}));
    u.frame(vec![]);
    // It appears at the new layer's lower right.
    assert!(near(shown(&u).unwrap(), (110.0, 70.0)), "{:?}", shown(&u));
    // The next edit: it glides there rather than jumping.
    execute(&mut u, "shape.rectangle", json!({"x": 300, "y": 200, "width": 40, "height": 40}));
    u.frame(vec![]);
    let mid = shown(&u).unwrap();
    assert!(!near(mid, (110.0, 70.0)) && !near(mid, (340.0, 240.0)), "{mid:?}");
    settle(&mut u);
    assert!(near(shown(&u).unwrap(), (340.0, 240.0)), "{:?}", shown(&u));
    // Gone a few seconds after the last edit.
    u.advance_clock(10.0);
    u.frame(vec![]);
    assert_eq!(shown(&u), None);
}

#[test]
fn off_in_preferences_and_not_for_edits_in_the_window() {
    let mut u = Ui::new(Session::new());
    u.app.run("pref.agentCursor", json!({"on": false}));
    execute(&mut u, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10}));
    u.frame(vec![]);
    assert_eq!(shown(&u), None);
    u.app.run("pref.agentCursor", json!({"on": true}));
    u.frame(vec![]);
    assert!(shown(&u).is_some());
    // Commands run from the window itself (menus, panels) aren't the agent's.
    u.advance_clock(10.0);
    u.frame(vec![]);
    u.app.run("shape.rectangle", json!({"x": 50, "y": 50, "width": 10, "height": 10}));
    u.frame(vec![]);
    assert_eq!(shown(&u), None);
}

#[test]
fn only_on_the_page_it_edited() {
    let mut u = Ui::new(Session::new());
    execute(&mut u, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10}));
    u.frame(vec![]);
    assert!(shown(&u).is_some());
    u.app.run("page.add", json!({}));
    u.frame(vec![]);
    assert_eq!(shown(&u), None);
}
