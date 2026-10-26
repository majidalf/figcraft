//! MCP tool definitions (names, descriptions, JSON Schemas) and their implementations on top of a
//! [`Backend`]. Every tool reduces to control-channel methods, so headless and attached sessions
//! behave the same.

use base64::Engine as _;
use figcraft_engine::params::normalize_paint;
use serde_json::{Map, Value, json};

use crate::backend::{Backend, decode_data, page_children};

/// The result of `tools/call`: MCP content blocks plus the `isError` flag.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: false }
    }
    /// Compact JSON: every byte is read by the model as tokens, so no indentation.
    pub fn json(v: &Value) -> Self {
        Self::text(serde_json::to_string(v).unwrap_or_default())
    }
    pub fn error(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: true }
    }
    /// An image block followed by a JSON text block describing it.
    pub fn image(bytes: &[u8], mime: &str, info: &Value) -> Self {
        Self {
            content: vec![
                json!({"type": "image", "data": base64::engine::general_purpose::STANDARD.encode(bytes), "mimeType": mime}),
                json!({"type": "text", "text": info.to_string()}),
            ],
            is_error: false,
        }
    }
    pub fn to_value(&self) -> Value {
        json!({"content": self.content, "isError": self.is_error})
    }
    /// The first text block (tests and the CLI).
    pub fn first_text(&self) -> &str {
        self.content.iter().find_map(|c| c.get("text").and_then(Value::as_str)).unwrap_or("")
    }
}

// ---------- schemas ----------

fn num(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}
fn int(desc: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": desc})
}
fn string(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
fn boolean(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}
fn id_schema(desc: &str) -> Value {
    json!({"description": desc, "anyOf": [{"type": "integer"}, {"type": "string"}]})
}
fn ids_schema(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {"anyOf": [{"type": "integer"}, {"type": "string"}]}})
}
fn verbose_schema() -> Value {
    boolean("Full JSON, defaults spelled out")
}
fn format_schema() -> Value {
    json!({"type": "string", "enum": ["json", "outline"]})
}
fn obj(props: Value, required: &[&str]) -> Value {
    let mut o = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        o["required"] = json!(required);
    }
    o
}

fn tool(name: &str, title: &str, desc: &str, schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": schema,
        // `title` above already names it for display; the hints default to true, so they stay.
        "annotations": {"readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false},
    })
}

/// Tool names, in `tools/list` order.
pub const TOOL_NAMES: &[&str] = &[
    "read_guide",
    "list_commands",
    "run_command",
    "batch",
    "create_node",
    "create_tree",
    "set_properties",
    "inspect_document",
    "get_node",
    "select",
    "export",
    "screenshot",
    "open_file",
    "save_file",
    "undo",
    "redo",
    "pointer",
    "key",
    "type_text",
    "inspect_ui",
];

