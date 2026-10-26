//! The agent's cursor: while an agent (an MCP client or a script on the control channel) edits,
//! a pointer with its name, the same as a live session's people have, glides to the layers it just
//! changed, which are outlined for a moment; a few seconds after its last edit it fades away. The
//! engine records which layers ([`figcraft_engine::agent`]); this only draws. Preferences turn it
//! off and give its name and colour.

use egui::{Align2, Pos2, Rect, Shape, Stroke, vec2};

use crate::FigApp;
use crate::live::{color, name_tag, pointer};

/// How fast the cursor closes in on the layers it edited (per second): slower than a person's,
/// so its path reads.
const GLIDE: f64 = 9.0;
/// The edited layers stay outlined this long, then the outline fades over `OUTLINE_FADE` (seconds).
const OUTLINE_HOLD: f64 = 0.6;
const OUTLINE_FADE: f64 = 0.6;
/// The cursor stays this long after the agent's last edit, then fades over `FADE` (seconds).
const IDLE: f64 = 3.0;
const FADE: f64 = 0.8;
/// How far from the canvas's edge the cursor stays when its layers are off screen.
const MARGIN: f32 = 8.0;

/// The agent's cursor as drawn.
#[derive(Default)]
pub struct AgentCursorUi {
    /// The last edit seen (`AgentEdit::serial`) and when it arrived (UI clock).
    serial: u64,
    since: f64,
    /// Where the cursor is drawn (page coordinates), gliding to the edited layers; `None`: hidden.
    shown: Option<(f64, f64)>,
    /// The UI clock at the last frame it was drawn.
    last: f64,
}

/// How opaque something that holds for `hold` seconds and then fades over `fade` is `age`
/// seconds in (0: gone); asks for the frame where it next looks different.
fn fade(ctx: &egui::Context, age: f64, hold: f64, fade: f64, reduce: bool) -> f32 {
    let after = |s: f64| ctx.request_repaint_after(std::time::Duration::from_secs_f64(s.max(0.0)));
    if age < hold {
        after(hold - age);
        1.0
    } else if age >= hold + fade {
        0.0
    } else if reduce {
        after(hold + fade - age);
        1.0
    } else {
        ctx.request_repaint();
        (1.0 - (age - hold) / fade) as f32
    }
}

/// Draw the agent's cursor and the outline of what it just changed.
pub fn draw(app: &mut FigApp, painter: &egui::Painter, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let edit = app.session.agent.clone().filter(|_| app.session.prefs.agent_cursor);
    let page = app.session.ds().ok().map(|d| (d.uid, d.page));
    let Some(edit) = edit.filter(|e| page == Some((e.doc, e.page))) else {
        app.ui.agent.shown = None;
        return;
    };
    if std::mem::replace(&mut app.ui.agent.serial, edit.serial) != edit.serial {
        app.ui.agent.since = now;
    }
    let reduce = app.reduce_motion();
    let age = now - app.ui.agent.since;
    let alpha = fade(ctx, age, IDLE, FADE, reduce);
    let Ok(doc) = app.session.doc().cloned() else { return };
    let quads: Vec<[Pos2; 4]> = edit.nodes.iter().filter_map(|n| crate::canvas::node_quad(app, &doc, *n)).collect();
    if alpha <= 0.0 || quads.is_empty() {
        app.ui.agent.shown = None;
        return;
    }
    let fill = color(&app.session.prefs.agent_color);
    let outline = fade(ctx, age, OUTLINE_HOLD, OUTLINE_FADE, reduce);
    if outline > 0.0 {
        for q in &quads {
            painter.add(Shape::closed_line(q.to_vec(), Stroke::new(1.5, fill.gamma_multiply(outline))));
        }
    }
    // The tip sits at the lower right of what changed, kept on the canvas.
    let bounds = Rect::from_points(quads.as_flattened());
    let area = app.canvas.rect.shrink(MARGIN);
    let corner = bounds.right_bottom().clamp(area.min, area.max.max(area.min));
    let target = app.to_page(corner);
    let (tx, ty) = (target.x, target.y);
    let dt = (now - app.ui.agent.last).clamp(0.0, 0.1);
    app.ui.agent.last = now;
    let k = if reduce { 1.0 } else { 1.0 - (-dt * GLIDE).exp() };
    let (mut x, mut y) = app.ui.agent.shown.unwrap_or((tx, ty));
    x += (tx - x) * k;
    y += (ty - y) * k;
    let zoom = app.session.ds().map_or(1.0, |d| d.view.zoom).max(1e-9);
    // Within half a pixel: there.
    if ((tx - x) * zoom).hypot((ty - y) * zoom) < 0.5 {
        (x, y) = (tx, ty);
    } else {
        ctx.request_repaint();
    }
    app.ui.agent.shown = Some((x, y));
    let mut faded = painter.clone();
    faded.multiply_opacity(alpha);
    let tip = app.to_screen(x, y);
    pointer(&faded, tip, fill);
    name_tag(&faded, tip + vec2(12.0, 16.0), Align2::LEFT_TOP, &app.session.prefs.agent_name, fill);
}

/// Where the cursor is drawn (`ui.inspect`): page coordinates, or `None` while hidden.
pub fn shown(app: &FigApp) -> Option<(f64, f64)> {
    app.ui.agent.shown
}
