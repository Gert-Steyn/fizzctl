//! The Key test page: shows how far the key you are pressing is down, live.
//!
//! Like the vendor app, it shows one key at a time (the one being pressed) and
//! marks where that key actuates. The stream is only requested while this page
//! is open and is always stopped when it is left.

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;

use egui::{Align, Layout, Sense, Ui, vec2};
use lucide_icons::Icon;

use fizz::keymap::LAYOUT;
use fizz::protocol::{KeyTrigger, TriggerConfig};

use crate::graphite::{self, Tone, Type, radius, size, space};
use crate::keyboard_view::{KeyFace, capture_keys, keyboard_view};
use crate::worker::Request;

/// Steps below this count as "not pressed" (the sensor idles at a few steps).
const PRESSED_MIN: u16 = 6;

pub struct KeyTestPage {
    running: bool,
    /// A start or stop request is on its way.
    pending: bool,
    config: Option<TriggerConfig>,
    triggers: Vec<Option<KeyTrigger>>,
    /// Latest depth of every key that has reported, in steps.
    depths: BTreeMap<usize, u16>,
    /// The key shown in the readout: the one most recently pressed.
    current: Option<usize>,
    /// Deepest value seen for the current key since it was last released.
    deepest: u16,
    /// Developer override (`FIZZ_KEYTEST_DEMO`): simulated readings, nothing sent.
    demo: bool,
}

impl KeyTestPage {
    pub fn new() -> Self {
        Self {
            running: false,
            pending: false,
            config: None,
            triggers: Vec::new(),
            depths: BTreeMap::new(),
            current: None,
            deepest: 0,
            demo: std::env::var("FIZZ_KEYTEST_DEMO").is_ok(),
        }
    }

    fn send(&self, tx: &Sender<Request>, request: Request) {
        if !self.demo {
            let _ = tx.send(request);
        }
    }

    /// Stop the stream because the user went elsewhere.
    pub fn leave(&mut self, tx: &Sender<Request>) {
        if self.running || self.pending {
            self.send(tx, Request::StopTravelTest);
            self.reset();
        }
    }

    pub fn reset(&mut self) {
        self.running = false;
        self.pending = false;
        self.depths.clear();
        self.current = None;
        self.deepest = 0;
    }

    pub fn on_started(&mut self, config: Option<TriggerConfig>, triggers: Vec<Option<KeyTrigger>>) {
        self.running = true;
        self.pending = false;
        self.config = config;
        self.triggers = triggers;
        self.depths.clear();
        self.current = None;
        self.deepest = 0;
    }

    pub fn on_stopped(&mut self) {
        self.reset();
    }

    pub fn on_error(&mut self) {
        self.pending = false;
    }

    pub fn on_travel(&mut self, batch: Vec<(usize, u16)>) {
        for (slot, depth) in batch {
            self.depths.insert(slot, depth);
            if depth >= PRESSED_MIN {
                if self.current != Some(slot) {
                    self.current = Some(slot);
                    self.deepest = 0;
                }
                self.deepest = self.deepest.max(depth);
            }
        }
    }

    fn unit(&self) -> f32 {
        self.config.map_or(0.01, |c| c.unit_mm)
    }

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>) {
        if self.demo && !self.running {
            self.running = true;
            self.config = Some(TriggerConfig { unit_mm: 0.01, min_steps: 10, max_steps: 340, rt_unit_mm: 0.01 });
            self.triggers = vec![Some(KeyTrigger { mode: 0x0C, travel: 170, press: 51, release: 51 }); 132];
            self.depths = LAYOUT.iter().map(|k| (k.slot, 0)).collect();
            self.depths.insert(22, 215);
            self.current = Some(22);
            self.deepest = 262;
        }
        if self.running {
            capture_keys(ui, "key-test");
        }
        let p = graphite::palette(ui);

        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(graphite::text("Test the keys", Type::Heading));
            ui.add_space(space::S2);
            ui.label(
                graphite::text(
                    "Press a key and see how far down it is, in millimetres. The marker shows where the key actuates. Handy for checking that a setting feels like you expect.",
                    Type::Body,
                )
                .color(p.text_secondary),
            );
        });
        ui.add_space(space::S4);

        let running = self.running;
        let busy = self.pending;
        ui.horizontal(|ui| {
            graphite::badge(
                ui,
                if running { "Testing" } else { "Not started" },
                if running { Tone::Accent } else { Tone::Neutral },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if busy {
                    ui.add(egui::Spinner::new().size(size::ICON));
                } else if graphite::primary_button(ui, if running { "Stop test" } else { "Start test" }).clicked() {
                    self.pending = true;
                    if running {
                        self.send(tx, Request::StopTravelTest);
                    } else {
                        self.send(tx, Request::StartTravelTest);
                    }
                    if self.demo {
                        self.pending = false;
                        if running {
                            self.reset();
                        }
                    }
                }
            });
        });
        ui.add_space(space::S4);

        if !running {
            graphite::banner(
                ui,
                Tone::Neutral,
                Icon::Info.unicode(),
                "Choose Start test, then press keys. While the test runs, key presses don't operate this window.",
            );
            return;
        }

        self.readout(ui);
        ui.add_space(space::S4);

        let depths = &self.depths;
        let unit = self.unit();
        keyboard_view(
            ui,
            "key-test",
            0.6,
            |slot| Some(slot) == self.current,
            |key| {
                let depth = depths.get(&key.slot).copied().unwrap_or(0);
                let pressed = depth >= PRESSED_MIN;
                KeyFace {
                    top: String::new(),
                    main: key.label.to_owned(),
                    main_color: if pressed { p.success } else { p.text },
                    unsaved: false,
                    swatch: None,
                    done: pressed,
                    tooltip: format!("{}: {:.2} mm", key.label, depth as f32 * unit),
                }
            },
        );
    }

    /// The big readout for the key being pressed, with a depth bar.
    fn readout(&self, ui: &mut Ui) {
        let p = graphite::palette(ui);
        let unit = self.unit();
        graphite::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(slot) = self.current else {
                ui.label(graphite::text("Press a key", Type::Title).color(p.text_secondary));
                return;
            };
            let name = LAYOUT.iter().find(|k| k.slot == slot).map_or("?", |k| k.label);
            let depth = self.depths.get(&slot).copied().unwrap_or(0);
            let trigger = self.triggers.get(slot).copied().flatten();
            let max = self.config.map_or(340, |c| c.max_steps).max(1);

            ui.horizontal(|ui| {
                ui.label(graphite::text(name, Type::Title));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(graphite::value_text(ui, format!("{:.2} mm", depth as f32 * unit), true));
                });
            });
            ui.add_space(space::S3);

            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), space::S4), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, radius::SM, p.raised);
            let frac = |steps: u16| (steps as f32 / max as f32).clamp(0.0, 1.0);
            let mut fill = rect;
            fill.set_right(rect.left() + rect.width() * frac(depth));
            let actuated = trigger.is_some_and(|t| depth >= t.travel);
            painter.rect_filled(fill, radius::SM, if actuated { p.success } else { p.accent });
            if let Some(t) = trigger {
                let x = rect.left() + rect.width() * frac(t.travel);
                painter.vline(x, rect.y_range(), egui::Stroke::new(2.0, p.text));
            }
            ui.add_space(space::S2);
            let mut text = format!("Deepest this press: {:.2} mm", self.deepest as f32 * unit);
            if let Some(t) = trigger {
                text += &format!("   Actuates at: {:.2} mm", t.travel as f32 * unit);
            }
            ui.label(graphite::text(text, Type::Small).color(p.text_muted));
        });
    }
}