/// All tools, in `tools/list` order.
pub fn tool_definitions() -> Vec<Value> {
    let empty = || obj(json!({}), &[]);
    let max_bytes = || int("Largest response in bytes (default 8 MiB)");
    vec![
        tool(
            "read_guide",
            "Read guide",
            "Detail the other tool descriptions leave out: every set_properties key, create_tree and batch examples, the outline format, screenshot and export options. No topic: the index.",
            obj(json!({"topic": string("create, properties, inspect, look, commands, input or files")}), &[]),
            true,
        ),
        tool(
            "list_commands",
            "List commands",
            "List commands (every menu item, panel action and shortcut is one): id, label, menu, shortcut, undoable, params. Narrow with `filter` (e.g. \"align\", \"layout\", \"component\").",
            obj(json!({"filter": string("Substring of id, label, menu or params")}), &[]),
            true,
        ),
        tool(
            "run_command",
            "Run command",
            "Run one command by id with JSON params, as one undo step; acts on `ids` when given, else the selection. E.g. {\"command\":\"align.left\",\"params\":{\"ids\":[3,4]}}.",
            obj(json!({"command": string("Command id"), "params": {"type": "object"}}), &["command"]),
            false,
        ),
        tool(
            "batch",
            "Run commands as one step",
            "Several commands as ONE undo step, all or nothing. {\"$step\": n} in a step's params is the id step n returned (read_guide create).",
            obj(
                json!({
                    "commands": {"type": "array", "minItems": 1, "items": obj(json!({"command": string("Command id"), "params": {"type": "object"}}), &["command"])},
                    "label": string("Undo label"),
                }),
                &["commands"],
            ),
            false,
        ),
        tool(
            "create_node",
            "Create layer",
            "Create one layer; returns {id, x, y, width, height}. x/y relative to the parent frame (page units, y down). For more than one layer use create_tree.",
            obj(
                json!({
                    "type": {"type": "string", "enum": ["frame", "rectangle", "ellipse", "polygon", "star", "line", "arrow", "text", "vector", "section", "component", "slice"]},
                    "x": num("Default 0"),
                    "y": num("Default 0"),
                    "width": num("Default 100; text hugs unless set"),
                    "height": num("Default 100"),
                    "parent": id_schema("Default: the current page"),
                    "index": int("Z-index (default on top)"),
                    "name": string("Layer name"),
                    "text": string("Characters (text)"),
                    "path": string("SVG path data (vector)"),
                    "props": {"type": "object", "description": "set_properties keys, e.g. {\"fill\":\"#3366FF\",\"layout\":\"vertical\",\"gap\":8}"},
                    "verbose": verbose_schema(),
                }),
                &["type"],
            ),
            false,
        ),
        tool(
            "create_tree",
            "Create layer tree",
            "Build a whole screen in ONE call and one undo step: `nodes` is a layer with the create_node keys plus `children`, nested. All or nothing. Returns the new layers as an outline (ids and laid-out boxes).",
            obj(
                json!({
                    "nodes": {"description": "{type, name?, x?, y?, width?, height?, text?, props?, children?} or an array", "anyOf": [{"type": "object"}, {"type": "array", "items": {"type": "object"}}]},
                    "parent": id_schema("Default: the current page"),
                    "label": string("Undo label"),
                }),
                &["nodes"],
            ),
            false,
        ),
        tool(
            "set_properties",
            "Set properties",
            "Set layer properties (one undo step): fill(s), stroke(s), cornerRadius, effects, opacity, x/y/width/height, layout/gap/padding/mainAlign/crossAlign, sizingH/V, text (fontSize, fontFamily…). Keys and paint forms: read_guide properties. Returns {ok, nodes: [{id, x, y, width, height}], warnings?}.",
            obj(json!({"ids": ids_schema("Default: the selection"), "props": {"type": "object"}, "verbose": verbose_schema()}), &["props"]),
            false,
        ),
        tool(
            "inspect_document",
            "Inspect document",
            "The document: selection, pages, the current page's layer tree, styles, undo history. Compact JSON (defaults left out); format \"outline\" is one line per layer, the cheapest check (read_guide inspect).",
            obj(
                json!({
                    "depth": int("Tree depth (default 4)"),
                    "allPages": boolean("Expand every page"),
                    "format": format_schema(),
                    "verbose": verbose_schema(),
                    "maxBytes": max_bytes(),
                }),
                &[],
            ),
            true,
        ),
        tool(
            "get_node",
            "Get layer",
            "One layer and its children to `depth`, as compact JSON or format \"outline\". `offset` pages through many children.",
            obj(
                json!({
                    "id": id_schema("Layer id"),
                    "depth": int("Default 1"),
                    "offset": int("First child (default 0)"),
                    "format": format_schema(),
                    "verbose": verbose_schema(),
                    "maxBytes": max_bytes(),
                }),
                &["id"],
            ),
            true,
        ),
        tool(
            "select",
            "Select layers",
            "Replace the selection with `ids` ([] deselects). Commands act on the selection when given no ids.",
            obj(json!({"ids": ids_schema("Layer ids")}), &["ids"]),
            false,
        ),
        tool(
            "export",
            "Export",
            "Export layers (default the selection) as png, jpg, svg, webp, pdf, or the file as fig; to `path`, else returned. Print and SVG options: read_guide look.",
            obj(
                json!({
                    "ids": ids_schema("Layers"),
                    "format": {"type": "string", "enum": ["png", "jpg", "svg", "webp", "pdf", "fig"]},
                    "scale": num("Default 1"),
                    "size": string("2x, 512w or 512h"),
                    "path": string("Output file"),
                    "bleed": {"type": ["number", "string"]},
                    "cropMarks": {"type": "boolean"},
                    "bleedMarks": {"type": "boolean"},
                    "cmyk": {"type": "boolean"},
                    "contentsOnly": {"type": "boolean"},
                    "includeBoundingBox": {"type": "boolean"},
                    "svgOutlineText": {"type": "boolean"},
                    "svgIdAttribute": {"type": "boolean"},
                    "svgSimplifyStroke": {"type": "boolean"},
                }),
                &[],
            ),
            true,
        ),
        tool(
            "screenshot",
            "Screenshot",
            "Look at the design: the page or `ids` as an image, fitted to maxSize (default 800). Tokens grow with the pixel area: screenshot just the layers in question.",
            obj(
                json!({
                    "ids": ids_schema("Default: the whole page"),
                    "scale": num("Default 1"),
                    "maxSize": int("Largest side (default 800)"),
                    "format": {"type": "string", "enum": ["png", "jpeg", "webp"]},
                    "window": {"description": "Attached app: true or a window id captures the window", "anyOf": [{"type": "boolean"}, {"type": "integer"}]},
                }),
                &[],
            ),
            true,
        ),
        tool(
            "open_file",
            "Open file",
            "Open .figcraft, .json, .fig (best effort), .svg or an image as the active document (read_guide files).",
            obj(
                json!({"path": string("File path"), "variables": {"description": "REST API JSON only: local variables JSON or its path"}}),
                &["path"],
            ),
            false,
        ),
        tool(
            "save_file",
            "Save file",
            "Save as .figcraft (`path` needed the first time). A .fig path writes an experimental export copy.",
            obj(json!({"path": string("Output path")}), &[]),
            false,
        ),
        tool("undo", "Undo", "Undo the last change (one command, batch or gesture). Returns {undone}.", empty(), false),
        tool("redo", "Redo", "Redo the last undone change. Returns {redone}.", empty(), false),
        tool(
            "pointer",
            "Pointer gesture",
            "Mouse gesture in page coordinates with the current tool, as a user: click, double, drag (`to`), down, move, up, hover; `tool` picks a tool first (read_guide input).",
            obj(
                json!({
                    "kind": {"type": "string", "enum": ["click", "double", "drag", "down", "move", "up", "hover"]},
                    "x": num("Page x"),
                    "y": num("Page y"),
                    "to": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2},
                    "steps": int("Drag move events (default 8)"),
                    "tool": string("e.g. rectangle, pen, text, move"),
                    "shift": {"type": "boolean"},
                    "alt": {"type": "boolean"},
                    "cmd": {"type": "boolean"},
                    "pressure": num("0-1"),
                    "pressureTo": num("0-1 at a drag's end"),
                }),
                &["x", "y"],
            ),
            false,
        ),
        tool(
            "key",
            "Press key",
            "Press a key or shortcut on the canvas (e.g. {\"key\":\"G\",\"cmd\":true}); while editing text, keys go to the text.",
            obj(
                json!({"key": string("A, 0, Escape, Delete, Enter, ArrowLeft…"), "shift": {"type": "boolean"}, "alt": {"type": "boolean"}, "cmd": {"type": "boolean"}}),
                &["key"],
            ),
            false,
        ),
        tool(
            "type_text",
            "Type text",
            "Replace text layer `id`'s contents, or without `id` type at the caret of the text being edited.",
            obj(json!({"text": string("Text (\\n: new line)"), "id": id_schema("Text layer")}), &["text"]),
            false,
        ),
        tool("inspect_ui", "Inspect UI", "Tool, view and status; panels, timings and windows when attached to the app.", empty(), true),
    ]
}

