//! The Macros page: a list of the 10 macro slots and an editor for the
//! selected one. All slots are saved together (the keyboard's macro memory
//! only erases when the full set is written).
//!
//! Steps can be added by hand or recorded: while recording, keys typed in
//! this window are appended to the macro's steps.

use std::sync::mpsc::Sender;

use egui::{Align, Layout, Ui};
use lucide_icons::Icon;

use fizz::keymap::{self, CATALOGUE, LAYOUT, kind};
use fizz::protocol::{KeyAction, MACRO_MAX_STEPS, MACRO_SLOTS, Macro, MacroStep, macro_play};
use fizz::recorder::{DEFAULT_DELAY_MS, MAX_DELAY_MS, Recorder, Timing, is_balanced};

use crate::capture;
use crate::graphite::{self, Tone, Type, size, space};
use crate::worker::{Request, Snapshot};

const MOUSE_BUTTONS: [(u8, &str); 5] = [
    (0x01, "Left click"),
    (0x02, "Right click"),
    (0x04, "Middle click"),
    (0x08, "Mouse forward"),
    (0x10, "Mouse back"),
];

fn step_target(step: &MacroStep) -> String {
    if step.mouse {
        MOUSE_BUTTONS
            .iter()
            .find(|(bit, _)| *bit == step.code)
            .map(|(_, name)| (*name).to_owned())
            .unwrap_or_else(|| format!("Mouse button {:02X}", step.code))
    } else {
        keymap::lookup(KeyAction::key(step.code))
            .map(|d| d.name.to_owned())
            .unwrap_or_else(|| format!("Key {:02X}", step.code))
    }
}

/// A recording in progress.
struct Recording {
    slot: usize,
    rec: Recorder,
    /// The window has had keyboard focus since recording began. Losing it
    /// afterwards ends the recording, because key releases would be missed.
    had_focus: bool,
}

#[derive(Default)]
pub struct MacrosPage {
    stored: Option<Vec<Option<Macro>>>,
    edit: Option<Vec<Option<Macro>>>,
    /// Key slots using each macro slot, from both key-map layers.
    users: Vec<Vec<String>>,
    selected: usize,
    saving: bool,
    new_key: u8,
    new_mouse: u8,
    recording: Option<Recording>,
    /// Record with the same delay after every step instead of real pauses.
    fixed_timing: bool,
    fixed_ms: u16,
    /// Developer override (`FIZZ_RECORD`) already started its recording.
    dev_record_started: bool,
}

impl MacrosPage {
    pub fn on_snapshot(&mut self, snapshot: &Snapshot) {
        let unchanged = self.edit == self.stored;
        self.stored = snapshot.macros.clone();
        if unchanged || self.edit.is_none() {
            self.edit = snapshot.macros.clone();
            // Developer override for screenshots: an unsaved sample in slot 1.
            if std::env::var("FIZZ_MACRO_DEMO").is_ok()
                && let Some(edit) = self.edit.as_mut()
                && edit[0].is_none()
            {
                let step = |press, code, delay_ms| MacroStep { press, mouse: false, code, delay_ms };
                edit[0] = Some(Macro {
                    play: macro_play::TIMES,
                    count: 3,
                    steps: vec![
                        step(true, 0x0B, 30),
                        step(false, 0x0B, 30),
                        step(true, 0x0C, 30),
                        step(false, 0x0C, 200),
                    ],
                });
            }
        }
        self.users = vec![Vec::new(); MACRO_SLOTS];
        for (prefix, layer) in [("", &snapshot.keymap_default), ("Fn + ", &snapshot.keymap_fn)] {
            let Some(keys) = layer else { continue };
            for phys in LAYOUT {
                let action = keys[phys.slot];
                if action.kind == kind::MACRO && (action.arg1 as usize) < MACRO_SLOTS {
                    self.users[action.arg1 as usize].push(format!("{prefix}{}", phys.label));
                }
            }
        }
        if self.new_key == 0 {
            self.new_key = 0x04;
            self.new_mouse = 0x01;
            self.fixed_ms = DEFAULT_DELAY_MS;
        }
        self.dev_start_recording();
    }

    /// Developer override for testing: `FIZZ_RECORD=1` creates an empty
    /// macro in slot 1 if needed and starts recording into it.
    fn dev_start_recording(&mut self) {
        if self.dev_record_started || std::env::var("FIZZ_RECORD").is_err() {
            return;
        }
        let timing = self.timing();
        let Some(edit) = self.edit.as_mut() else { return };
        self.dev_record_started = true;
        self.selected = 0;
        let m = edit[0].get_or_insert_with(|| Macro {
            play: macro_play::ONCE,
            count: 1,
            steps: Vec::new(),
        });
        self.recording = Some(Recording {
            slot: 0,
            rec: Recorder::new(m.steps.len(), timing),
            had_focus: false,
        });
    }

