//! `read_guide`: the detail tool descriptions leave out. Every tool definition is sent to the model
//! at the start of each session whether it is used or not, so the definitions stay short and the
//! rest is read here, one topic at a time, when an agent needs it.

/// `(topic, one-line summary, text)`, in the order the index lists them.
pub const TOPICS: &[(&str, &str, &str)] = &[
    ("create", "create_node, create_tree, batch and $step references", CREATE),
    ("properties", "every set_properties key: paints, gradients, effects, auto layout, text", PROPERTIES),
    ("inspect", "inspect_document / get_node: compact JSON, outline format, size limits", INSPECT),
    ("look", "screenshot and export: sizes, formats, PDF and SVG options, windows", LOOK),
    ("commands", "list_commands and run_command: the command catalog", COMMANDS),
    ("input", "pointer, key and type_text: driving the canvas like a user", INPUT),
    ("files", "open_file and save_file: formats, .fig, REST API JSON", FILES),
    ("pen", "importing a pen.dev design through pen.dev's own MCP tools", PEN),
];

/// The topic's text, or the index when `topic` is none or unknown.
pub fn read(topic: Option<&str>) -> Result<String, String> {
    match topic {
        None | Some("" | "index") => Ok(index()),
        Some(t) => TOPICS
            .iter()
            .find(|(name, _, _)| name.eq_ignore_ascii_case(t))
            .map(|(_, _, text)| (*text).to_string())
            .ok_or_else(|| format!("no guide `{t}`; {}", index())),
    }
}

fn index() -> String {
    let mut out = String::from("Guides (read_guide {topic}):\n");
    for (name, summary, _) in TOPICS {
        out.push_str(&format!("- {name}: {summary}\n"));
    }
    out
}

const CREATE: &str = "\
Creating layers

Coordinates are page units (1 unit = 1 px at 100%), x right, y DOWN. x/y are relative to the parent \
frame (the page when there is no parent, so page coordinates); groups don't count as parents for \
coordinates. Inside an auto-layout frame the layout positions children: set layout, gap, padding, \
mainAlign, crossAlign, sizingH/sizingV on the frame instead of x/y on the child.

