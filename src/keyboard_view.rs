//! The clickable keyboard drawing shared by the key mapping and actuation pages.

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use fizz::keymap::{LAYOUT, LAYOUT_SIZE, PhysKey};

use crate::graphite::{self, Type, radius, size, space};

/// Largest drawing scale, so the keyboard doesn't get cartoonishly big on a
/// wide window. Layout units are about 1 px per unit at scale 1.
const MAX_SCALE: f32 = 1.6;

/// What to draw on one keycap.
pub struct KeyFace {
    /// Small line at the top, e.g. the keycap label. Empty for none.
    pub top: String,
    /// Main text.
    pub main: String,
    pub main_color: Color32,
    /// Shows the unsaved-change dot.
    pub unsaved: bool,
    pub tooltip: String,
}

pub struct Click {
    pub slot: usize,
    /// Ctrl or Shift was held: add to / remove from a selection.
    pub extend: bool,
}

/// Draw the keyboard at up to `max_height_share` of the available height.
/// Returns the key that was clicked, if any.
pub fn keyboard_view(
    ui: &mut Ui,
    salt: &str,
    max_height_share: f32,
    is_selected: impl Fn(usize) -> bool,
    face: impl Fn(&PhysKey) -> KeyFace,
) -> Option<Click> {
    let p = graphite::palette(ui);
    let width = ui.available_width();
    let scale = (width / LAYOUT_SIZE.0)
        .min(ui.available_height() * max_height_share / LAYOUT_SIZE.1)
        .min(MAX_SCALE);
    let (area, _) = ui.allocate_exact_size(Vec2::new(width, LAYOUT_SIZE.1 * scale), Sense::hover());
    // Centre the drawing when it's narrower than the page.
    let origin = area.left_top() + Vec2::new((width - LAYOUT_SIZE.0 * scale) / 2.0, 0.0);
    let mut clicked = None;

    for key in LAYOUT {
        let rect = Rect::from_min_size(
            origin + Vec2::new(key.x, key.y) * scale,
            Vec2::new(key.w, key.h) * scale,
        );
        let id = ui.scope_id().with((salt, key.slot));
        let response = ui.interact(rect, id, Sense::click());
        if response.clicked() {
            let m = ui.input(|i| i.modifiers);
            clicked = Some(Click {
                slot: key.slot,
                extend: m.command || m.shift,
            });
        }

        let selected = is_selected(key.slot);
        let face = face(key);
        let fill = if selected {
            p.accent_subtle
        } else if response.hovered() {
            p.hover
        } else {
            p.surface
        };
        let stroke = if selected || response.has_focus() {
            Stroke::new(size::BORDER, p.accent)
        } else if response.hovered() {
            Stroke::new(size::BORDER, p.border_strong)
        } else {
            Stroke::new(size::BORDER, p.border)
        };
        let painter = ui.painter();
        painter.rect(rect, radius::MD, fill, stroke, StrokeKind::Inside);

        let wrap = rect.width() - space::S2 * 2.0;
        let main = painter.layout(face.main, Type::Caption.font(), face.main_color, wrap);
        if face.top.is_empty() {
            painter.galley(rect.center() - main.size() / 2.0, main, face.main_color);
        } else {
            let top = painter.layout_no_wrap(face.top, Type::Caption.font(), p.text_muted);
            let total = top.size().y + main.size().y;
            let y = rect.center().y - total / 2.0;
            painter.galley(
                egui::pos2(rect.center().x - top.size().x / 2.0, y),
                top.clone(),
                p.text_muted,
            );
            painter.galley(
                egui::pos2(rect.center().x - main.size().x / 2.0, y + top.size().y),
                main,
                face.main_color,
            );
        }

        if face.unsaved {
            // 8px status dot marking an unsaved change.
            painter.circle_filled(
                rect.right_top() + Vec2::new(-space::S3, space::S3),
                space::S2,
                p.accent,
            );
        }
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(face.tooltip);
    }
    clicked
}