// ---------- helpers ----------

fn exec(b: &mut dyn Backend, cmd: &str, params: Value) -> Result<Value, String> {
    b.call("engine.execute", json!({"command": cmd, "params": params}))
}

fn args_obj(args: &Value) -> Result<Map<String, Value>, String> {
    match args {
        Value::Object(o) => Ok(o.clone()),
        Value::Null => Ok(Map::new()),
        _ => Err("`arguments` must be an object".into()),
    }
}

fn s_arg<'a>(a: &'a Map<String, Value>, k: &str) -> Result<Option<&'a str>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("`{k}` must be a string")),
    }
}

fn b_arg(a: &Map<String, Value>, k: &str) -> Result<bool, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(x)) => Ok(*x),
        Some(_) => Err(format!("`{k}` must be a boolean")),
    }
}

fn req_s<'a>(a: &'a Map<String, Value>, k: &str) -> Result<&'a str, String> {
    s_arg(a, k)?.ok_or_else(|| format!("`{k}` is required"))
}

fn n_arg(a: &Map<String, Value>, k: &str) -> Result<Option<f64>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => figcraft_engine::params::f64_of(v).map(Some).ok_or_else(|| format!("`{k}` must be a finite number")),
    }
}

fn id_arg(v: &Value) -> Result<u64, String> {
    figcraft_engine::params::id_of(v).map(|i| i.0).ok_or_else(|| format!("{v} is not a layer id"))
}

fn ids_arg(a: &Map<String, Value>, k: &str) -> Result<Option<Vec<u64>>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => arr.iter().map(id_arg).collect::<Result<Vec<_>, _>>().map(Some),
        Some(v) => id_arg(v).map(|i| Some(vec![i])),
    }
}

fn png_info(bytes: &[u8]) -> (u32, u32) {
    crate::png_size(bytes).unwrap_or((0, 0))
}

/// Union of the layers' absolute bounds (page coordinates).
fn bounds_of(b: &mut dyn Backend, ids: &[u64]) -> Option<(f64, f64, f64, f64)> {
    let mut acc: Option<(f64, f64, f64, f64)> = None;
    for id in ids.iter().take(500) {
        let Ok(n) = b.call("node.get", json!({"id": id, "depth": 0})) else { continue };
        let Some(ab) = n.get("absoluteBounds") else { continue };
        let g = |k: &str| ab.get(k).and_then(Value::as_f64);
        let (Some(x), Some(y), Some(w), Some(h)) = (g("x"), g("y"), g("width"), g("height")) else { continue };
        acc = Some(match acc {
            None => (x, y, x + w, y + h),
            Some((a, b2, c, d)) => (a.min(x), b2.min(y), c.max(x + w), d.max(y + h)),
        });
    }
    acc
}

/// Make paint/effect JSON friendlier than the engine's strict serde (see
/// [`figcraft_engine::params::normalize_paint`]). The engine reads paints this way itself; effects
/// still need it here.
pub fn normalize_props(props: &mut Map<String, Value>) {
    for k in ["fills", "strokes", "effects"] {
        if let Some(v) = props.get_mut(k) {
            normalize_paint(v);
        }
    }
}

/// [`normalize_props`] for command params: `props`, flat node.set keys, `paint`, `effect`.
fn friendly_params(p: &mut Value) {
    let Some(o) = p.as_object_mut() else { return };
    if let Some(Value::Object(props)) = o.get_mut("props") {
        normalize_props(props);
    }
    normalize_props(o);
    for k in ["paint", "effect"] {
        if let Some(v) = o.get_mut(k) {
            normalize_paint(v);
        }
    }
}

// ---------- dispatch ----------

/// Run one tool. Every failure becomes an `isError` result (never a panic).
pub fn call_tool(b: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    let a = match args_obj(args) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(e),
    };
    let r = match name {
        "read_guide" => s_arg(&a, "topic").and_then(crate::guide::read).map(ToolResult::text),
        "list_commands" => list_commands(b, &a),
        "run_command" => run_command(b, &a),
        "batch" => batch(b, &a),
        "create_node" => create_node(b, &a),
        "create_tree" => create_tree(b, &a),
        "set_properties" => set_properties(b, &a),
        "inspect_document" => inspect_document(b, &a),
        "get_node" => get_node(b, &a),
        "select" => select(b, &a),
        "export" => export(b, &a),
        "screenshot" => screenshot(b, &a),
        "open_file" => open_file(b, &a),
        "save_file" => save_file(b, &a),
        "undo" => exec(b, "edit.undo", json!({})).map(|v| ToolResult::json(&v)),
        "redo" => exec(b, "edit.redo", json!({})).map(|v| ToolResult::json(&v)),
        "pointer" => pointer(b, &a),
        "key" => key(b, &a),
        "type_text" => type_text(b, &a),
        "inspect_ui" => b.call("ui.inspect", json!({})).map(|v| ToolResult::json(&v)),
        other => Err(format!("unknown tool `{other}` (tools/list has: {})", TOOL_NAMES.join(", "))),
    };
    r.unwrap_or_else(ToolResult::error)
}