create_node {type, x?, y?, width?, height?, parent?, index?, name?, text?, path?, props?, verbose?}
- type: frame, rectangle, ellipse, polygon, star, line, arrow, text, vector, section, component, slice.
- width/height default 100. Text hugs its content unless width is set, which makes it wrap. For a \
vector with `path` (SVG path data, e.g. \"M0 0 L100 0 L50 80 Z\"), width/height scale the path, one \
alone keeping its proportion; without them the path's own box is used. height is ignored for \
lines/arrows and auto-height text.
- index: z-index among the parent's children (default on top).
- props: any set_properties keys applied at creation, e.g. \
{\"fill\":\"#3366FF\",\"cornerRadius\":12,\"fontSize\":24,\"layout\":\"vertical\",\"gap\":8,\"padding\":16}.
- Returns {id, x, y, width, height}, read back after the layout ran; verbose: true returns {id, node} \
with the whole layer. The new layer becomes the selection.

create_tree {nodes, parent?, label?}: a whole screen in ONE call and one undo step. `nodes` is a \
layer (or an array) with the create_node keys plus `children`, nested up to 64 levels and 5000 \
layers:
{\"nodes\":{\"type\":\"frame\",\"name\":\"Login\",\"width\":390,\"height\":844,\
\"props\":{\"fill\":\"#FFFFFF\",\"layout\":\"vertical\",\"padding\":24,\"gap\":16},\"children\":[\
{\"type\":\"text\",\"text\":\"Welcome back\",\"props\":{\"fontSize\":28}},\
{\"type\":\"rectangle\",\"name\":\"Email\",\"width\":342,\"height\":48,\"props\":{\"fill\":\"#F1F3F5\",\"cornerRadius\":8}}]}}
Icons are layers too: {\"type\":\"icon\",\"set\":\"lucide\",\"icon\":\"search\",\"size\":20,\"color\":\"#475467\"} \
(sets lucide, phosphor with style regular|thin|light|bold|fill|duotone, material with style \
outlined|rounded|sharp and their -fill; find names with run_command icon.search {query, set?}). \
All or nothing: a failing layer creates nothing and the error gives its place (nodes[0].children[1]). \
Returns the new layers as an outline (each id and where the layout put it); the roots become the \
selection.

batch {commands: [{command, params}], label?}: several commands in order as ONE undo step. If one \
fails, everything is rolled back and the error starts with `step N:`. Returns the results in order. \
A later step can use what an earlier one returned: anywhere in its params {\"$step\": n} stands for \
the `id` in step n's result (zero-based); {\"$step\": n, \"field\": \"…\"} picks another field. Example: \
[{\"command\":\"node.create\",\"params\":{\"type\":\"frame\"}}, \
{\"command\":\"node.create\",\"params\":{\"type\":\"text\",\"text\":\"Hi\",\"parent\":{\"$step\":0}}}]. \
Use batch for mixed commands (create, then align, group, make a component…); create_tree for layer trees.
";

const PROPERTIES: &str = "\
set_properties {ids?, props, verbose?}: node.set on layers (default the selection), one undo step.

Layer: name, visible, locked, opacity (0-1), blend, x, y (relative to the parent frame; groups don't \
count), width, height, rotation (degrees), clip, constraints {horizontal: left|right|leftRight|center|scale, \
vertical: top|bottom|topBottom|center|scale}.
Paints: fill (one colour) or fills (array). A paint is \"#RRGGBB\" / \"#RRGGBBAA\", \
{\"type\":\"solid\",\"color\":\"#hex\",\"opacity\"?}, or a gradient \
{\"type\":\"linearGradient\",\"stops\":[\"#hex\",\"#hex\"]} (stops may be [{\"position\":0..1,\"color\":\"#hex\"}], \
optional from/to handles in the unit box; radialGradient, angularGradient and diamondGradient too). \
stroke / strokes the same, strokeWeight, strokeAlign (inside|center|outside).
cornerRadius: n or [tl, tr, br, bl].
effects: array, e.g. {\"type\":\"dropShadow\",\"color\":\"#00000040\",\"offset\":[0,4],\"radius\":8} \
(innerShadow, layerBlur, backgroundBlur, noise, texture and glass too).
Auto layout (frames, components, instances): layout (null|horizontal|vertical|grid), gap, padding \
(n or [t,r,b,l]), mainAlign (min|center|max|spaceBetween|spaceAround|spaceEvenly), crossAlign, wrap, \
contentAlign (auto|spaceBetween: wrapped lines). Other layers among the ids are skipped and named in \
`warnings`; with none that can take them the call is an error.
Children of auto layout: sizingH / sizingV (fixed|hug|fill).
Text: characters, fontFamily, fontStyle, fontSize, lineHeight, letterSpacing, textAlign, \
textAutoResize (widthAndHeight|height|none|truncate). A fontStyle draws in the family's nearest face; \
`warnings` says when that face has another weight or slant than the name asks for.

Unknown keys are an error. list_commands {filter: \"node.set\"} has the full list (masks, dev status, \
plugin data, …). Returns {ok, nodes: [{id, x, y, width, height}], warnings?}; verbose: true returns the \
whole layers.
";

const INSPECT: &str = "\
inspect_document {depth?, allPages?, format?, verbose?, maxBytes?}: the active document: name, \
dirty flag, current page, selection, the current page's layer tree (default depth 4), other pages \
summarised, styles, variable count and the undo history.
get_node {id, depth?, offset?, format?, verbose?, maxBytes?}: one layer with its children to depth \
(default 1).

Layer JSON: id, name, type, x/y relative to the parent frame (groups don't count), width, height, \
absoluteBounds in page coordinates, fills, strokes, effects, cornerRadius, layout, sizing, text \
properties, component links, overrides, bound variables.
Compact by default: what a new layer has anyway is left out (no clip = clips, no sizing = fixed, no \
mainAlign/crossAlign = min, no textAlign = left, no absoluteBounds = same as x/y), a plain solid \
paint is \"#hex\" and uniform padding one number, as set_properties takes them. verbose: true spells \
everything out.

format: \"outline\" sends one text line per layer, indented by depth: `id TYPE \"name\" x,y WxH` then \
only what differs: fills as #hex, stroke#hex/weight, r (radius), rot, opacity, hidden, locked, noclip, \
auto layout mode with gap, pad, main:/cross: alignment, wrap; w:/h: sizing, pin:h/v constraints; text \
characters, family/style/size, align:, resize:; an instance's of:id; (n children) below the depth \
asked for. Anything else is named after `+` (e.g. +effects,interactions): get_node reads it. The \
cheapest way to check a screen's structure.

Size: each level of depth multiplies the response and allPages multiplies it again. A tree over \
maxBytes (default 8 MiB, the usual client buffer) comes back shallower with a `truncated` object \
saying so (outline: its last lines cut, with a closing line). get_node reads one subtree; `offset` \
pages through a layer with more children than fit.
";

const LOOK: &str = "\
screenshot {ids?, scale?, maxSize?, format?, window?}: renders the layers (or the whole current \
page with its background) and returns an image plus its size and the scale used. It is scaled down \
to fit maxSize (default 800). An image costs tokens by its pixel area (about width x height / 750), \
so screenshot just the layers in question, and ask for a larger maxSize only to read small detail. \
format png (default), webp (lossless, usually a fraction of PNG's bytes for flat UI) or jpeg \
(quality 80 over white, smaller only for photos); the format changes the bytes, not the token cost. \
window: true (or a window id from inspect_ui's `windows`) captures the attached app's window \
including the UI.

export {ids?, format?, scale? | size?, path?, …}: layers (default the selection, else every \
top-level layer) as png, jpg, svg, webp or pdf, or the whole file as fig (experimental, with \
`warnings` for what was left out). size: 2x, 512w or 512h instead of scale (max 64). With path the \
file is written there and the result says where; without it png/jpg/webp come back as an image, svg \
as text, other formats as base64 `data`.
Options: contentsOnly (default true; false also draws every other layer showing in the area), \
includeBoundingBox (the layers' own boxes, cutting off shadows). PDF: bleed (points or \"3mm\", \
\"0.125in\"), cropMarks, bleedMarks, cmyk. SVG: svgOutlineText (default true), svgIdAttribute, \
svgSimplifyStroke (default true).
";

const COMMANDS: &str = "\
Every menu item, panel action and shortcut is a command. list_commands {filter?} lists id, label, \
menu path, shortcut, whether it is undoable (*) and a parameter description; filter by a substring \
(\"align\", \"layout\", \"variable\", \"style\", \"component\", \"proto\", \"export\").

run_command {command, params?} runs one; mutating commands are one undo step each and a failing \
command leaves the document unchanged. Most commands act on `ids` when given, else on the selection. \
Examples: {\"command\":\"align.left\",\"params\":{\"ids\":[3,4]}}, \
{\"command\":\"layout.add\",\"params\":{\"ids\":[2],\"direction\":\"vertical\"}}, \
{\"command\":\"style.create\",\"params\":{\"type\":\"paint\",\"name\":\"Brand/Primary\",\"from\":5}}. \
Creation commands return {\"id\"}. Programmatic calls never open dialogs. Several commands as one undo \
step: batch (read_guide create).
select {ids} replaces the selection (switching to their page); [] deselects. undo / redo step through \
history (one command, batch or gesture each).
";

const INPUT: &str = "\
pointer {kind?, x, y, to?, steps?, tool?, shift?, alt?, cmd?, pressure?, pressureTo?}: the mouse in \
page coordinates with the current tool, exactly as a user: kind click (default), double, drag, down, \
move, up, hover. Set `tool` first to draw: move|scale|hand|frame|section|slice|rectangle|line|arrow|\
ellipse|polygon|star|pen|pencil|text|comment, e.g. {\"tool\":\"rectangle\",\"kind\":\"drag\",\"x\":0,\"y\":0,\"to\":[200,100]}. \
A dragged layer drops into the frame or auto layout slot under the pointer. double on a text layer \
starts editing it. pressure / pressureTo (0-1) is pen pressure, interpolated along a drag. Returns the \
selection and tool.

key {key, shift?, alt?, cmd?}: a key or shortcut as if typed on the canvas: menu shortcuts \
({\"key\":\"G\",\"cmd\":true} groups), tool keys (R, O, T, F, V…), Escape, Delete, Enter, arrows (nudge; \
shift = big nudge). cmd is Cmd on macOS and Ctrl elsewhere. While editing text, keys go to the text \
editor.

type_text {text, id?}: with id, replaces that text layer's contents; without, types at the caret of \
the text being edited.

inspect_ui: tool, view (zoom, centre), status; panels, timings and the app's windows when attached.
";

const FILES: &str = "\
open_file {path, variables?}: opens .figcraft, .json (FigCraft JSON, or REST API file JSON with its \
variables: `variables` is the local variables JSON or its path, default <name>.variables.json or \
variables.json next to the file), .fig (best effort, with `warnings` for what wasn't imported), .svg \
(becomes vectors) or an image (becomes an image layer) as the active document. Returns a summary and \
the document's outline.

save_file {path?}: saves as .figcraft; path is required the first time, later saves reuse it. A path \
ending in .fig writes an experimental .fig copy instead and leaves the document's own path and unsaved \
changes alone.
";

const PEN: &str = "\
Importing from pen.dev

A .pen file is encrypted and pen.dev only opens it through its own tools, so FigCraft never reads \
it: read the design with pen.dev's MCP `execute`, then hand the JSON to FigCraft.

1. In pen.dev, with the .pen file open, run this `execute` snippet with R set to the frame's id (ids \
from pen.dev's get_app_state, or a Get visitor). It prints the node with variables and instances \
resolved, path geometry included, ids dropped and each node's resolved box as `b`:

const R=\"FRAME_ID\"
const b={}
Get(R,(n,c)=>{b[n.id]=[c.bounds.x,c.bounds.y,c.bounds.width,c.bounds.height].map(v=>Math.round(v*100)/100)},{resolveInstances:true})
const t=Get(R,{resolveVariables:true,resolveInstances:true,includePathGeometry:true})
const w=n=>{if(b[n.id])n.b=b[n.id];delete n.id;delete n.metadata;delete n.context;(n.children||[]).forEach(w)}
w(t)
Print(JSON.stringify(t))

2. In FigCraft: run_command {\"command\":\"import.pen\",\"params\":{\"nodes\": <the printed JSON>, \"x\"?: 0, \
\"y\"?: 0, \"parent\"?: id}}. One undo step; returns {ids, layers, warnings}; the roots are selected. \
Check it with get_node {id, format: \"outline\", depth: 8} and a screenshot.

Design tokens: in pen.dev run Print(JSON.stringify(GetVariables())), then run_command \
{\"command\":\"import.penVariables\",\"params\":{\"variables\": <the printed JSON>}}. Each theme axis \
becomes a collection with a mode per value, unthemed variables go to \"Tokens\", `$name` values stay \
aliases; importing again updates rather than doubles. Import the tokens first, then read frames \
WITHOUT resolveVariables (Get(R,{resolveInstances:true,includePathGeometry:true})): import.pen then \
binds each `$name` (fills, strokes, effect colours, radii, padding, gap, sizes, font family and \
size, text) to the variable of that name, with the value of the mode a node's `theme` picks \
(theme {mode: dark} → the frame uses the dark mode). bind: false keeps plain values.

Several frames: one array in `nodes` (the first root goes to x/y, the rest keep their places). The \
JSON passes through you, so import a screen at a time rather than a whole file.

Mapping: frames → frames (no `layout` = horizontal; `none` = no auto layout; fit_content = hug, \
fill_container = fill; pen.dev frames don't clip unless `clip`), reusable frames → components, groups → \
frames without auto layout, rectangle/ellipse (arcs)/polygon → the same, path → vector (viewBox \
mapped onto the box), text → text (weights as style names, lineHeight ratio as %, textGrowth as auto \
resize). Fills, gradients, strokes (per side too), corner radii, shadows and blurs come along. Icons \
(lucide, feather, phosphor, Material Symbols) come from FigCraft's own copies of those libraries in \
their fill colour (one not found: an empty frame named after it). Not imported, each named in \
`warnings`: image and shader fills, unresolved instances or variables, notes, prompts, scripts, \
browsers, links, flips, blend modes and caps.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_lists_every_topic_and_each_topic_reads() {
        let idx = read(None).unwrap();
        for (name, _, text) in TOPICS {
            assert!(idx.contains(&format!("- {name}:")), "{idx}");
            assert_eq!(read(Some(name)).unwrap(), *text);
            assert!(text.len() > 200, "{name}");
        }
        assert_eq!(read(Some("INSPECT")).unwrap(), INSPECT);
        assert_eq!(read(Some("index")).unwrap(), idx);
        let e = read(Some("nope")).unwrap_err();
        assert!(e.contains("no guide `nope`") && e.contains("- create:"), "{e}");
    }
}
