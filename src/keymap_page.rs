//! The Key mapping page: a drawing of the keyboard, a picker for what the
//! selected key does, and saving both layers back to the keyboard.

use std::collections::HashSet;
use std::sync::mpsc::Sender;

use egui::{Align, Layout, ScrollArea, Ui, Vec2};
use lucide_icons::Icon;

use fizz::keymap::{self, CATALOGUE, Group, LAYOUT, PhysKey};
use fizz::protocol::{KeyAction, Layer};

use crate::graphite::{self, Tone, Type, size, space};
use crate::keyboard_view::{KeyFace, keyboard_view};
use crate::worker::{Request, Snapshot};

pub enum Outcome {
    None,
    Saved,
}

#[derive(Default)]
pub struct KeymapPage {
    layer: Option<Layer>,
    selected: Option<usize>,
    search: String,
    /// Layers as stored on the keyboard.
    stored_default: Option<Vec<KeyAction>>,
    stored_fn: Option<Vec<KeyAction>>,
    /// Layers being edited.
    edit_default: Option<Vec<KeyAction>>,
    edit_fn: Option<Vec<KeyAction>>,
    saving: HashSet<Layer>,
}

impl KeymapPage {
    fn layer(&self) -> Layer {
        self.layer.unwrap_or(Layer::Default)
    }

    fn stored(&self, layer: Layer) -> Option<&Vec<KeyAction>> {
        match layer {
            Layer::Default => self.stored_default.as_ref(),
            Layer::Fn => self.stored_fn.as_ref(),
        }
    }

    fn edit(&self, layer: Layer) -> Option<&Vec<KeyAction>> {
        match layer {
            Layer::Default => self.edit_default.as_ref(),
            Layer::Fn => self.edit_fn.as_ref(),
        }
    }

    fn edit_mut(&mut self, layer: Layer) -> Option<&mut Vec<KeyAction>> {
        match layer {
            Layer::Default => self.edit_default.as_mut(),
            Layer::Fn => self.edit_fn.as_mut(),
        }
    }

    fn is_changed(&self, layer: Layer) -> bool {
        self.edit(layer) != self.stored(layer)
    }

    fn any_changed(&self) -> bool {
        self.is_changed(Layer::Default) || self.is_changed(Layer::Fn)
    }

    pub fn select(&mut self, slot: usize) {
        self.selected = Some(slot);
    }

    pub fn set_layer(&mut self, layer: Layer) {
        self.layer = Some(layer);
    }

    pub fn is_saving(&self) -> bool {
        !self.saving.is_empty()
    }

    /// New data from the keyboard. Unsaved edits are kept.
    pub fn on_snapshot(&mut self, snapshot: &Snapshot) {
        for layer in [Layer::Default, Layer::Fn] {
            let incoming = match layer {
                Layer::Default => snapshot.keymap_default.clone(),
                Layer::Fn => snapshot.keymap_fn.clone(),
            };
            let unchanged = !self.is_changed(layer);
            match layer {
                Layer::Default => self.stored_default = incoming.clone(),
                Layer::Fn => self.stored_fn = incoming.clone(),
            }
            if unchanged || self.edit(layer).is_none() {
                match layer {
                    Layer::Default => self.edit_default = incoming,
                    Layer::Fn => self.edit_fn = incoming,
                }
            }
        }
    }

    pub fn on_saved(&mut self, layer: Layer, stored: Vec<KeyAction>) -> Outcome {
        match layer {
            Layer::Default => {
                self.stored_default = Some(stored.clone());
                self.edit_default = Some(stored);
            }
            Layer::Fn => {
                self.stored_fn = Some(stored.clone());
                self.edit_fn = Some(stored);
            }
        }
        self.saving.remove(&layer);
        if self.saving.is_empty() {
            Outcome::Saved
        } else {
            Outcome::None
        }
    }

    pub fn on_error(&mut self) {
        self.saving.clear();
    }

    pub fn reset(&mut self) {
        *self = Self {
            layer: self.layer,
            ..Self::default()
        };
    }

    // -----------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let layer = self.layer();
        let Some(keys) = self.edit(layer).cloned() else {
            graphite::empty_state(
                ui,
                Some(Icon::CircleAlert.unicode()),
                "Couldn't read the key map",
                "The keyboard didn't send a complete key map. Read it again to retry.",
                Some("Read again"),
            )
            .then(|| tx.send(Request::Refresh));
            return;
        };

        self.action_bar(ui, tx);
        ui.add_space(space::S4);

        if layer == Layer::Default && !keymap::has_fn_key(&keys) {
            graphite::banner(
                ui,
                Tone::Warning,
                Icon::TriangleAlert.unicode(),
                "No key is set to Fn, so you can't reach the Fn layer. Set one before saving.",
            );
            ui.add_space(space::S4);
        }