fn list_commands(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let filter = s_arg(a, "filter")?.map(str::to_lowercase).unwrap_or_default();
    let all = b.call("engine.commands", json!({}))?;
    let list: Vec<Value> = all
        .as_array()
        .map(|v| {
            v.iter()
                .filter(|c| {
                    filter.is_empty()
                        || ["id", "label", "menu", "params"]
                            .iter()
                            .any(|k| c.get(*k).and_then(Value::as_str).is_some_and(|s| s.to_lowercase().contains(&filter)))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if list.is_empty() {
        return Ok(ToolResult::text(format!("no command matches `{filter}`")));
    }
    // One compact line per command keeps this readable for agents.
    let mut out = format!("{} commands (id — label [menu] (shortcut) params; * = undoable):\n", list.len());
    for c in &list {
        let g = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("");
        out.push_str(g("id"));
        if c.get("undoable").and_then(Value::as_bool) == Some(true) {
            out.push('*');
        }
        out.push_str(" — ");
        out.push_str(g("label"));
        if !g("menu").is_empty() {
            out.push_str(&format!(" [{}]", g("menu")));
        }
        if !g("shortcut").is_empty() {
            out.push_str(&format!(" ({})", g("shortcut")));
        }
        out.push(' ');
        out.push_str(g("params"));
        out.push('\n');
    }
    Ok(ToolResult::text(out))
}

fn run_command(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let cmd = req_s(a, "command")?;
    let params = match a.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(_) => return Err("`params` must be an object".into()),
    };
    let mut params = params;
    friendly_params(&mut params);
    exec(b, cmd, params).map(|v| ToolResult::json(&v))
}

fn batch(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let Some(Value::Array(cmds)) = a.get("commands") else { return Err("`commands` must be an array of {command, params}".into()) };
    if cmds.is_empty() {
        return Err("`commands` is empty".into());
    }
    let mut steps = Vec::with_capacity(cmds.len());
    for (i, c) in cmds.iter().enumerate() {
        let Some(cmd) = c.get("command").and_then(Value::as_str) else { return Err(format!("step {i}: `command` must be a string")) };
        let params = match c.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(v @ Value::Object(_)) => v.clone(),
            Some(_) => return Err(format!("step {i} ({cmd}): `params` must be an object")),
        };
        let mut params = params;
        friendly_params(&mut params);
        steps.push(json!({"command": cmd, "params": params}));
    }
    let label = s_arg(a, "label")?.unwrap_or("Batch");
    exec(b, "command.batch", json!({"label": label, "commands": steps})).map(|v| ToolResult::json(&v))
}

fn create_node(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    req_s(a, "type")?;
    for k in ["x", "y", "width", "height"] {
        n_arg(a, k)?;
    }
    if let Some(p) = a.get("props")
        && !p.is_object()
        && !p.is_null()
    {
        return Err("`props` must be an object".into());
    }
    let mut params = Map::new();
    for k in ["type", "x", "y", "width", "height", "parent", "index", "name", "text", "path", "points", "closed", "props", "select"] {
        if let Some(v) = a.get(k).filter(|v| !v.is_null()) {
            params.insert(k.into(), v.clone());
        }
    }
    if let Some(Value::Object(p)) = params.get_mut("props") {
        normalize_props(p);
    }
    let verbose = b_arg(a, "verbose")?;
    let r = exec(b, "node.create", Value::Object(params))?;
    let id = r.get("id").cloned().ok_or("node.create returned no id")?;
    let node = b.call("node.get", json!({"id": id, "depth": 0}))?;
    if verbose {
        return Ok(ToolResult::json(&json!({"id": id, "node": node})));
    }
    Ok(ToolResult::json(&layer_box(&node)))
}

/// A nested layer tree as one batch (see `tree`); answers with the new layers as an outline, so
/// the ids and where the layout put each layer come back in the same call.
fn create_tree(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let nodes = a.get("nodes").ok_or("`nodes` is required")?;
    let parent = a.get("parent").filter(|v| !v.is_null());
    if let Some(p) = parent {
        id_arg(p)?;
    }
    let plan = crate::tree::plan(nodes, parent)?;
    let label = s_arg(a, "label")?.unwrap_or("Create layers");
    let (max_bytes, budget) = budget(a)?;
    let results = exec(b, "command.batch", json!({"label": label, "commands": plan.steps})).map_err(|e| crate::tree::locate(&e, &plan))?;
    let roots: Vec<Value> = plan.roots.iter().filter_map(|i| results.get(*i)?.get("id").cloned()).collect();
    exec(b, "select.set", json!({"ids": roots}))?;
    let mut lines = Vec::new();
    for id in &roots {
        let mut v = b.call("node.get", json!({"id": id, "depth": crate::tree::MAX_DEPTH}))?;
        crate::slim::tree(&mut v);
        lines.extend(crate::outline::tree(&v));
    }
    Ok(outline_result(lines, budget, max_bytes))
}

/// A layer's id and box, the reply to an edit: what an agent needs to place the next layer, at a
/// tenth of the whole layer's JSON. The layout may have moved or sized it, so the box is read back.
fn layer_box(node: &Value) -> Value {
    let mut out = Map::new();
    for k in ["id", "x", "y", "width", "height"] {
        if let Some(v) = node.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    let mut out = Value::Object(out);
    crate::slim::tree(&mut out);
    out
}

fn set_properties(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let props = match a.get("props") {
        Some(Value::Object(p)) if !p.is_empty() => p.clone(),
        _ => return Err("`props` must be a non-empty object".into()),
    };
    let verbose = b_arg(a, "verbose")?;
    let mut props = props;
    normalize_props(&mut props);
    let mut params = json!({"props": props});
    let ids = ids_arg(a, "ids")?;
    if let Some(ids) = &ids {
        if ids.is_empty() {
            return Err("`ids` is empty".into());
        }
        params["ids"] = json!(ids);
    }
    let warnings = exec(b, "node.set", params)?.get("warnings").cloned();
    let ids = match ids {
        Some(i) => i,
        None => b
            .call("document.inspect", json!({"depth": 0}))?
            .get("selection")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
    };
    let nodes: Vec<Value> = ids
        .iter()
        .take(20)
        .filter_map(|i| b.call("node.get", json!({"id": i, "depth": 0})).ok())
        .map(|n| if verbose { n } else { layer_box(&n) })
        .collect();
    let mut out = json!({"ok": true, "nodes": nodes});
    if let Some(w) = warnings {
        out["warnings"] = w;
    }
    Ok(ToolResult::json(&out))
}

/// What one tool result may take on the wire. MCP stdio is newline-delimited JSON, so a result is
/// a single message and has to fit the client's read buffer whole -- there is no streaming of part
/// of one. The TypeScript SDK's stdio transport defaults to 10 MiB
/// (`STDIO_DEFAULT_MAX_BUFFER_SIZE`) and hosts rarely raise it; a document whose tree went past
/// that killed the server, because the client stopped reading, the pipe broke, and every later
/// call failed (#912). The 2 MiB left over covers the JSON-RPC envelope.
const MAX_RESPONSE: usize = 8 << 20;

/// Room kept for the `truncated` object, which is added after the size is measured and would
/// otherwise be able to push a response that only just fitted back over the limit.
const MARKER_RESERVE: usize = 1 << 10;

/// Room kept for everything the text block is wrapped in: the content object and `isError`, the
/// JSON-RPC envelope (`jsonrpc`, `id`, `result`) and the newline the transport writes after it.
/// About ninety bytes of that is fixed and the rest is the request id, so `maxBytes` holds for
/// any id up to roughly 900 bytes -- which is every id in practice, a number or a UUID. An id
/// larger than that is the one case where the message can pass the caller's figure, and sizing
/// the real envelope here would mean handing the transport's request id to every tool.
const ENVELOPE_RESERVE: usize = 1 << 10;

/// What the caller says it can read, as the bytes its *tree* may take: the figure is for the whole
/// message, so the envelope's share comes off it here.
fn budget(a: &Map<String, Value>) -> Result<(usize, usize), String> {
    // A caller that knows its own buffer can say so; 1 KiB is below the shallowest tree, so the
    // floor only keeps the arithmetic sane.
    let max_bytes = match n_arg(a, "maxBytes")? {
        Some(v) if v < 1024.0 => return Err("`maxBytes` must be at least 1024".into()),
        Some(v) => v.min(usize::MAX as f64) as usize,
        None => MAX_RESPONSE,
    };
    Ok((max_bytes, max_bytes.saturating_sub(ENVELOPE_RESERVE)))
}

/// What a value's text block would take on the wire. It is sent as a text block, so it is measured
/// as [`ToolResult::json`] writes it *and* escaped -- escaping adds about a tenth.
fn wire_size(value: &Value) -> usize {
    let text = serde_json::to_string(value).unwrap_or_default();
    serde_json::to_string(&text).map_or(text.len(), |s| s.len())
}

/// The document at `depth` (compact unless `verbose`), and what its text block would take on the
/// wire.
fn inspect_at(b: &mut dyn Backend, depth: u64, all: bool, verbose: bool) -> Result<(Value, usize), String> {
    let mut value = b.call("document.inspect", json!({"depth": depth, "allPages": all}))?;
    if !verbose {
        crate::slim::document(&mut value);
    }
    let wire = wire_size(&value);
    Ok((value, wire))
}

/// One layer's subtree at `depth` (compact unless `verbose`), and what its text block would take on
/// the wire.
fn node_at(b: &mut dyn Backend, id: u64, depth: u64, verbose: bool) -> Result<(Value, usize), String> {
    let mut value = b.call("node.get", json!({"id": id, "depth": depth}))?;
    if !verbose {
        crate::slim::tree(&mut value);
    }
    let wire = wire_size(&value);
    Ok((value, wire))
}

/// `format: "outline"` (one line per layer) rather than the default JSON.
fn outline_arg(a: &Map<String, Value>) -> Result<bool, String> {
    let outline = match s_arg(a, "format")? {
        None | Some("json") => false,
        Some("outline") => true,
        Some(other) => return Err(format!("`format` must be \"json\" or \"outline\", not {other:?}")),
    };
    if outline && b_arg(a, "verbose")? {
        return Err("`verbose` applies to format \"json\" only".into());
    }
    Ok(outline)
}

/// Outline lines as one text block, cut to the lines that fit in `budget` with a last line saying
/// how many were left out.
fn outline_result(lines: Vec<String>, budget: usize, max_bytes: usize) -> ToolResult {
    let budget = budget.saturating_sub(MARKER_RESERVE);
    let total = lines.len();
    let mut used = 0usize;
    let mut kept = Vec::with_capacity(total);
    for l in lines {
        // Escaped as it goes on the wire, plus the `\n` between lines.
        let size = serde_json::to_string(&l).map_or(l.len(), |s| s.len());
        if used + size > budget {
            break;
        }
        used += size;
        kept.push(l);
    }
    let shown = kept.len();
    if shown < total {
        kept.push(format!(
            "… {} of {total} lines left out: over the {max_bytes}-byte limit a client can read in one message; get_node({{id, depth, format: \"outline\"}}) reads one subtree",
            total - shown
        ));
    }
    ToolResult::text(kept.join("\n"))
}

fn inspect_document(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let depth = n_arg(a, "depth")?.unwrap_or(4.0).clamp(0.0, 64.0) as u64;
    let all = b_arg(a, "allPages")?;
    let verbose = b_arg(a, "verbose")?;
    let (max_bytes, budget) = budget(a)?;
    if outline_arg(a)? {
        let (value, _) = inspect_at(b, depth, all, false)?;
        return Ok(outline_result(crate::outline::document(&value), budget, max_bytes));
    }

    let (value, wire) = inspect_at(b, depth, all, verbose)?;
    if wire <= budget {
        return Ok(ToolResult::json(&value));
    }

    let (used, mut value, used_wire) = if depth == 0 {
        // Depth 0 is the shallowest tree there is, so there is nothing smaller to send: it goes
        // out over the limit with the marker saying so, the same as a bisection that reaches
        // depth 0. An error would leave the caller with nothing at all.
        (0, value, wire)
    } else {
        // The deepest tree that fits, by bisection: a deeper tree holds every node of a shallower
        // one, so the size does not fall as the depth grows. Bisecting keeps this to about six
        // more walks of the document rather than one per level.
        let (mut lo, mut hi) = (0u64, depth - 1);
        let mut best: Option<(u64, Value, usize)> = None;
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let (v, w) = inspect_at(b, mid, all, verbose)?;
            if w + MARKER_RESERVE <= budget {
                best = Some((mid, v, w));
                if mid == hi {
                    break;
                }
                lo = mid + 1;
            } else if mid == 0 {
                best = Some((0, v, w));
                break;
            } else {
                hi = mid - 1;
            }
        }
        match best {
            Some(found) => found,
            None => return Ok(ToolResult::json(&value)),
        }
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "truncated".into(),
            json!({
                "requestedDepth": depth,
                "depth": used,
                "requestedDepthBytes": wire,
                "bytes": used_wire,
                "maxBytes": max_bytes,
                "reason": format!(
                    "the tree at depth {depth} would be {wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; it is reported at depth {used} instead"
                ),
                "hint": "get_node({id, depth}) walks one subtree at a time, within the same limit: a node with more children than fit answers with a window of them and the offset to ask for next",
            }),
        );
    }
    Ok(ToolResult::json(&value))
}

/// One layer's subtree, kept inside what the caller can read.
///
/// `inspect_document` sends a shallower tree when the document's is too large and points here for
/// the rest, so this tool is the way out of that limit and must not be a way past it: a frame with
/// fifteen thousand children answers with 10.5 MiB at depth 1, over the buffer the report in #912
/// is about, and no depth makes that smaller because the width is at depth 1. So the subtree is
/// cut by depth first, as the document's is, and then by a window over the children.
fn get_node(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let id = id_arg(a.get("id").ok_or("`id` is required")?)?;
    let depth = n_arg(a, "depth")?.unwrap_or(1.0).clamp(0.0, 64.0) as u64;
    let offset = n_arg(a, "offset")?.unwrap_or(0.0).max(0.0).min(u32::MAX as f64) as usize;
    let verbose = b_arg(a, "verbose")?;
    let (max_bytes, budget) = budget(a)?;
    if outline_arg(a)? {
        let (mut value, _) = node_at(b, id, depth, false)?;
        if value.is_null() {
            return Err(format!("no such layer {id}"));
        }
        let mut skipped = 0;
        if let Some(Value::Array(c)) = value.get_mut("children") {
            skipped = offset.min(c.len());
            c.drain(..skipped);
        }
        let mut lines = crate::outline::tree(&value);
        if skipped > 0 && !lines.is_empty() {
            lines.insert(1, format!("  … {skipped} children before (offset {offset})"));
        }
        return Ok(outline_result(lines, budget, max_bytes));
    }

    let (value, wire) = node_at(b, id, depth, verbose)?;
    if value.is_null() {
        return Err(format!("no such layer {id}"));
    }
    if wire <= budget && offset == 0 {
        return Ok(ToolResult::json(&value));
    }

    // The deepest subtree that fits, by bisection, as the document's tree is cut. A node asked for
    // at a depth of its own children only (1) cannot be made shallower without losing them all,
    // so that is where the window takes over.
    let (mut used, mut value, mut used_wire) = (depth, value, wire);
    if depth > 1 && offset == 0 {
        let (mut lo, mut hi) = (1u64, depth - 1);
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let (v, w) = node_at(b, id, mid, verbose)?;
            if w + MARKER_RESERVE <= budget {
                (used, value, used_wire) = (mid, v, w);
                if mid == hi {
                    break;
                }
                lo = mid + 1;
            } else if mid == 1 {
                (used, value, used_wire) = (1, v, w);
                break;
            } else {
                hi = mid - 1;
            }
        }
        if used_wire + MARKER_RESERVE <= budget {
            return Ok(ToolResult::json(&with_marker(value, depth, used, wire, used_wire, max_bytes, true, None)));
        }
    }

    // Still over, or a window was asked for: the children from `offset` that fit.
    let total = value.get("children").and_then(Value::as_array).map_or(0, Vec::len);
    let window = children_window(&mut value, offset, budget.saturating_sub(MARKER_RESERVE));
    let shown = value.get("children").and_then(Value::as_array).map_or(0, Vec::len);
    // A child too large to fit on its own is passed over (a page of just the marker), so paging
    // moves on and every answer stays within maxBytes; `get_node` on the child reads it.
    let after = offset + shown.max(1);
    let next = (after < total).then_some(after);
    let used_wire = wire_size(&value);
    if used_wire + MARKER_RESERVE > budget {
        // Even with every child left out, the layer's own fields (a very long name, say) pass the
        // limit: an answer would be larger than the caller can read, so there is none.
        return Err(format!(
            "layer {id} takes {used_wire} bytes on the wire without its children, more than fit in maxBytes {max_bytes}; ask again with a larger `maxBytes`"
        ));
    }
    // Children left out after the window were cut by size; a window that only `offset` moved, of a
    // subtree within the limit, is paging.
    let over = wire > budget || next.is_some();
    let mut out = with_marker(
        value,
        depth,
        used,
        wire,
        used_wire,
        max_bytes,
        over,
        Some(json!({
            "from": offset,
            "shown": shown,
            "total": total,
            "nextOffset": next,
        })),
    );
    if window && shown == total && offset == 0 && used == depth {
        // Nothing was left out after all (an offset of zero that fitted whole).
        out.as_object_mut().map(|o| o.remove("truncated"));
    }
    Ok(ToolResult::json(&out))
}

