//! A nested layer description flattened into one `command.batch` of `node.create` steps, each
//! child's `parent` a `{"$step": n}` reference to the step that made its parent: one undo step
//! builds a whole screen. MCP `create_tree` and `import.pen` both build their layers this way.

use serde_json::{Map, Value, json};

/// Most layers one call creates (a dense screen is a few hundred).
pub const MAX_NODES: usize = 5000;
/// Deepest nesting accepted (deeper than any real design).
pub const MAX_DEPTH: usize = 64;

/// `node.create` keys a tree node passes through as they are.
const PASSED: &[&str] = &["type", "x", "y", "width", "height", "index", "name", "text", "path", "points", "closed", "props"];

/// The flattened tree: the batch steps, which steps are the roots, and each step's place in the
/// tree (`nodes[0].children[2]`) for error messages.
pub struct Plan {
    pub steps: Vec<Value>,
    pub roots: Vec<usize>,
    pub paths: Vec<String>,
}

/// Flatten `nodes` (one object or an array of them) under `parent` (default: the current page).
pub fn plan(nodes: &Value, parent: Option<&Value>) -> Result<Plan, String> {
    let list: Vec<&Value> = match nodes {
        Value::Array(a) if !a.is_empty() => a.iter().collect(),
        Value::Object(_) => vec![nodes],
        _ => return Err("`nodes` must be a layer object or a non-empty array of them".into()),
    };
    let mut p = Plan { steps: Vec::new(), roots: Vec::new(), paths: Vec::new() };
    for (i, n) in list.into_iter().enumerate() {
        let step = p.steps.len();
        add(&mut p, n, parent.cloned(), format!("nodes[{i}]"), 0)?;
        p.roots.push(step);
    }
    Ok(p)
}

fn add(p: &mut Plan, n: &Value, parent: Option<Value>, path: String, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{path}: nests deeper than {MAX_DEPTH} levels"));
    }
    if p.steps.len() >= MAX_NODES {
        return Err(format!("more than {MAX_NODES} layers in one call; split the tree"));
    }
    let Some(o) = n.as_object() else { return Err(format!("{path}: must be an object")) };
    if !o.get("type").is_some_and(Value::is_string) {
        return Err(format!("{path}: `type` must be a string (frame, rectangle, text, …)"));
    }
    if let Some(k) = o.keys().find(|k| !PASSED.contains(&k.as_str()) && *k != "children") {
        return Err(format!("{path}: unknown key `{k}` (layer properties go in `props`)"));
    }
    if o.get("props").is_some_and(|v| !v.is_object() && !v.is_null()) {
        return Err(format!("{path}: `props` must be an object"));
    }
    let mut params = Map::new();
    for k in PASSED {
        if let Some(v) = o.get(*k).filter(|v| !v.is_null()) {
            params.insert((*k).into(), v.clone());
        }
    }
    if let Some(Value::Object(props)) = params.get_mut("props") {
        for k in ["fills", "strokes", "effects"] {
            if let Some(v) = props.get_mut(k) {
                crate::params::normalize_paint(v);
            }
        }
    }
    if let Some(parent) = parent {
        params.insert("parent".into(), parent);
    }
    // The roots are selected once at the end, not each layer as it is made.
    params.insert("select".into(), json!(false));
    let me = p.steps.len();
    p.steps.push(json!({"command": "node.create", "params": params}));
    p.paths.push(path.clone());
    match o.get("children") {
        None | Some(Value::Null) => {}
        Some(Value::Array(children)) => {
            for (i, c) in children.iter().enumerate() {
                add(p, c, Some(json!({"$step": me})), format!("{path}.children[{i}]"), depth + 1)?;
            }
        }
        Some(_) => return Err(format!("{path}: `children` must be an array")),
    }
    Ok(())
}

/// The batch error with the failing layer's place in the tree, when the engine names a step.
pub fn locate(err: &str, plan: &Plan) -> String {
    let step = err.split("step ").nth(1).and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|d| d.parse::<usize>().ok());
    match step.and_then(|s| plan.paths.get(s)) {
        Some(path) => format!("{err} (at {path}; nothing was created)"),
        None => format!("{err} (nothing was created)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_refer_to_their_parents_step() {
        let t = json!({"type": "frame", "name": "Login", "props": {"layout": "vertical"}, "children": [
            {"type": "text", "text": "Hi"},
            {"type": "frame", "children": [{"type": "text", "text": "Go"}]},
        ]});
        let p = plan(&t, Some(&json!(7))).unwrap();
        let parents: Vec<Value> = p.steps.iter().map(|s| s["params"]["parent"].clone()).collect();
        assert_eq!(parents, vec![json!(7), json!({"$step": 0}), json!({"$step": 0}), json!({"$step": 2})]);
        assert_eq!(p.roots, vec![0]);
        assert_eq!(p.paths[3], "nodes[0].children[1].children[0]");
        assert!(p.steps.iter().all(|s| s["params"]["select"] == false && s["command"] == "node.create"));
        assert!(p.steps[0]["params"].get("children").is_none());
    }

    #[test]
    fn several_roots_and_no_parent() {
        let p = plan(&json!([{"type": "rectangle"}, {"type": "ellipse"}]), None).unwrap();
        assert_eq!(p.roots, vec![0, 1]);
        assert!(p.steps[0]["params"].get("parent").is_none());
    }

    #[test]
    fn bad_trees_say_where() {
        for (t, want) in [
            (json!([]), "`nodes`"),
            (json!({"name": "x"}), "nodes[0]: `type`"),
            (json!({"type": "frame", "children": [{"type": "text", "fill": "#000"}]}), "nodes[0].children[0]: unknown key `fill`"),
            (json!({"type": "frame", "children": {}}), "`children` must be an array"),
            (json!({"type": "frame", "props": 3}), "`props` must be an object"),
        ] {
            let e = plan(&t, None).err().unwrap();
            assert!(e.contains(want), "{t}: {e}");
        }
    }

    #[test]
    fn size_and_depth_are_capped() {
        let mut deep = json!({"type": "frame"});
        for _ in 0..=MAX_DEPTH {
            deep = json!({"type": "frame", "children": [deep]});
        }
        assert!(plan(&deep, None).err().unwrap().contains("deeper"));
        let wide = json!({"type": "frame", "children": vec![json!({"type": "rectangle"}); MAX_NODES]});
        assert!(plan(&wide, None).err().unwrap().contains("more than"));
    }

    #[test]
    fn errors_name_the_layer() {
        let p = plan(&json!({"type": "frame", "children": [{"type": "text"}]}), None).unwrap();
        assert!(locate("command.batch: step 1: bad text", &p).contains("at nodes[0].children[0]"));
        assert!(locate("something else", &p).ends_with("(nothing was created)"));
    }
}
