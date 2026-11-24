//! The Snap tap (SOCD) page: turn SOCD on or off, and edit the key pairs.

use std::sync::mpsc::Sender;

use egui::{Align, Layout, Ui};
use lucide_icons::Icon;

use fizz::keymap::{self, CATALOGUE, kind};
use fizz::protocol::{KeyAction, SOCD_SLOTS, SocdPair, SocdPower, socd_mode};

use crate::graphite::{self, Tone, Type, size, space};
use crate::worker::{Request, Snapshot};

/// Name of a key from its HID usage code.
fn key_name(code: u8) -> String {
    keymap::lookup(KeyAction::key(code))
        .map(|d| d.name.to_owned())
        .unwrap_or_else(|| format!("Key {code:02X}"))
}

/// The keyboard reports two codes with the on/off state; on this keyboard
/// `FA E5`, read as the Fn + Right Shift toggle.
fn shortcut_text(power: &SocdPower) -> Option<String> {
    let first = match power.code1 {
        0xFA => "Fn".to_owned(),
        0 => return None,
        c => key_name(c),
    };
    let second = match power.code2 {
        0 => return None,
        c => key_name(c),
    };
    Some(format!("{first} + {second}"))
}

fn mode_picker(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, mode: &mut u8) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(socd_mode::name(*mode))
        .show_ui(ui, |ui| {
            for m in socd_mode::ALL {
                ui.selectable_value(mode, m, socd_mode::name(m));
            }
        });
}

fn key_picker(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, code: &mut u8) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(key_name(*code))
        .show_ui(ui, |ui| {
            for def in CATALOGUE.iter().filter(|d| d.action.kind == kind::KEY) {
                ui.selectable_value(code, def.action.code, def.name);
            }
        });
}

#[derive(Default)]
pub struct SocdPage {
    stored: Option<Vec<Option<SocdPair>>>,
    edit: Option<Vec<Option<SocdPair>>>,
    power: Option<SocdPower>,
    setting_power: Option<bool>,
    saving: bool,
    new_key1: u8,
    new_key2: u8,
    new_mode: u8,
}

impl SocdPage {
    pub fn on_snapshot(&mut self, snapshot: &Snapshot) {
        let unchanged = self.edit == self.stored;
        self.stored = snapshot.socd_pairs.clone();
        if unchanged || self.edit.is_none() {
            self.edit = snapshot.socd_pairs.clone();
        }
        self.power = snapshot.socd_power;
        if self.new_key1 == 0 {
            self.suggest_pair();
        }
    }

    /// Pre-fill the "add a pair" row with the first common pair not in use.
    fn suggest_pair(&mut self) {
        // A+D, W+S, Left+Right, Up+Down (HID usage codes).
        const COMMON: [(u8, u8); 4] = [(0x04, 0x07), (0x1A, 0x16), (0x50, 0x4F), (0x52, 0x51)];
        let used = |a: u8, b: u8| {
            self.edit.iter().flatten().flatten().any(|q| {
                (q.key1, q.key2) == (a, b) || (q.key1, q.key2) == (b, a)
            })
        };
        let (k1, k2) = COMMON
            .into_iter()
            .find(|&(a, b)| !used(a, b))
            .unwrap_or(COMMON[0]);
        self.new_key1 = k1;
        self.new_key2 = k2;
        self.new_mode = socd_mode::LAST_INPUT;
    }

    pub fn on_pairs_saved(&mut self, stored: Vec<Option<SocdPair>>) {
        self.stored = Some(stored.clone());
        self.edit = Some(stored);
        self.saving = false;
    }

    pub fn on_power_set(&mut self, power: Option<SocdPower>) {
        if power.is_some() {
            self.power = power;
        }
        self.setting_power = None;
    }