    pub fn on_saved(&mut self, stored: Vec<Option<Macro>>) {
        self.stored = Some(stored.clone());
        self.edit = Some(stored);
        self.saving = false;
    }

    pub fn on_error(&mut self) {
        self.saving = false;
    }

    pub fn reset(&mut self) {
        *self = Self {
            selected: self.selected,
            ..Self::default()
        };
    }

    /// Which macro slots hold a macro, for the key mapping picker.
    pub fn defined(&self) -> Vec<bool> {
        self.stored
            .as_ref()
            .map(|m| m.iter().map(Option::is_some).collect())
            .unwrap_or_default()
    }

    fn timing(&self) -> Timing {
        if self.fixed_timing {
            Timing::Fixed(self.fixed_ms)
        } else {
            Timing::Recorded
        }
    }

    /// End a recording from outside the page, for example when another page
    /// is opened. Keys still held are released at the last recorded event.
    pub fn stop_recording(&mut self) {
        if let Some(mut edit) = self.edit.take() {
            self.stop_in(&mut edit, None);
            self.edit = Some(edit);
        } else {
            self.recording = None;
        }
    }

    fn stop_in(&mut self, macros: &mut [Option<Macro>], at_ms: Option<f64>) {
        if let Some(mut r) = self.recording.take()
            && let Some(Some(m)) = macros.get_mut(r.slot)
        {
            r.rec.finish(&mut m.steps, at_ms);
        }
    }

    /// Feed this frame's keyboard events to the recorder.
    fn capture(&mut self, ui: &Ui, macros: &mut [Option<Macro>], now_ms: f64) {
        let Some(r) = self.recording.as_mut() else { return };
        let (events, focused) = ui.input(|i| (i.events.clone(), i.focused));
        r.had_focus |= focused;
        let lost_focus = r.had_focus && !focused;
        if let Some(Some(m)) = macros.get_mut(r.slot) {
            for (code, pressed) in capture::key_events(&events) {
                if pressed {
                    r.rec.key_down(&mut m.steps, code, now_ms);
                } else {
                    r.rec.key_up(&mut m.steps, code, now_ms);
                }
            }
        }
        if r.rec.is_full() || lost_focus {
            self.stop_in(macros, Some(now_ms));
        }
    }