/// Keep the children from `offset` that fit in `budget`, and say whether any were left out. The
/// window is found by bisection: a node with thousands of children would otherwise be serialised
/// once per child.
fn children_window(value: &mut Value, offset: usize, budget: usize) -> bool {
    let Some(all) = value.get("children").and_then(Value::as_array).cloned() else { return false };
    let rest: Vec<Value> = all.iter().skip(offset).cloned().collect();
    let fits = |value: &mut Value, n: usize| {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("children".into(), Value::Array(rest.iter().take(n).cloned().collect()));
        }
        wire_size(value) <= budget
    };
    if fits(value, rest.len()) {
        return rest.len() != all.len();
    }
    // The largest window that fits, between none and all of the rest.
    let (mut lo, mut hi) = (0usize, rest.len());
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if fits(value, mid) { lo = mid } else { hi = mid - 1 }
    }
    fits(value, lo);
    true
}

/// `value` with the `truncated` object saying how it was cut. `over`: the subtree asked for passed
/// the limit (else the window is only the `offset` asked for). `children`: the window, when the
/// children were cut as well as (or instead of) the depth.
fn with_marker(
    mut value: Value,
    asked: u64,
    used: u64,
    asked_wire: usize,
    used_wire: usize,
    max_bytes: usize,
    over: bool,
    children: Option<Value>,
) -> Value {
    if let Some(obj) = value.as_object_mut() {
        let mut marker = json!({
            "requestedDepth": asked,
            "depth": used,
            "requestedDepthBytes": asked_wire,
            "bytes": used_wire,
            "maxBytes": max_bytes,
        });
        match &children {
            Some(window) => {
                marker["children"] = window.clone();
                let from = window.get("from").and_then(Value::as_u64).unwrap_or(0);
                marker["reason"] = json!(if over {
                    format!(
                        "the subtree at depth {asked} would be {asked_wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; its children are reported from {from} instead"
                    )
                } else {
                    format!(
                        "its children are reported from {from}, the `offset` asked for; the subtree at depth {asked} ({asked_wire} bytes) is within the {max_bytes}-byte limit"
                    )
                });
                marker["hint"] = json!("ask again with the `offset` in `children.nextOffset` for the rest, or get_node({id, depth}) on one child");
            }
            None => {
                marker["reason"] = json!(format!(
                    "the subtree at depth {asked} would be {asked_wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; it is reported at depth {used} instead"
                ));
                marker["hint"] = json!("get_node({id, depth}) on a child reads deeper, one subtree at a time");
            }
        }
        obj.insert("truncated".into(), marker);
    }
    value
}

