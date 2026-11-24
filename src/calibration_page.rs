//! The Calibration page: a guided run that teaches the keyboard how far each
//! magnetic switch travels.
//!
//! Calibration can't be read back or backed up, so the page explains what is
//! about to happen and asks before starting. While it runs, the window keeps
//! keyboard focus so that pressing keys doesn't operate the page.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use egui::{Align, Layout, Ui};
use lucide_icons::Icon;

use fizz::keymap::LAYOUT;
use fizz::protocol::CalibrationEvent;

use crate::graphite::{self, Tone, Type, size, space};
use crate::keyboard_view::{KeyFace, capture_keys, keyboard_view};
use crate::worker::Request;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Running,
    /// The keyboard says it is complete; waiting for the user to finish.
    Finished,
    Failed(u8),
}

pub struct CalibrationPage {
    state: State,
    /// Keys the keyboard has reported done in this run.
    done: BTreeSet<usize>,
    /// A start or stop request is on its way.
    pending: bool,
    /// The "Start calibration?" dialog is open.
    confirm: bool,
    log_path: Option<PathBuf>,
    /// Developer override (`FIZZ_CAL_DEMO`): show a simulated run and send nothing.
    demo: bool,
}

impl CalibrationPage {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            done: BTreeSet::new(),
            pending: false,
            confirm: false,
            log_path: None,
            demo: std::env::var("FIZZ_CAL_DEMO").is_ok(),
        }
    }

    /// The keyboard is, or is about to be, in calibration mode.
    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Running | State::Finished)
    }

    fn known_done(&self) -> usize {
        self.done
            .iter()
            .filter(|slot| LAYOUT.iter().any(|k| k.slot == **slot))
            .count()
    }

    fn send(&self, tx: &Sender<Request>, request: Request) {
        if !self.demo {
            let _ = tx.send(request);
        }
    }

    /// Leave calibration because the user went elsewhere.
    pub fn leave(&mut self, tx: &Sender<Request>) {
        if self.is_active() {
            self.send(tx, Request::StopCalibration);
            self.state = State::Idle;
            self.done.clear();
            self.pending = false;
        }
        self.confirm = false;
    }

    pub fn on_started(&mut self, log: Option<PathBuf>) {
        self.state = State::Running;
        self.done.clear();
        self.pending = false;
        self.log_path = log;
    }

    pub fn on_event(&mut self, event: CalibrationEvent) {
        match event {
            CalibrationEvent::Progress(slots) => self.done = slots.into_iter().collect(),
            CalibrationEvent::Finished => self.state = State::Finished,
            CalibrationEvent::Failed(code) => self.state = State::Failed(code),
        }
    }

    /// The keyboard confirmed it left calibration. Returns a message for the notice.
    pub fn on_stopped(&mut self) -> Option<String> {
        self.pending = false;
        let total = LAYOUT.len();
        let n = self.known_done();
        let message = match self.state {
            State::Finished => Some(format!("Calibration finished. The keyboard reported {n} of {total} keys.")),
            State::Running => Some(format!("Calibration stopped. The keyboard had reported {n} of {total} keys.")),
            State::Idle | State::Failed(_) => None,
        };
        // A failure stays on screen until the user starts again.
        if !matches!(self.state, State::Failed(_)) {
            self.state = State::Idle;
            self.done.clear();
        }
        message
    }

    /// A request failed or the keyboard went away.
    pub fn on_error(&mut self) {
        self.pending = false;
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.done.clear();
        self.pending = false;
        self.confirm = false;
    }

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        if self.demo && self.state == State::Idle && self.done.is_empty() {
            // A believable mid-run picture for screenshots.
            self.state = State::Running;
            self.done = LAYOUT.iter().map(|k| k.slot).filter(|s| s % 3 != 0).collect();
        }
        if self.is_active() {
            capture_keys(ui, "calibration");
        }
        let p = graphite::palette(ui);
        let total = LAYOUT.len();

        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(graphite::text("Calibrate the switches", Type::Heading));
            ui.add_space(space::S2);
            ui.label(
                graphite::text(
                    "Calibration teaches the keyboard how far each magnetic switch travels. Do it after changing switches, or if keys seem to actuate too early or too late.",
                    Type::Body,
                )
                .color(p.text_secondary),
            );
            ui.add_space(space::S3);
            for (n, step) in [
                "Choose Start calibration.",
                "Press each key all the way down and hold it there, wiggling it slightly, until it turns green on the drawing below.",
                "Release it, and repeat each key 2 or 3 times. Don't press harder than you need to.",
                "When every key is green, choose Stop calibration.",
            ]
            .iter()
            .enumerate()
            {
                ui.label(
                    graphite::text(format!("{}.  {step}", n + 1), Type::Body).color(p.text_secondary),
                );
            }
            ui.add_space(space::S3);
            ui.label(
                graphite::text(
                    "The keyboard's calibration can't be read or backed up. If a run goes wrong, calibrate again.",
                    Type::Small,
                )
                .color(p.text_muted),
            );
        });
        ui.add_space(space::S4);

        match self.state {
            State::Running => graphite::banner(
                ui,
                Tone::Accent,
                Icon::Circle.unicode(),
                "Calibrating. Press each key all the way down and hold it until it turns green.",
            ),
            State::Finished => graphite::banner(
                ui,
                Tone::Success,
                Icon::CircleCheck.unicode(),
                "The keyboard says calibration is complete. Choose Finish calibration when you're done.",
            ),
            State::Failed(code) => graphite::banner(
                ui,
                Tone::Danger,
                Icon::CircleAlert.unicode(),
                &format!("Calibration failed (status {code}). Choose Start calibration to try again."),
            ),
            State::Idle => {}
        }
        if self.state != State::Idle {
            ui.add_space(space::S3);
        }

        let state = self.state;
        let busy = self.pending;
        let (label, can) = match state {
            State::Running => ("Stop calibration", true),
            State::Finished => ("Finish calibration", true),
            State::Idle | State::Failed(_) => ("Start calibration", true),
        };
        ui.horizontal(|ui| {
            let (tone, text) = match state {
                State::Idle => (Tone::Neutral, "Not started"),
                State::Running => (Tone::Accent, "Calibrating"),
                State::Finished => (Tone::Success, "Complete"),
                State::Failed(_) => (Tone::Danger, "Failed"),
            };
            graphite::badge(ui, text, tone);
            if state != State::Idle {
                graphite::badge(ui, &format!("{} of {total} keys", self.known_done()), Tone::Neutral);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if busy {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    return;
                }
                ui.add_enabled_ui(can, |ui| {
                    if graphite::primary_button(ui, label).clicked() {
                        match state {
                            State::Running | State::Finished => {
                                self.pending = true;
                                self.send(tx, Request::StopCalibration);
                                if self.demo {
                                    self.pending = false;
                                    self.state = State::Idle;
                                    self.done.clear();
                                }
                            }
                            State::Idle | State::Failed(_) => self.confirm = true,
                        }
                    }
                });
            });
        });
        if let Some(path) = &self.log_path
            && self.is_active()
        {
            ui.add_space(space::S2);
            ui.label(
                graphite::text(format!("Session log: {}", path.display()), Type::Small).color(p.text_muted),
            );
        }
        ui.add_space(space::S4);

        let done = &self.done;
        keyboard_view(
            ui,
            "calibration",
            0.6,
            |_| false,
            |key| {
                let is_done = done.contains(&key.slot);
                KeyFace {
                    top: String::new(),
                    main: key.label.to_owned(),
                    main_color: if is_done { p.success } else { p.text },
                    unsaved: false,
                    swatch: None,
                    done: is_done,
                    tooltip: format!(
                        "{}: {}",
                        key.label,
                        if is_done { "calibrated in this run" } else { "not calibrated yet" }
                    ),
                }
            },
        );

        self.dialog(ui, tx);
    }

    /// The "Start calibration?" confirmation.
    fn dialog(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        if !self.confirm {
            return;
        }
        let p = graphite::palette(ui);
        let mut start = false;
        let mut cancel = false;
        let id = ui.make_persistent_id("start-calibration-dialog");
        let response = egui::Modal::new(id).show(ui.ctx(), |ui| {
            ui.set_width(size::DIALOG);
            ui.label(graphite::text("Start calibration?", Type::Title));
            ui.add_space(space::S3);
            ui.label(
                graphite::text(
                    "The keyboard will learn each switch's travel from what you do next, so you'll need to press every key all the way down.",
                    Type::Body,
                )
                .color(p.text_secondary),
            );
            ui.add_space(space::S2);
            ui.label(
                graphite::text(
                    "The current calibration can't be backed up. Stopping part way keeps whatever the keyboard has already learned.",
                    Type::Body,
                )
                .color(p.text_secondary),
            );
            ui.add_space(space::S6);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                start = graphite::primary_button(ui, "Start calibration").clicked();
                cancel = graphite::ghost_button(ui, "Cancel").clicked();
            });
        });
        if start {
            self.confirm = false;
            self.pending = true;
            if self.demo {
                self.pending = false;
            } else {
                self.send(tx, Request::StartCalibration);
            }
        } else if cancel || response.should_close() {
            self.confirm = false;
        }
    }
}