    /// Keep keyboard focus on an invisible recorder widget while recording,
    /// and tell egui it wants Tab, the arrows and Esc, so those keys are
    /// recorded instead of moving focus around the window.
    fn keep_focus(&self, ui: &mut Ui) {
        if self.recording.is_none() {
            return;
        }
        let id = ui.make_persistent_id("macro-recorder");
        let rect = egui::Rect::from_min_size(ui.cursor().min, egui::Vec2::splat(size::BORDER));
        ui.interact(rect, id, egui::Sense::focusable_noninteractive());
        ui.memory_mut(|m| {
            m.request_focus(id);
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
    }

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        let Some(mut macros) = self.edit.clone() else {
            graphite::empty_state(
                ui,
                Some(Icon::CircleAlert.unicode()),
                "Couldn't read the macros",
                "The keyboard didn't send all 10 macro slots. Read it again to retry.",
                Some("Read again"),
            )
            .then(|| tx.send(Request::Refresh));
            return;
        };
        let now_ms = ui.input(|i| i.time) * 1000.0;
        self.capture(ui, &mut macros, now_ms);
        self.keep_focus(ui);

        let p = graphite::palette(ui);
        let recording = self.recording.is_some();
        let changed = self.stored.as_ref() != Some(&macros);
        let invalid = macros.iter().flatten().any(|m| !is_balanced(&m.steps));

        // Action bar.
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.saving {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(graphite::text("Saving", Type::Body).color(p.text_secondary));
                    return;
                }
                let save = ui.add_enabled_ui(!invalid && !recording, |ui| {
                    graphite::primary_button(ui, "Save macros")
                });
                if save.inner.clicked() && changed {
                    self.saving = true;
                    let _ = tx.send(Request::SaveMacros(macros.clone()));
                }
                if ui
                    .add_enabled(changed, egui::Button::new("Discard changes"))
                    .clicked()
                {
                    self.recording = None;
                    self.edit = self.stored.clone();
                    macros = self.edit.clone().unwrap_or_default();
                }
                if changed {
                    graphite::badge(ui, "Unsaved changes", Tone::Accent);
                }
            });
        });
        ui.add_space(space::S4);

        ui.horizontal_top(|ui| {
            // Slot list.
            let mut picked = None;
            ui.vertical(|ui| {
                ui.set_width(size::INSPECTOR);
                graphite::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (slot, m) in macros.iter().enumerate() {
                        let label = match m {
                            Some(m) => format!("Macro {}  ·  {} steps", slot + 1, m.steps.len()),
                            None => format!("Macro {}  ·  empty", slot + 1),
                        };
                        let glyph = if m.is_some() { Icon::ListVideo } else { Icon::Plus };
                        if graphite::nav_row(ui, Some(glyph.unicode()), &label, self.selected == slot)
                            .clicked()
                        {
                            picked = Some(slot);
                        }
                    }
                });
            });
            if let Some(slot) = picked {
                // A recording belongs to one macro: finish it before moving on.
                self.stop_in(&mut macros, Some(now_ms));
                self.selected = slot;
            }
            ui.add_space(space::S5);
            // Editor.
            ui.vertical(|ui| {
                ui.add_enabled_ui(!self.saving, |ui| self.editor(ui, &mut macros, now_ms));
            });
        });
        self.edit = Some(macros);
    }

    fn editor(&mut self, ui: &mut Ui, macros: &mut [Option<Macro>], now_ms: f64) {
        let p = graphite::palette(ui);
        let slot = self.selected.min(MACRO_SLOTS - 1);
        let users = self.users.get(slot).cloned().unwrap_or_default();
        let recording_here = self.recording.as_ref().is_some_and(|r| r.slot == slot);
        let timing = self.timing();

        let Some(m) = macros[slot].as_mut() else {
            graphite::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(space::S6);
                if graphite::empty_state(
                    ui,
                    Some(Icon::ListVideo.unicode()),
                    &format!("Macro {} is empty", slot + 1),
                    "Create a macro, then add the keys it should press or record them.",
                    Some("Create macro"),
                ) {
                    macros[slot] = Some(Macro {
                        play: macro_play::ONCE,
                        count: 1,
                        steps: Vec::new(),
                    });
                }
                ui.add_space(space::S6);
            });
            return;
        };

        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(graphite::text(format!("Macro {}", slot + 1), Type::Heading));
            let used = if users.is_empty() {
                "Not assigned to a key yet. Assign it on the Key mapping page.".to_owned()
            } else {
                format!("Assigned to {}.", users.join(", "))
            };
            ui.label(graphite::text(used, Type::Small).color(p.text_secondary));
            ui.add_space(space::S3);

            graphite::setting_row(ui, "When the key is pressed", None, |ui| {
                egui::ComboBox::from_id_salt(("macro-play", slot))
                    .selected_text(macro_play::name(m.play))
                    .show_ui(ui, |ui| {
                        for play in macro_play::ALL {
                            ui.selectable_value(&mut m.play, play, macro_play::name(play));
                        }
                    });
            });
            if m.play == macro_play::TIMES {
                graphite::divider(ui);
                graphite::setting_row(ui, "Number of times", None, |ui| {
                    ui.add(egui::DragValue::new(&mut m.count).range(1..=99));
                });
            }
        });

        ui.add_space(space::S5);
        ui.horizontal(|ui| {
            ui.label(graphite::text("Steps", Type::Heading));
            graphite::badge(ui, &format!("{} of {MACRO_MAX_STEPS}", m.steps.len()), Tone::Neutral);
        });
        ui.add_space(space::S2);

        if let Some(r) = self.recording.as_ref().filter(|_| recording_here) {
            let n = r.rec.recorded(&m.steps);
            let noun = if n == 1 { "step" } else { "steps" };
            graphite::banner(
                ui,
                Tone::Accent,
                Icon::Circle.unicode(),
                &format!("Recording. {n} {noun} added. Press more keys, or choose Stop recording."),
            );
            ui.add_space(space::S3);
        }

        let can_record = m.steps.len() + 2 <= MACRO_MAX_STEPS;
        let mut start = false;
        let mut stop = false;
        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());

            // Recording controls.
            graphite::setting_row(
                ui,
                "Record keys",
                Some("Press keys in this window to add them to the end of the steps. Some keys, such as the Windows key, can't be recorded here; use Add key press for those."),
                |ui| {
                    if recording_here {
                        stop = ui.button("Stop recording").clicked();
                    } else {
                        start = ui
                            .add_enabled(can_record, egui::Button::new("Record keys"))
                            .clicked();
                    }
                },
            );
            graphite::divider(ui);
            graphite::setting_row(
                ui,
                "Delays while recording",
                Some("Recorded keeps the pauses you make. Fixed waits the same time after every step."),
                |ui| {
                    ui.add_enabled_ui(!recording_here, |ui| {
                        if self.fixed_timing {
                            ui.add(
                                egui::DragValue::new(&mut self.fixed_ms)
                                    .range(0..=MAX_DELAY_MS)
                                    .suffix(" ms"),
                            );
                        }
                        graphite::segmented(
                            ui,
                            &mut self.fixed_timing,
                            &[(false, "Recorded"), (true, "Fixed")],
                        );
                    });
                },
            );
            graphite::divider(ui);

            // Everything below is locked while recording.
            ui.add_enabled_ui(!recording_here, |ui| {
                let mut remove = None;
                let mut swap = None;
                let count = m.steps.len();
                for (i, step) in m.steps.iter_mut().enumerate() {
                    if i > 0 {
                        graphite::divider(ui);
                    }
                    let action = if step.press { "Press" } else { "Release" };
                    let title = format!("{}.  {action} {}", i + 1, step_target(step));
                    graphite::setting_row(ui, &title, None, |ui| {
                        if graphite::icon_button(ui, Icon::Trash2.unicode(), "Remove step").clicked() {
                            remove = Some(i);
                        }
                        if i + 1 < count
                            && graphite::icon_button(ui, Icon::ChevronDown.unicode(), "Move down")
                                .clicked()
                        {
                            swap = Some(i);
                        }
                        if i > 0
                            && graphite::icon_button(ui, Icon::ChevronUp.unicode(), "Move up").clicked()
                        {
                            swap = Some(i - 1);
                        }
                        ui.add(
                            egui::DragValue::new(&mut step.delay_ms)
                                .range(0..=MAX_DELAY_MS)
                                .suffix(" ms"),
                        );
                        ui.label(graphite::text("then wait", Type::Small).color(p.text_muted));
                    });
                }
                if let Some(i) = remove {
                    m.steps.remove(i);
                }
                if let Some(i) = swap {
                    m.steps.swap(i, i + 1);
                }
                if m.steps.is_empty() {
                    ui.label(
                        graphite::text(
                            "No steps yet. Record keys, or add a key press or a click below.",
                            Type::Body,
                        )
                        .color(p.text_secondary),
                    );
                }

                // Add steps: a press and a release together.
                graphite::divider(ui);
                ui.add_space(space::S3);
                let room = m.steps.len() + 2 <= MACRO_MAX_STEPS;
                let pair = |code: u8, mouse: bool| {
                    [true, false].map(|press| MacroStep {
                        press,
                        mouse,
                        code,
                        delay_ms: DEFAULT_DELAY_MS,
                    })
                };
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("macro-new-key")
                        .selected_text(
                            keymap::lookup(KeyAction::key(self.new_key)).map_or("Key", |d| d.name),
                        )
                        .show_ui(ui, |ui| {
                            for def in CATALOGUE.iter().filter(|d| d.action.kind == kind::KEY) {
                                ui.selectable_value(&mut self.new_key, def.action.code, def.name);
                            }
                        });
                    if ui.add_enabled(room, egui::Button::new("Add key press")).clicked() {
                        m.steps.extend(pair(self.new_key, false));
                    }
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("macro-new-mouse")
                        .selected_text(
                            MOUSE_BUTTONS
                                .iter()
                                .find(|(b, _)| *b == self.new_mouse)
                                .map_or("Mouse", |(_, n)| *n),
                        )
                        .show_ui(ui, |ui| {
                            for (bit, name) in MOUSE_BUTTONS {
                                ui.selectable_value(&mut self.new_mouse, bit, name);
                            }
                        });
                    if ui.add_enabled(room, egui::Button::new("Add click")).clicked() {
                        m.steps.extend(pair(self.new_mouse, true));
                    }
                });
                if !room {
                    graphite::field_hint(ui, "A macro can have up to 40 steps (20 key presses).", false);
                }
                if !is_balanced(&m.steps) {
                    graphite::field_hint(
                        ui,
                        "Every press needs a matching release after it. Fix the steps before saving.",
                        true,
                    );
                }
            });
        });

        if start {
            self.recording = Some(Recording {
                slot,
                rec: Recorder::new(m.steps.len(), timing),
                had_focus: ui.input(|i| i.focused),
            });
        }
        if stop && let Some(mut r) = self.recording.take() {
            r.rec.finish(&mut m.steps, Some(now_ms));
        }

        ui.add_space(space::S5);
        let mut clear_macro = false;
        ui.horizontal(|ui| {
            let bound = !users.is_empty();
            let busy = recording_here || stop;
            if ui
                .add_enabled(!busy && !m.steps.is_empty(), egui::Button::new("Clear steps"))
                .clicked()
            {
                m.steps.clear();
            }
            clear_macro = ui
                .add_enabled(!busy && !bound, egui::Button::new("Clear macro"))
                .clicked();
            if bound {
                ui.label(
                    graphite::text(
                        "Unassign it from its keys on the Key mapping page before clearing it.",
                        Type::Small,
                    )
                    .color(p.text_muted),
                );
            }
        });
        if clear_macro {
            macros[slot] = None;
        }
    }
}
