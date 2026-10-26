//! The Icons section of the Assets tab: the bundled icon libraries (Lucide, Phosphor, Material
//! Symbols; see `figcraft_engine::icon_sets`), one library and style at a time, searched with the
//! Assets search field. A click inserts the icon (`icon.insert`) in the middle of the view, or into
//! the selected frame when it has auto layout. Only the rows in view are laid out, and previews
//! render when their row first shows (a few per frame) and are cached.

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use figcraft_engine::icon_sets::{self, Icon, IconSet};
use serde_json::json;

use crate::FigApp;
use crate::i18n::{tr, trf};
use crate::theme::{ACCENT, Tokens};

const PAD: f32 = 16.0;
const CELL: f32 = 36.0;
const GAP: f32 = 4.0;
/// Most icons listed at once; a search narrows the rest.
const MAX_LISTED: usize = 1200;
const PREVIEWS_PER_FRAME: usize = 24;
const MAX_PREVIEWS: usize = 1500;
/// Size of an inserted icon.
const INSERT_SIZE: f64 = 24.0;

/// Which library and style the section shows.
#[derive(Clone, Default)]
struct Pick {
    set: usize,
    style: String,
}

/// Rendered previews by (set, name, style, colour).
#[derive(Clone, Default)]
struct Previews {
    map: HashMap<(String, String, String, [u8; 4]), egui::TextureHandle>,
}

/// The Icons section (its title row and, when open, the library tabs, the style menu and the grid).
pub(crate) fn section(app: &mut FigApp, ui: &mut Ui, w: f32, query: &str, open: bool) -> bool {
    let toggled = fold(ui, w, tr("Icons"), open);
    if !open {
        return toggled;
    }
    let sets = icon_sets::sets();
    if sets.is_empty() {
        crate::widgets::note(ui, tr("The icon libraries could not be read."), Tokens::get(ui.ctx()).text_secondary);
        return toggled;
    }
    let pick_id = Id::unique("icon_pick");
    let mut pick: Pick = ui.data(|d| d.get_temp(pick_id)).unwrap_or_default();
    pick.set = pick.set.min(sets.len() - 1);
    // "Material Symbols" is too long for a tab; the credit line below names it in full.
    let titles: Vec<&str> = sets.iter().map(|s| s.title.split_whitespace().next().unwrap_or(&s.title)).collect();
    let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(ui.cursor().min + vec2(PAD, 0.0), vec2(w - PAD * 2.0, 28.0))));
    if let Some(i) = crate::widgets::segmented_text(&mut tabs, &titles, pick.set) {
        pick = Pick { set: i, style: String::new() };
    }
    ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let Some(set) = sets.get(pick.set) else { return toggled };
    if !set.styles.is_empty() {
        if !set.styles.contains(&pick.style) {
            pick.style = set.styles.first().cloned().unwrap_or_default();
        }
        style_menu(ui, w, set, &mut pick);
    }
    ui.data_mut(|d| d.insert_temp(pick_id, pick.clone()));
    let style = (!pick.style.is_empty()).then_some(pick.style.as_str());
    let listed: Vec<&Icon> = if query.is_empty() {
        set.icons.iter().filter(|i| set.body(i, style).is_some()).take(MAX_LISTED).collect()
    } else {
        icon_sets::search(query, Some(set.id), MAX_LISTED).into_iter().map(|(_, i)| i).filter(|i| set.body(i, style).is_some()).collect()
    };
    if listed.is_empty() {
        crate::widgets::note(ui, tr("No matching icons"), Tokens::get(ui.ctx()).text_secondary);
    }
    if let Some(name) = grid(ui, w, set, style, &listed) {
        insert(app, set, &name, style);
    }
    if listed.len() >= MAX_LISTED {
        crate::widgets::note(ui, tr("Search to find the rest."), Tokens::get(ui.ctx()).text_secondary);
    }
    let credit = trf("{title} · {licence} · {package}", &[("title", &set.title), ("licence", &set.licence), ("package", &set.package)]);
    crate::widgets::note(ui, &credit, Tokens::get(ui.ctx()).text_secondary);
    toggled
}

fn fold(ui: &mut Ui, w: f32, title: &str, open: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (hr, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let chevron = Rect::from_center_size(pos2(hr.min.x + PAD + 5.0, hr.center().y), vec2(12.0, 12.0));
    crate::icons::paint(ui.painter(), chevron, if open { crate::icons::Icon::Chevron } else { crate::icons::Icon::ChevronRight }, t.icon_secondary);
    ui.painter().text(pos2(chevron.max.x + 6.0, hr.center().y), Align2::LEFT_CENTER, title, crate::theme::semibold(11.0), t.text);
    resp.on_hover_text(if open { tr("Icons: click to collapse") } else { tr("Icons: click to expand") }).clicked()
}

fn style_menu(ui: &mut Ui, w: f32, set: &IconSet, pick: &mut Pick) {
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(ui.cursor().min + vec2(PAD, 0.0), vec2(w - PAD * 2.0, 28.0))));
    let resp = crate::widgets::dropdown(&mut row, &pick.style, w - PAD * 2.0, None);
    crate::right::menu_popup(&resp).show(|ui| {
        for st in &set.styles {
            if crate::right::menu_item(ui, st, "") {
                pick.style = st.clone();
                ui.close();
            }
        }
    });
    ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
}

