//! The Actuation page: where each key actuates, and rapid trigger.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;

use egui::{Align, Layout, Ui};
use lucide_icons::Icon;

use fizz::keymap::LAYOUT;
use fizz::protocol::{KEY_SLOTS, KeyTrigger, TriggerConfig, trigger_mode};

use crate::graphite::{self, Tone, Type, size, space};
use crate::keyboard_view::{KeyFace, keyboard_view};
use crate::worker::{Request, Snapshot};

/// The editable view of one key's settings. Rapid trigger with one
/// sensitivity stores the same value for press and release.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fields {
    travel: u16,
    rapid: bool,
    split: bool,
    press: u16,
    release: u16,
}

impl Fields {
    fn from(t: KeyTrigger) -> Self {
        Self {
            travel: t.travel,
            rapid: trigger_mode::is_rapid(t.mode),
            split: t.mode == trigger_mode::RT_SPLIT,
            press: t.press,
            release: t.release,
        }
    }

    fn to_trigger(self) -> KeyTrigger {
        let mode = match (self.rapid, self.split) {
            (false, _) => trigger_mode::NORMAL,
            (true, false) => trigger_mode::RT_SINGLE,
            (true, true) => trigger_mode::RT_SPLIT,
        };
        let release = if self.rapid && !self.split { self.press } else { self.release };
        KeyTrigger {
            mode,
            travel: self.travel,
            press: self.press,
            release,
        }
    }
}

#[derive(Default)]
pub struct ActuationPage {
    config: Option<TriggerConfig>,
    stored: Option<Vec<Option<KeyTrigger>>>,
    edit: Option<Vec<Option<KeyTrigger>>>,
    selected: BTreeSet<usize>,
    saving: bool,
}

impl ActuationPage {
    fn changed_slots(&self) -> Vec<usize> {
        match (&self.stored, &self.edit) {
            (Some(stored), Some(edit)) => (0..KEY_SLOTS).filter(|&s| stored[s] != edit[s]).collect(),
            _ => Vec::new(),
        }
    }

    pub fn on_snapshot(&mut self, snapshot: &Snapshot) {
        let unchanged = self.changed_slots().is_empty();
        self.config = snapshot.trigger_config;
        self.stored = snapshot.triggers.clone();
        if unchanged || self.edit.is_none() {
            self.edit = snapshot.triggers.clone();
        }
    }

    pub fn on_saved(&mut self, stored: Vec<(usize, Option<KeyTrigger>)>) {
        for (slot, t) in stored {
            if let Some(s) = self.stored.as_mut() {
                s[slot] = t;
            }
            if let Some(e) = self.edit.as_mut() {
                e[slot] = t;
            }
        }
        self.saving = false;
    }

    pub fn on_error(&mut self) {
        self.saving = false;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn select(&mut self, slot: usize) {
        self.selected = BTreeSet::from([slot]);
    }

    // -----------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let (Some(config), Some(keys)) = (self.config, self.edit.clone()) else {
            graphite::empty_state(
                ui,
                Some(Icon::CircleAlert.unicode()),
                "Couldn't read actuation settings",
                "The keyboard didn't answer the actuation requests. Read it again to retry.",
                Some("Read again"),
            )
            .then(|| tx.send(Request::Refresh));
            return;
        };

        self.action_bar(ui, tx);
        ui.add_space(space::S4);