fn select(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let ids = ids_arg(a, "ids")?.ok_or("`ids` is required (an empty array deselects)")?;
    if ids.is_empty() {
        exec(b, "select.none", json!({}))?;
    } else {
        exec(b, "select.set", json!({"ids": ids}))?;
    }
    let sel = b.call("document.inspect", json!({"depth": 0}))?.get("selection").cloned().unwrap_or(Value::Null);
    Ok(ToolResult::json(&json!({"selection": sel})))
}

fn export(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let mut params = Map::new();
    if let Some(ids) = ids_arg(a, "ids")? {
        params.insert("ids".into(), json!(ids));
    }
    let format = s_arg(a, "format")?.map(str::to_ascii_lowercase);
    if let Some(f) = &format {
        if !["png", "jpg", "jpeg", "svg", "webp", "pdf", "fig"].contains(&f.as_str()) {
            return Err(format!("unknown format `{f}` (png|jpg|svg|webp|pdf|fig)"));
        }
        params.insert("format".into(), json!(f));
    }
    if let Some(s) = n_arg(a, "scale")? {
        if s <= 0.0 {
            return Err("`scale` must be positive".into());
        }
        params.insert("scale".into(), json!(s));
    }
    if let Some(s) = s_arg(a, "size")? {
        params.insert("size".into(), json!(s));
    }
    // PDF print options and the export options, checked by the command.
    for k in
        ["bleed", "cropMarks", "bleedMarks", "cmyk", "contentsOnly", "includeBoundingBox", "svgOutlineText", "svgIdAttribute", "svgSimplifyStroke"]
    {
        if let Some(v) = a.get(k).filter(|v| !v.is_null()) {
            params.insert(k.into(), v.clone());
        }
    }
    if let Some(p) = s_arg(a, "path")? {
        params.insert("path".into(), json!(p));
        return exec(b, "file.export", Value::Object(params)).map(|v| ToolResult::json(&v));
    }
    let r = exec(b, "file.export", Value::Object(params))?;
    let fmt = r.get("format").and_then(Value::as_str).unwrap_or("png").to_string();
    let bytes = decode_data(&r)?;
    match fmt.as_str() {
        "png" => {
            let (w, h) = png_info(&bytes);
            Ok(ToolResult::image(&bytes, "image/png", &json!({"format": "png", "width": w, "height": h, "bytes": bytes.len()})))
        }
        "jpg" | "jpeg" => Ok(ToolResult::image(&bytes, "image/jpeg", &json!({"format": "jpg", "bytes": bytes.len()}))),
        "webp" => Ok(ToolResult::image(&bytes, "image/webp", &json!({"format": "webp", "bytes": bytes.len()}))),
        "svg" => Ok(ToolResult::text(String::from_utf8_lossy(&bytes).into_owned())),
        other => {
            let mut out = json!({"format": other, "bytes": bytes.len(), "data": base64::engine::general_purpose::STANDARD.encode(&bytes)});
            if let Some(w) = r.get("warnings") {
                out["warnings"] = w.clone();
            }
            Ok(ToolResult::json(&out))
        }
    }
}