        let stored = self.stored(layer).cloned();
        if let Some(slot) = self.keyboard(ui, layer, &keys, stored.as_deref()) {
            self.selected = Some(slot);
        }
        ui.add_space(space::S5);
        ui.add_enabled_ui(!self.is_saving(), |ui| self.inspector(ui, layer, &keys));
    }

    fn action_bar(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let p = graphite::palette(ui);
        ui.horizontal(|ui| {
            let mut layer = self.layer();
            let label = |l: Layer, name: &'static str| -> String {
                if self.is_changed(l) { format!("{name} •") } else { name.to_owned() }
            };
            let default_label = label(Layer::Default, "Default layer");
            let fn_label = label(Layer::Fn, "Fn layer");
            if graphite::segmented(
                ui,
                &mut layer,
                &[(Layer::Default, &default_label), (Layer::Fn, &fn_label)],
            ) {
                self.layer = Some(layer);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.is_saving() {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(graphite::text("Saving", Type::Body).color(p.text_secondary));
                    return;
                }
                let changed = self.any_changed();
                if graphite::primary_button(ui, "Save key map").clicked() && changed {
                    for l in [Layer::Default, Layer::Fn] {
                        if self.is_changed(l)
                            && let Some(keys) = self.edit(l).cloned()
                        {
                            self.saving.insert(l);
                            let _ = tx.send(Request::SaveKeymap(l, keys));
                        }
                    }
                }
                if ui
                    .add_enabled(changed, egui::Button::new("Discard changes"))
                    .clicked()
                {
                    self.edit_default = self.stored_default.clone();
                    self.edit_fn = self.stored_fn.clone();
                }
                if !changed {
                    ui.label(graphite::text("No unsaved changes", Type::Small).color(p.text_muted));
                }
            });
        });
    }

    /// Draw the keyboard. Returns a slot when a key was clicked.
    fn keyboard(
        &self,
        ui: &mut Ui,
        layer: Layer,
        keys: &[KeyAction],
        stored: Option<&[KeyAction]>,
    ) -> Option<usize> {
        let p = graphite::palette(ui);
        // Leave at least half the height for the picker below.
        keyboard_view(
            ui,
            "keymap",
            0.5,
            |slot| self.selected == Some(slot),
            |key| {
                let action = keys[key.slot];
                let (main, main_color) = key_caption(layer, key, action, p);
                KeyFace {
                    top: String::new(),
                    main,
                    main_color,
                    unsaved: stored.is_some_and(|s| s[key.slot] != action),
                    tooltip: format!("{}: {}", key.label, describe_in(layer, action)),
                    swatch: None,
                }
            },
        )
        .map(|click| click.slot)
    }

    fn inspector(&mut self, ui: &mut Ui, layer: Layer, keys: &[KeyAction]) {
        let p = graphite::palette(ui);
        let Some(slot) = self.selected else {
            graphite::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(space::S5);
                graphite::empty_state(
                    ui,
                    Some(Icon::Keyboard.unicode()),
                    "Choose a key",
                    "Click a key above to change what it does.",
                    None,
                );
                ui.add_space(space::S5);
            });
            return;
        };
        let Some(phys) = LAYOUT.iter().find(|k| k.slot == slot) else {
            return;
        };
        let current = keys[slot];
        let stored = self.stored(layer).map(|s| s[slot]);
        let reset_to = match layer {
            Layer::Default => phys.default,
            Layer::Fn => KeyAction::default(),
        };

        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            // Header: which key, what it does, reset.
            egui::Sides::new().height(size::ROW).show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = space::S1;
                        let layer_name = match layer {
                            Layer::Default => "",
                            Layer::Fn => "Fn + ",
                        };
                        ui.label(graphite::text(
                            format!("{layer_name}{} key", phys.label),
                            Type::Heading,
                        ));
                        let mut line = format!("Does: {}", describe_in(layer, current));
                        if let Some(stored) = stored.filter(|s| *s != current) {
                            line.push_str(&format!(". On the keyboard now: {}", describe_in(layer, stored)));
                        }
                        ui.label(graphite::text(line, Type::Small).color(p.text_secondary));
                    });
                },
                |ui| {
                    let label = match layer {
                        Layer::Default => "Reset key",
                        Layer::Fn => "Use built-in Fn action",
                    };
                    if ui
                        .add_enabled(current != reset_to, egui::Button::new(label))
                        .clicked()
                        && let Some(edit) = self.edit_mut(layer)
                    {
                        edit[slot] = reset_to;
                    }
                },
            );
            ui.add_space(space::S3);
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search keys and actions")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(space::S2);

            let query = self.search.trim().to_lowercase();
            let mut chosen = None;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut any = false;
                    for group in Group::ALL {
                        let defs: Vec<_> = CATALOGUE
                            .iter()
                            .filter(|d| d.group == group)
                            .filter(|d| query.is_empty() || d.name.to_lowercase().contains(&query))
                            .collect();
                        if defs.is_empty() {
                            continue;
                        }
                        any = true;
                        graphite::group_header(ui, group.title());
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(space::S2);
                            for def in defs {
                                let option = egui::Button::new(graphite::text(def.name, Type::Body))
                                    .selected(current == def.action)
                                    .min_size(Vec2::new(size::CONTROL_SM, size::CONTROL_SM));
                                if ui.add(option).clicked() {
                                    chosen = Some(def.action);
                                }
                            }
                        });
                    }
                    if !any {
                        ui.add_space(space::S4);
                        ui.label(
                            graphite::text("Nothing matches that search.", Type::Body)
                                .color(p.text_secondary),
                        );
                    }
                });
            if let Some(action) = chosen
                && let Some(edit) = self.edit_mut(layer)
            {
                edit[slot] = action;
            }
        });
    }
}

/// What to print on a keycap, and in which colour.
fn key_caption(
    layer: Layer,
    key: &PhysKey,
    action: KeyAction,
    p: graphite::Palette,
) -> (String, egui::Color32) {
    match layer {
        Layer::Default if action == key.default => (key.label.to_owned(), p.text),
        Layer::Default => (keymap::describe(action), p.accent),
        Layer::Fn if action == KeyAction::default() => (key.label.to_owned(), p.text_disabled),
        Layer::Fn => (keymap::describe(action), p.accent),
    }
}

fn describe_in(layer: Layer, action: KeyAction) -> String {
    if layer == Layer::Fn && action == KeyAction::default() {
        "Built-in Fn action".to_owned()
    } else {
        keymap::describe(action)
    }
}