/// The icon grid; returns the clicked icon.
fn grid(ui: &mut Ui, w: f32, set: &IconSet, style: Option<&str>, listed: &[&Icon]) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let cols = (((w - PAD * 2.0 + GAP) / (CELL + GAP)).floor() as usize).max(1);
    let rows = listed.len().div_ceil(cols);
    let row_h = CELL + GAP;
    let (all, _) = ui.allocate_exact_size(vec2(w, rows as f32 * row_h), Sense::hover());
    let clip = ui.clip_rect();
    let first = (((clip.min.y - all.min.y) / row_h).floor().max(0.0) as usize).min(rows);
    let last = (((clip.max.y - all.min.y) / row_h).ceil().max(0.0) as usize).min(rows);
    let colour = t.icon;
    let hex = format!("#{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b());
    let px = (CELL - 12.0) * ui.ctx().pixels_per_point();
    let previews_id = Id::unique("icon_previews");
    let mut previews: Previews = ui.data(|d| d.get_temp(previews_id)).unwrap_or_default();
    let mut rendered = 0;
    let mut clicked = None;
    for row in first..last {
        for (k, icon) in listed.iter().skip(row * cols).take(cols).enumerate() {
            let r = Rect::from_min_size(pos2(all.min.x + PAD + k as f32 * (CELL + GAP), all.min.y + row as f32 * row_h), vec2(CELL, CELL));
            let resp = ui.interact(r, Id::unique(("icon", set.id, &icon.name)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
                ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
            }
            let key = (set.id.to_string(), icon.name.clone(), style.unwrap_or("").to_string(), colour.to_array());
            if !previews.map.contains_key(&key) && rendered < PREVIEWS_PER_FRAME {
                rendered += 1;
                if let Some(tex) = preview(ui, set, icon, style, f64::from(px), &hex) {
                    previews.map.insert(key.clone(), tex);
                }
            }
            match previews.map.get(&key) {
                Some(tex) => {
                    let img = Rect::from_center_size(r.center(), vec2(CELL - 12.0, CELL - 12.0));
                    ui.painter().image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    ui.painter().rect_filled(r.shrink(10.0), CornerRadius::same(3), t.field);
                    ui.ctx().request_repaint();
                }
            }
            let tags = if icon.tags.is_empty() { String::new() } else { format!("\n{}", icon.tags.join(", ")) };
            if resp.on_hover_text(trf("{name}{tags}\nClick to insert.", &[("name", &icon.name), ("tags", &tags)])).clicked() {
                clicked = Some(icon.name.clone());
            }
        }
    }
    if previews.map.len() > MAX_PREVIEWS {
        previews.map.clear();
    }
    ui.data_mut(|d| d.insert_temp(previews_id, previews));
    clicked
}

fn preview(ui: &Ui, set: &IconSet, icon: &Icon, style: Option<&str>, px: f64, hex: &str) -> Option<egui::TextureHandle> {
    let (doc, frame) = icon_sets::preview(set, icon, style, px, hex)?;
    let pix = figcraft_render::render_node(&doc, frame, 1.0)?;
    let img = crate::render_worker::screen_image(&pix, doc.color_profile);
    Some(ui.ctx().load_texture(format!("icon-{}-{}-{}", set.id, icon.name, style.unwrap_or("")), img, egui::TextureOptions::LINEAR))
}

/// Insert into the selected auto layout frame, else centred in the view.
fn insert(app: &mut FigApp, set: &IconSet, name: &str, style: Option<&str>) {
    if crate::menus::dev_mode_refuses(app, "icon.insert") {
        return;
    }
    let mut params = json!({"set": set.id, "name": name, "size": INSERT_SIZE, "color": "#000000"});
    if let Some(st) = style {
        params["style"] = json!(st);
    }
    let into = app.session.ds().ok().and_then(|ds| match ds.selection.as_slice() {
        [one] => ds.doc.node(*one).filter(|n| n.layout.is_some()).map(|_| *one),
        _ => None,
    });
    match into {
        Some(parent) => params["parent"] = json!(parent),
        None => {
            let (cx, cy) = app.session.ds().map(|d| (d.view.cx, d.view.cy)).unwrap_or((0.0, 0.0));
            params["x"] = json!((cx - INSERT_SIZE / 2.0).round());
            params["y"] = json!((cy - INSERT_SIZE / 2.0).round());
        }
    }
    app.run("icon.insert", params);
}