    pub fn on_error(&mut self) {
        self.saving = false;
        self.setting_power = None;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let (Some(power), Some(mut pairs)) = (self.power, self.edit.clone()) else {
            graphite::empty_state(
                ui,
                Some(Icon::CircleAlert.unicode()),
                "Couldn't read the SOCD settings",
                "The keyboard didn't answer the SOCD requests. Read it again to retry.",
                Some("Read again"),
            )
            .then(|| tx.send(Request::Refresh));
            return;
        };
        let p = graphite::palette(ui);
        let changed = self.edit != self.stored;

        graphite::banner(
            ui,
            Tone::Warning,
            Icon::TriangleAlert.unicode(),
            "Some games, such as CS2, ban SOCD. Check a game's rules before using it in competitive play.",
        );
        ui.add_space(space::S5);

        // --- on / off ------------------------------------------------------
        graphite::card(ui, |ui| {
            graphite::setting_row(
                ui,
                "Snap tap (SOCD)",
                Some("When both keys of a pair are held, decide which one counts, using the pairs below. Applies immediately."),
                |ui| {
                    let mut on = self.setting_power.unwrap_or(power.on);
                    let pending = self.setting_power.is_some();
                    if ui
                        .add_enabled(!pending, egui::Checkbox::without_text(&mut on))
                        .changed()
                    {
                        self.setting_power = Some(on);
                        let _ = tx.send(Request::SetSocdPower(on));
                    }
                    if pending {
                        ui.add(egui::Spinner::new().size(size::ICON));
                    }
                },
            );
            if let Some(shortcut) = shortcut_text(&power) {
                graphite::divider(ui);
                graphite::setting_row(
                    ui,
                    "Keyboard shortcut",
                    Some("The keys the keyboard reports for turning SOCD on and off."),
                    |ui| {
                        ui.label(graphite::value_text(ui, shortcut, false));
                    },
                );
            }
        });

        // --- pairs -----------------------------------------------------------
        ui.add_space(space::S6);
        let used = pairs.iter().flatten().count();
        ui.horizontal(|ui| {
            ui.label(graphite::text("Key pairs", Type::Heading));
            graphite::badge(ui, &format!("{used} of {SOCD_SLOTS}"), Tone::Neutral);
            if changed {
                graphite::badge(ui, "Unsaved changes", Tone::Accent);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.saving {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(graphite::text("Saving", Type::Body).color(p.text_secondary));
                    return;
                }
                if graphite::primary_button(ui, "Save pairs").clicked() && changed {
                    self.saving = true;
                    let _ = tx.send(Request::SaveSocdPairs(pairs.clone()));
                }
                if ui
                    .add_enabled(changed, egui::Button::new("Discard changes"))
                    .clicked()
                {
                    self.edit = self.stored.clone();
                    pairs = self.edit.clone().unwrap_or_default();
                }
            });
        });
        ui.add_space(space::S2);

        let mut added = false;
        ui.add_enabled_ui(!self.saving, |ui| {
            graphite::card(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut remove = None;
                let mut first = true;
                for (slot, entry) in pairs.iter_mut().enumerate() {
                    let Some(pair) = entry else { continue };
                    if !first {
                        graphite::divider(ui);
                    }
                    first = false;
                    let title = format!("{} + {}", key_name(pair.key1), key_name(pair.key2));
                    graphite::setting_row(ui, &title, None, |ui| {
                        if graphite::icon_button(ui, Icon::Trash2.unicode(), "Remove pair").clicked() {
                            remove = Some(slot);
                        }
                        mode_picker(ui, ("socd-mode", slot), &mut pair.mode);
                    });
                }
                if let Some(slot) = remove {
                    pairs[slot] = None;
                }
                if used == 0 {
                    ui.label(
                        graphite::text("No pairs yet. Add one below.", Type::Body)
                            .color(p.text_secondary),
                    );
                }

                // Add a pair.
                graphite::divider(ui);
                ui.add_space(space::S3);
                ui.label(graphite::text("Add a pair", Type::Label));
                ui.add_space(space::S2);
                let duplicate = pairs.iter().flatten().any(|q| {
                    (q.key1, q.key2) == (self.new_key1, self.new_key2)
                        || (q.key1, q.key2) == (self.new_key2, self.new_key1)
                });
                let free = pairs.iter().position(Option::is_none);
                let problem = if self.new_key1 == self.new_key2 {
                    Some("Choose two different keys.")
                } else if duplicate {
                    Some("These two keys are already a pair.")
                } else if free.is_none() {
                    Some("All 20 pairs are in use. Remove one first.")
                } else {
                    None
                };
                ui.horizontal(|ui| {
                    key_picker(ui, "socd-new-key1", &mut self.new_key1);
                    ui.label(graphite::text("+", Type::Label).color(p.text_muted));
                    key_picker(ui, "socd-new-key2", &mut self.new_key2);
                    mode_picker(ui, "socd-new-mode", &mut self.new_mode);
                    if ui
                        .add_enabled(problem.is_none(), egui::Button::new("Add pair"))
                        .clicked()
                        && let Some(slot) = free
                    {
                        pairs[slot] = Some(SocdPair {
                            state: 1,
                            mode: self.new_mode,
                            key1: self.new_key1,
                            key2: self.new_key2,
                        });
                        added = true;
                    }
                });
                if let Some(problem) = problem {
                    graphite::field_hint(ui, problem, false);
                }
            });
        });
        self.edit = Some(pairs);
        if added {
            self.suggest_pair();
        }
    }
}