/// Largest side of a screenshot unless `maxSize` says otherwise. An image costs the model tokens by
/// its pixel area, so this is the size that still shows a phone screen's text legibly at a quarter
/// of what 1600 cost.
const SCREENSHOT_SIDE: f64 = 800.0;
/// JPEG quality for screenshots: smaller than PNG on photos and textures, text still sharp.
const SCREENSHOT_JPEG_QUALITY: u8 = 80;

/// A rendered PNG re-encoded in the `format` asked for: `(bytes, mime type)`.
fn encode_shot(png: Vec<u8>, format: &str) -> Result<(Vec<u8>, &'static str), String> {
    if format == "png" {
        return Ok((png, "image/png"));
    }
    let pix = figcraft_render::images::decode(&png).ok_or("could not read the rendered image")?;
    match format {
        "jpeg" => Ok((figcraft_render::encode_jpeg(&pix, figcraft_color::Color::WHITE, SCREENSHOT_JPEG_QUALITY)?, "image/jpeg")),
        _ => Ok((figcraft_render::encode_webp(&pix)?, "image/webp")),
    }
}

fn screenshot(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let format = match s_arg(a, "format")?.map(str::to_ascii_lowercase).as_deref() {
        None | Some("png") => "png",
        Some("jpeg" | "jpg") => "jpeg",
        Some("webp") => "webp",
        Some(other) => return Err(format!("`format` must be png, jpeg or webp, not {other:?}")),
    };
    let id = a.get("window").and_then(Value::as_u64);
    if (id.is_some() || matches!(a.get("window"), Some(Value::Bool(true)))) && b.has_ui() {
        let png = match id {
            Some(id) => b.window_png(id)?,
            None => b.screenshot_png()?,
        };
        let (w, h) = png_info(&png);
        let (bytes, mime) = encode_shot(png, format)?;
        return Ok(ToolResult::image(&bytes, mime, &json!({"source": "window", "width": w, "height": h})));
    }
    let max = n_arg(a, "maxSize")?.unwrap_or(SCREENSHOT_SIDE).clamp(16.0, 8192.0);
    let req = n_arg(a, "scale")?.unwrap_or(1.0);
    if req <= 0.0 {
        return Err("`scale` must be positive".into());
    }
    let ids = ids_arg(a, "ids")?.unwrap_or_default();
    let measured = if ids.is_empty() { page_children(b)? } else { ids.clone() };
    let mut scale = req;
    if let Some((x0, y0, x1, y1)) = bounds_of(b, &measured) {
        let side = (x1 - x0).max(y1 - y0).max(1e-6);
        scale = scale.min(max / side);
    }
    let mut bytes = b.render_png(&ids, scale)?;
    let (mut w, mut h) = png_info(&bytes);
    // Layer bounds leave out effects (shadows) and overflow of unclipped frames, but the render
    // includes them: shrink to the measured image until it fits `maxSize` (rounding can add a pixel).
    for _ in 0..4 {
        let side = f64::from(w.max(h));
        if side <= max {
            break;
        }
        scale *= max / side;
        bytes = b.render_png(&ids, scale)?;
        (w, h) = png_info(&bytes);
    }
    let (bytes, mime) = encode_shot(bytes, format)?;
    let mut info = json!({"source": if ids.is_empty() { "page" } else { "layers" }, "scale": scale, "width": w, "height": h});
    if !ids.is_empty() {
        info["ids"] = json!(ids);
    }
    Ok(ToolResult::image(&bytes, mime, &info))
}