        let changed: BTreeSet<usize> = self.changed_slots().into_iter().collect();
        let p = graphite::palette(ui);
        let clicked = keyboard_view(
            ui,
            "actuation",
            0.55,
            |slot| self.selected.contains(&slot),
            |key| {
                let Some(t) = keys[key.slot] else {
                    return KeyFace {
                        top: key.label.to_owned(),
                        main: "?".to_owned(),
                        main_color: p.text_disabled,
                        unsaved: false,
                        tooltip: format!("{}: no reply from the keyboard", key.label),
                        swatch: None,
                    };
                };
                let f = Fields::from(t);
                let main = format!("{:.2}", config.mm(f.travel));
                let tooltip = if f.rapid {
                    format!(
                        "{}: actuates at {:.2} mm, rapid trigger {}",
                        key.label,
                        config.mm(f.travel),
                        sensitivity_text(&config, f)
                    )
                } else {
                    format!("{}: actuates at {:.2} mm", key.label, config.mm(f.travel))
                };
                KeyFace {
                    top: key.label.to_owned(),
                    main,
                    main_color: if f.rapid { p.accent } else { p.text },
                    unsaved: changed.contains(&key.slot),
                    tooltip,
                    swatch: None,
                }
            },
        );
        if let Some(click) = clicked {
            if click.extend {
                if !self.selected.remove(&click.slot) {
                    self.selected.insert(click.slot);
                }
            } else {
                self.select(click.slot);
            }
        }
        ui.add_space(space::S2);
        ui.label(
            graphite::text(
                "Numbers are the actuation point in mm. Accent numbers have rapid trigger on. Ctrl-click to choose several keys.",
                Type::Small,
            )
            .color(p.text_muted),
        );
        ui.add_space(space::S4);
        egui::ScrollArea::vertical()
            .id_salt("actuation-inspector")
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.saving, |ui| self.inspector(ui, &config));
            });
    }

    fn action_bar(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let p = graphite::palette(ui);
        ui.horizontal(|ui| {
            if graphite::ghost_button(ui, "Select all").clicked() {
                self.selected = LAYOUT.iter().map(|k| k.slot).collect();
            }
            if graphite::ghost_button(ui, "Select none").clicked() {
                self.selected.clear();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.saving {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(graphite::text("Saving", Type::Body).color(p.text_secondary));
                    return;
                }
                let changed = self.changed_slots();
                if graphite::primary_button(ui, "Save actuation").clicked() && !changed.is_empty() {
                    let groups = self.groups(&changed);
                    if !groups.is_empty() {
                        self.saving = true;
                        let _ = tx.send(Request::SaveTriggers(groups));
                    }
                }
                if ui
                    .add_enabled(!changed.is_empty(), egui::Button::new("Discard changes"))
                    .clicked()
                {
                    self.edit = self.stored.clone();
                }
                let note = match changed.len() {
                    0 => "No unsaved changes".to_owned(),
                    1 => "1 key changed".to_owned(),
                    n => format!("{n} keys changed"),
                };
                ui.label(graphite::text(note, Type::Small).color(p.text_muted));
            });
        });
    }

    /// Changed keys grouped by identical settings: one packet per group.
    fn groups(&self, changed: &[usize]) -> Vec<(KeyTrigger, Vec<usize>)> {
        let Some(edit) = &self.edit else {
            return Vec::new();
        };
        let mut groups: Vec<(KeyTrigger, Vec<usize>)> = Vec::new();
        for &slot in changed {
            let Some(t) = edit[slot] else { continue };
            match groups.iter_mut().find(|(g, _)| *g == t) {
                Some((_, slots)) => slots.push(slot),
                None => groups.push((t, vec![slot])),
            }
        }
        groups
    }

    fn inspector(&mut self, ui: &mut Ui, config: &TriggerConfig) {
        let p = graphite::palette(ui);
        let Some(edit) = self.edit.as_mut() else { return };
        let selected: Vec<usize> = self
            .selected
            .iter()
            .copied()
            .filter(|s| edit[*s].is_some())
            .collect();
        let Some(&first) = selected.first() else {
            graphite::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(space::S5);
                graphite::empty_state(
                    ui,
                    Some(Icon::Keyboard.unicode()),
                    "Choose keys",
                    "Click a key to change where it actuates, or select all to change every key.",
                    None,
                );
                ui.add_space(space::S5);
            });
            return;
        };

        let before = Fields::from(edit[first].unwrap_or_default());
        let mixed = selected
            .iter()
            .any(|s| edit[*s].map(Fields::from) != Some(before));
        let mut f = before;

        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let title = if selected.len() == 1 {
                    let label = LAYOUT.iter().find(|k| k.slot == first).map_or("", |k| k.label);
                    format!("{label} key")
                } else {
                    format!("{} keys selected", selected.len())
                };
                ui.label(graphite::text(title, Type::Heading));
                if mixed {
                    graphite::badge(ui, "Mixed settings, showing the first key", Tone::Neutral);
                }
            });
            ui.add_space(space::S2);

            let mm_slider = |ui: &mut Ui, steps: &mut u16, lo: u16, hi: u16| {
                let mut mm = config.mm(*steps);
                let response = ui.add(
                    egui::Slider::new(&mut mm, config.mm(lo)..=config.mm(hi))
                        .step_by(config.unit_mm as f64)
                        .fixed_decimals(2)
                        .suffix(" mm"),
                );
                if response.changed() {
                    *steps = config.steps(mm).clamp(lo, hi);
                }
            };

            graphite::setting_row(
                ui,
                "Actuation point",
                Some("How far the key travels before it registers. Lower is more sensitive."),
                |ui| mm_slider(ui, &mut f.travel, config.min_steps, config.max_steps),
            );
            graphite::divider(ui);
            graphite::setting_row(
                ui,
                "Rapid trigger",
                Some("Re-press and release by movement direction instead of a fixed point."),
                |ui| {
                    ui.add(egui::Checkbox::without_text(&mut f.rapid));
                },
            );
            let rt_lo = 1;
            let rt_hi = config.rt_max_steps();
            if f.rapid {
                graphite::divider(ui);
                graphite::setting_row(
                    ui,
                    "Separate press and release",
                    Some("Set different distances for pressing again and for releasing."),
                    |ui| {
                        ui.add(egui::Checkbox::without_text(&mut f.split));
                    },
                );
                graphite::divider(ui);
                if f.split {
                    graphite::setting_row(
                        ui,
                        "Press sensitivity",
                        Some("How far the key must move down again to press."),
                        |ui| mm_slider(ui, &mut f.press, rt_lo, rt_hi),
                    );
                    graphite::divider(ui);
                    graphite::setting_row(
                        ui,
                        "Release sensitivity",
                        Some("How far the key must rise to release."),
                        |ui| mm_slider(ui, &mut f.release, rt_lo, rt_hi),
                    );
                } else {
                    graphite::setting_row(
                        ui,
                        "Sensitivity",
                        Some("How far the key must move to press again or release."),
                        |ui| mm_slider(ui, &mut f.press, rt_lo, rt_hi),
                    );
                }
            }
            ui.add_space(space::S2);
            ui.label(
                graphite::text(
                    format!(
                        "This keyboard accepts {:.2}–{:.2} mm in {:.2} mm steps.",
                        config.mm(config.min_steps),
                        config.mm(config.max_steps),
                        config.unit_mm
                    ),
                    Type::Small,
                )
                .color(p.text_muted),
            );
        });

        // Apply only the fields that changed, to every selected key.
        if f != before {
            for slot in &selected {
                let Some(current) = edit[*slot] else { continue };
                let mut k = Fields::from(current);
                if f.travel != before.travel {
                    k.travel = f.travel;
                }
                if f.rapid != before.rapid {
                    k.rapid = f.rapid;
                }
                if f.split != before.split {
                    k.split = f.split;
                }
                if f.press != before.press {
                    k.press = f.press;
                }
                if f.release != before.release {
                    k.release = f.release;
                }
                edit[*slot] = Some(k.to_trigger());
            }
        }
    }
}

fn sensitivity_text(config: &TriggerConfig, f: Fields) -> String {
    if f.split {
        format!(
            "press {:.2} mm, release {:.2} mm",
            config.mm(f.press),
            config.mm(f.release)
        )
    } else {
        format!("{:.2} mm", config.mm(f.press))
    }
}