fn open_file(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let path = req_s(a, "path")?;
    let mut params = json!({"path": path});
    match a.get("variables") {
        None | Some(Value::Null) => {}
        Some(v @ (Value::String(_) | Value::Object(_))) => params["variables"] = v.clone(),
        Some(_) => return Err("`variables` must be a path or the local variables JSON object".into()),
    }
    let r = exec(b, "file.open", params)?;
    let doc = b.call("document.inspect", json!({"depth": 1}))?;
    Ok(ToolResult::json(&json!({"opened": r, "document": doc})))
}

fn save_file(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let params = match s_arg(a, "path")? {
        Some(p) => json!({"path": p}),
        None => json!({}),
    };
    exec(b, "file.save", params).map(|v| ToolResult::json(&v))
}

fn mods(a: &Map<String, Value>, out: &mut Map<String, Value>) -> Result<(), String> {
    for k in ["shift", "alt", "cmd", "ctrl"] {
        match a.get(k) {
            None | Some(Value::Null) => {}
            Some(Value::Bool(x)) => {
                out.insert(k.into(), json!(x));
            }
            Some(_) => return Err(format!("`{k}` must be a boolean")),
        }
    }
    Ok(())
}

fn pointer(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let x = n_arg(a, "x")?.ok_or("`x` is required")?;
    let y = n_arg(a, "y")?.ok_or("`y` is required")?;
    let kind = s_arg(a, "kind")?.unwrap_or("click");
    if let Some(t) = s_arg(a, "tool")? {
        exec(b, "tool.set", json!({"tool": t}))?;
    }
    let mut p = Map::new();
    p.insert("kind".into(), json!(kind));
    p.insert("x".into(), json!(x));
    p.insert("y".into(), json!(y));
    if let Some(to) = a.get("to").filter(|v| !v.is_null()) {
        let arr =
            to.as_array().filter(|t| t.len() == 2 && t.iter().all(|v| figcraft_engine::params::f64_of(v).is_some())).ok_or("`to` must be [x, y]")?;
        p.insert("to".into(), Value::Array(arr.clone()));
    }
    if let Some(s) = n_arg(a, "steps")? {
        p.insert("steps".into(), json!(s.clamp(1.0, 200.0) as u64));
    }
    for k in ["pressure", "pressureTo"] {
        if let Some(v) = n_arg(a, k)? {
            p.insert(k.into(), json!(v.clamp(0.0, 1.0)));
        }
    }
    mods(a, &mut p)?;
    b.call("ui.pointer", Value::Object(p)).map(|v| ToolResult::json(&v))
}

fn key(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let k = req_s(a, "key")?;
    let mut p = Map::new();
    p.insert("key".into(), json!(k));
    mods(a, &mut p)?;
    b.call("ui.key", Value::Object(p)).map(|v| ToolResult::json(&v))
}

fn type_text(b: &mut dyn Backend, a: &Map<String, Value>) -> Result<ToolResult, String> {
    let text = req_s(a, "text")?;
    match a.get("id").filter(|v| !v.is_null()) {
        Some(id) => {
            let id = id_arg(id)?;
            exec(b, "text.edit.begin", json!({"id": id}))?;
            let typed = b.call("ui.type", json!({"text": text}));
            // Always leave edit mode, even when typing failed.
            let ended = exec(b, "text.edit.end", json!({}));
            typed?;
            ended?;
            let node = b.call("node.get", json!({"id": id, "depth": 0}))?;
            Ok(ToolResult::json(&json!({"ok": true, "node": node})))
        }
        None => b.call("ui.type", json!({"text": text})).map(|v| ToolResult::json(&v)),
    }
}
