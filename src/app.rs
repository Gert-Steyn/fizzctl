use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui::{Align, Layout, ScrollArea, Ui};
use lucide_icons::Icon;

use fizz::device::{Dir, Ident, PRODUCT_IDS};
use fizz::protocol::{self as p, Layer, LightConfig};

use crate::graphite::{self, Tone, Type, size, space};
use crate::keymap_page::{KeymapPage, Outcome};
use crate::worker::{LogEntry, Request, Snapshot, Update, Worker};

const THEME_KEY: &str = "theme";
const LOG_LIMIT: usize = 500;
const NOTICE_FOR: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ThemeChoice {
    System,
    Dark,
    Light,
}

impl ThemeChoice {
    fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Keymap,
    Lighting,
    Packets,
    Settings,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Keymap => "Key mapping",
            Page::Lighting => "Lighting",
            Page::Packets => "Packet log",
            Page::Settings => "Settings",
        }
    }

    fn icon(self) -> char {
        match self {
            Page::Overview => Icon::Info,
            Page::Keymap => Icon::Keyboard,
            Page::Lighting => Icon::Lightbulb,
            Page::Packets => Icon::Terminal,
            Page::Settings => Icon::Settings,
        }
        .unicode()
    }
}

enum Notice {
    Success(String),
    Error(String),
}

pub struct FizzApp {
    worker: Worker,
    page: Page,
    theme: ThemeChoice,
    device: Option<Ident>,
    snapshot: Option<Snapshot>,
    /// Lighting being edited; `None` until the keyboard has been read.
    light_edit: Option<LightConfig>,
    keymap: KeymapPage,
    applying: bool,
    notice: Option<(Notice, Instant)>,
    log: VecDeque<LogEntry>,
    started: Instant,
}

impl FizzApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme = cc
            .storage
            .and_then(|s| eframe::get_value(s, THEME_KEY))
            .unwrap_or(ThemeChoice::System);
        cc.egui_ctx.set_theme(theme.preference());

        // Developer overrides for screenshots; not persisted.
        let env = |k: &str| std::env::var(k).unwrap_or_default().to_lowercase();
        match env("FIZZ_THEME").as_str() {
            "dark" => cc.egui_ctx.set_theme(egui::ThemePreference::Dark),
            "light" => cc.egui_ctx.set_theme(egui::ThemePreference::Light),
            _ => {}
        }
        let page = match env("FIZZ_PAGE").as_str() {
            "keys" => Page::Keymap,
            "lighting" => Page::Lighting,
            "packets" => Page::Packets,
            "settings" => Page::Settings,
            _ => Page::Overview,
        };
        let mut keymap = KeymapPage::default();
        if let Ok(slot) = env("FIZZ_KEY").parse() {
            keymap.select(slot);
        }
        if env("FIZZ_LAYER") == "fn" {
            keymap.set_layer(Layer::Fn);
        }

        Self {
            worker: Worker::spawn(cc.egui_ctx.clone()),
            page,
            theme,
            device: None,
            snapshot: None,
            light_edit: None,
            keymap,
            applying: false,
            notice: None,
            log: VecDeque::new(),
            started: Instant::now(),
        }
    }

    fn handle_updates(&mut self) {
        while let Ok(update) = self.worker.rx.try_recv() {
            match update {
                Update::Connected(ident) => {
                    self.device = Some(ident);
                    self.snapshot = None;
                }
                Update::Disconnected => {
                    self.device = None;
                    self.snapshot = None;
                    self.light_edit = None;
                    self.applying = false;
                    self.keymap.reset();
                }
                Update::Snapshot(snapshot) => {
                    let device_light = self.snapshot.as_ref().and_then(|s| s.light);
                    // Keep unsaved edits; otherwise follow the keyboard.
                    if self.light_edit.is_none() || self.light_edit == device_light {
                        self.light_edit = snapshot.light;
                    }
                    self.keymap.on_snapshot(&snapshot);
                    self.snapshot = Some(snapshot);
                }
                Update::LightApplied(stored) => {
                    if let Some(s) = self.snapshot.as_mut() {
                        s.light = Some(stored);
                    }
                    self.light_edit = Some(stored);
                    self.applying = false;
                    self.notify(Notice::Success("Lighting applied.".into()));
                }
                Update::KeymapSaved(layer, stored) => {
                    if let Some(s) = self.snapshot.as_mut() {
                        match layer {
                            Layer::Default => s.keymap_default = Some(stored.clone()),
                            Layer::Fn => s.keymap_fn = Some(stored.clone()),
                        }
                    }
                    if let Outcome::Saved = self.keymap.on_saved(layer, stored) {
                        self.notify(Notice::Success("Key map saved to the keyboard.".into()));
                    }
                }
                Update::Error(message) => {
                    self.applying = false;
                    self.keymap.on_error();
                    self.notify(Notice::Error(message));
                }
                Update::Packet(entry) => {
                    if self.log.len() == LOG_LIMIT {
                        self.log.pop_front();
                    }
                    self.log.push_back(entry);
                }
            }
        }
    }

    fn notify(&mut self, notice: Notice) {
        self.notice = Some((notice, Instant::now()));
    }

    fn refresh(&mut self) {
        let _ = self.worker.tx.send(Request::Refresh);
    }

    // -----------------------------------------------------------------------
    // Shell
    // -----------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(space::S2);
        ui.horizontal(|ui| {
            ui.add_space(space::S3);
            ui.label(graphite::text("Fizz control", Type::Heading));
        });
        ui.add_space(space::S4);
        for page in [Page::Overview, Page::Keymap, Page::Lighting, Page::Packets] {
            if graphite::nav_row(ui, Some(page.icon()), page.title(), self.page == page).clicked()
            {
                self.page = page;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            let page = Page::Settings;
            if graphite::nav_row(ui, Some(page.icon()), page.title(), self.page == page).clicked()
            {
                self.page = page;
            }
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(graphite::text(self.page.title(), Type::Heading));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.device.is_some()
                    && graphite::icon_button(ui, Icon::RefreshCw.unicode(), "Read keyboard again")
                        .clicked()
                {
                    self.refresh();
                }
                match &self.device {
                    Some(_) => graphite::badge(ui, "Connected", Tone::Success),
                    None => graphite::badge(ui, "Not connected", Tone::Neutral),
                };
            });
        });
    }

    fn notice(&mut self, ui: &mut Ui) {
        let Some((notice, at)) = &self.notice else {
            return;
        };
        let expired = matches!(notice, Notice::Success(_)) && at.elapsed() > NOTICE_FOR;
        if expired {
            self.notice = None;
            return;
        }
        match notice {
            Notice::Success(m) => {
                graphite::banner(ui, Tone::Success, Icon::CircleCheck.unicode(), m);
                ui.ctx().request_repaint_after(NOTICE_FOR);
            }
            Notice::Error(m) => {
                graphite::banner(ui, Tone::Danger, Icon::CircleAlert.unicode(), m);
            }
        }
        ui.add_space(space::S5);
    }

    // -----------------------------------------------------------------------
    // Pages
    // -----------------------------------------------------------------------

    /// Shared "not connected" / "reading" states. Returns the snapshot when
    /// the page can draw its content.
    fn connected_or_placeholder(&self, ui: &mut Ui) -> Option<Snapshot> {
        if self.device.is_none() {
            ui.add_space(space::S8);
            graphite::empty_state(
                ui,
                Some(Icon::Unplug.unicode()),
                "No keyboard connected",
                "Plug in the Fizz Magnetic Plus with its USB cable. It's detected automatically.",
                None,
            );
            return None;
        }
        match &self.snapshot {
            Some(s) => Some(s.clone()),
            None => {
                ui.add_space(space::S8);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(
                        graphite::text("Reading keyboard", Type::Body)
                            .color(graphite::palette(ui).text_secondary),
                    );
                });
                None
            }
        }
    }

    fn overview(&mut self, ui: &mut Ui) {
        let Some(snap) = self.connected_or_placeholder(ui) else {
            return;
        };
        let ident = self.device.clone().unwrap_or_else(|| Ident {
            vendor_id: 0,
            product_id: 0,
            product_name: String::new(),
            manufacturer: String::new(),
            path: String::new(),
        });
        let unknown = || "Unknown".to_owned();

        section_heading(ui, "Device");
        graphite::card(ui, |ui| {
            let product = snap.product.as_ref();
            let name = if ident.product_name.is_empty() {
                unknown()
            } else {
                ident.product_name.clone()
            };
            graphite::setting_row(ui, "Name", None, |ui| {
                ui.label(graphite::value_text(ui, name, false));
            });
            graphite::divider(ui);
            let firmware = product.and_then(|p| p.firmware()).unwrap_or_else(unknown);
            let built = product.and_then(|p| p.build_date.clone());
            graphite::setting_row(ui, "Firmware", built.as_deref(), |ui| {
                ui.label(graphite::value_text(ui, firmware, false));
            });
            graphite::divider(ui);
            let code = product
                .and_then(|p| p.product())
                .map(str::to_owned)
                .unwrap_or_else(unknown);
            graphite::setting_row(ui, "Product code", None, |ui| {
                ui.label(graphite::value_text(ui, code, true));
            });
            graphite::divider(ui);
            let serial = snap.serial.clone().unwrap_or_else(unknown);
            graphite::setting_row(ui, "Serial number", None, |ui| {
                ui.label(graphite::value_text(ui, serial, true));
            });
            graphite::divider(ui);
            let usb = format!("{:04X}:{:04X}", ident.vendor_id, ident.product_id);
            let mode = if ident.product_id == PRODUCT_IDS[0] {
                "Wired USB, keyboard mode"
            } else {
                "Wired USB, gamepad mode"
            };
            graphite::setting_row(ui, "Connection", Some(mode), |ui| {
                ui.label(graphite::value_text(ui, usb, true));
            });
        });

        ui.add_space(space::S6);
        section_heading(ui, "Performance");
        graphite::card(ui, |ui| {
            let rate = snap
                .poll_rate_hz
                .map(|hz| format!("{hz} Hz"))
                .unwrap_or_else(unknown);
            graphite::setting_row(ui, "Polling rate", None, |ui| {
                ui.label(graphite::value_text(ui, rate, false));
            });
            graphite::divider(ui);
            let calibrated = snap.status.as_ref().map(|s| s.support.calibrated);
            graphite::setting_row(ui, "Switch calibration", None, |ui| {
                match calibrated {
                    Some(true) => graphite::badge(ui, "Calibrated", Tone::Success),
                    Some(false) => graphite::badge(ui, "Not calibrated", Tone::Warning),
                    None => graphite::badge(ui, "Unknown", Tone::Neutral),
                };
            });
        });

        if let Some(status) = &snap.status {
            ui.add_space(space::S6);
            section_heading(ui, "Supported features");
            graphite::card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for feature in status.support.features() {
                        graphite::badge(ui, feature, Tone::Neutral);
                    }
                });
            });
        }
    }

    fn lighting(&mut self, ui: &mut Ui) {
        let Some(snap) = self.connected_or_placeholder(ui) else {
            return;
        };
        let (Some(device_light), Some(mut edit)) = (snap.light, self.light_edit) else {
            graphite::empty_state(
                ui,
                Some(Icon::CircleAlert.unicode()),
                "Couldn't read the lighting",
                "The keyboard didn't answer the lighting request. Read it again to retry.",
                Some("Read again"),
            )
            .then(|| self.refresh());
            return;
        };
        let (modes, max_brightness, max_speed) = match &snap.modes {
            Some(m) => (m.modes.clone(), m.max_brightness, m.max_speed),
            None => (vec![edit.mode], 4, 4),
        };
        let p = graphite::palette(ui);

        ui.horizontal(|ui| {
            section_heading(ui, "Key lighting");
            if edit != device_light {
                graphite::badge(ui, "Unsaved changes", Tone::Accent);
            }
        });
        ui.add_enabled_ui(!self.applying, |ui| {
            graphite::card(ui, |ui| {
                graphite::setting_row(ui, "Effect", None, |ui| {
                    egui::ComboBox::from_id_salt("effect")
                        .selected_text(p::mode_name(edit.mode))
                        .show_ui(ui, |ui| {
                            for mode in &modes {
                                ui.selectable_value(&mut edit.mode, *mode, p::mode_name(*mode));
                            }
                        });
                });
                graphite::divider(ui);
                graphite::setting_row(ui, "Brightness", None, |ui| {
                    ui.add(egui::Slider::new(&mut edit.brightness, 0..=max_brightness));
                });
                graphite::divider(ui);
                graphite::setting_row(ui, "Speed", Some("For animated effects."), |ui| {
                    ui.add(egui::Slider::new(&mut edit.speed, 0..=max_speed));
                });
                graphite::divider(ui);
                graphite::setting_row(
                    ui,
                    "Rainbow colours",
                    Some("Cycle through all colours instead of using the colour below."),
                    |ui| {
                        ui.add(egui::Checkbox::without_text(&mut edit.full_color));
                    },
                );
                graphite::divider(ui);
                graphite::setting_row(ui, "Colour", None, |ui| {
                    ui.color_edit_button_srgb(&mut edit.foreground);
                    ui.label(graphite::value_text(ui, hex_color(edit.foreground), true));
                });
                graphite::divider(ui);
                graphite::setting_row(
                    ui,
                    "Background colour",
                    Some("Used by effects that light keys over a base colour."),
                    |ui| {
                        ui.color_edit_button_srgb(&mut edit.background);
                        ui.label(graphite::value_text(ui, hex_color(edit.background), true));
                    },
                );
                graphite::divider(ui);
                graphite::setting_row(ui, "Direction", Some("For moving effects."), |ui| {
                    egui::ComboBox::from_id_salt("direction")
                        .selected_text(p::direction_name(edit.direction))
                        .show_ui(ui, |ui| {
                            for d in 0..6 {
                                ui.selectable_value(&mut edit.direction, d, p::direction_name(d));
                            }
                        });
                });
            });
        });
        self.light_edit = Some(edit);

        ui.add_space(space::S5);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let changed = edit != device_light;
                if self.applying {
                    ui.add(egui::Spinner::new().size(size::ICON));
                    ui.label(graphite::text("Applying", Type::Body).color(p.text_secondary));
                } else {
                    if graphite::primary_button(ui, "Apply lighting").clicked() {
                        if changed {
                            self.applying = true;
                            let _ = self.worker.tx.send(Request::ApplyLight(edit));
                        } else {
                            self.notify(Notice::Success(
                                "The keyboard already has these settings.".into(),
                            ));
                        }
                    }
                    if ui
                        .add_enabled(changed, egui::Button::new("Discard changes"))
                        .clicked()
                    {
                        self.light_edit = Some(device_light);
                    }
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(
                        graphite::text("Changes are sent when you apply them.", Type::Small)
                            .color(p.text_muted),
                    );
                });
            });
        });
    }

    fn packets(&mut self, ui: &mut Ui) {
        let p = graphite::palette(ui);
        ui.horizontal(|ui| {
            ui.label(
                graphite::text(
                    "Every configuration packet sent to and from the keyboard. Heartbeats are hidden.",
                    Type::Small,
                )
                .color(p.text_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if graphite::ghost_button(ui, "Clear log").clicked() {
                    self.log.clear();
                }
            });
        });
        ui.add_space(space::S3);
        if self.log.is_empty() {
            ui.add_space(space::S8);
            graphite::empty_state(
                ui,
                Some(Icon::Terminal.unicode()),
                "No packets yet",
                "Packets appear here as the app talks to the keyboard.",
                None,
            );
            return;
        }
        graphite::card(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = space::S2;
                    for entry in &self.log {
                        let secs = entry.at.duration_since(self.started).as_secs_f32();
                        ui.horizontal(|ui| {
                            ui.label(
                                graphite::text(format!("{secs:>8.3}"), Type::Mono)
                                    .color(p.text_muted),
                            );
                            match entry.dir {
                                Dir::Out => graphite::badge(ui, "Sent", Tone::Accent),
                                Dir::In => graphite::badge(ui, "Received", Tone::Neutral),
                            };
                            ui.add(
                                egui::Label::new(graphite::text(
                                    trimmed_hex(&entry.packet),
                                    Type::Mono,
                                ))
                                .wrap(),
                            );
                        });
                    }
                });
        });
    }

    fn settings(&mut self, ui: &mut Ui) {
        section_heading(ui, "Appearance");
        graphite::card(ui, |ui| {
            let mut theme = self.theme;
            graphite::setting_row(ui, "Theme", Some("System follows your OS setting."), |ui| {
                graphite::segmented(
                    ui,
                    &mut theme,
                    &[
                        (ThemeChoice::System, "System"),
                        (ThemeChoice::Dark, "Dark"),
                        (ThemeChoice::Light, "Light"),
                    ],
                );
            });
            if theme != self.theme {
                self.theme = theme;
                ui.ctx().set_theme(theme.preference());
            }
        });

        ui.add_space(space::S6);
        section_heading(ui, "About");
        graphite::card(ui, |ui| {
            graphite::setting_row(ui, "Version", None, |ui| {
                ui.label(graphite::value_text(ui, env!("CARGO_PKG_VERSION"), true));
            });
        });
    }
}

impl eframe::App for FizzApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.handle_updates();
        let dark = ui.visuals().dark_mode;

        egui::Panel::left("sidebar")
            .exact_size(size::SIDEBAR)
            .resizable(false)
            .frame(graphite::sidebar_frame(dark))
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::top("toolbar")
            .exact_size(size::TOOLBAR)
            .frame(graphite::toolbar_frame(dark))
            .show(ui, |ui| self.toolbar(ui));
        egui::CentralPanel::default()
            .frame(graphite::page_frame(dark))
            .show(ui, |ui| {
                // `set_max_width` sets the width outright, so cap it ourselves.
                ui.set_max_width(ui.available_width().min(size::CONTENT_MAX));
                self.notice(ui);
                match self.page {
                    Page::Packets => self.packets(ui),
                    Page::Keymap => {
                        if self.connected_or_placeholder(ui).is_some() {
                            self.keymap.show(ui, &self.worker.tx);
                        }
                    }
                    page => {
                        ScrollArea::vertical().id_salt(page.title()).show(ui, |ui| match page {
                            Page::Overview => self.overview(ui),
                            Page::Lighting => self.lighting(ui),
                            _ => self.settings(ui),
                        });
                    }
                }
            });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, THEME_KEY, &self.theme);
    }
}

fn section_heading(ui: &mut Ui, title: &str) {
    ui.label(graphite::text(title, Type::Heading));
    ui.add_space(space::S2);
}

fn hex_color(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Hex of a packet without its trailing zero padding.
fn trimmed_hex(packet: &p::Packet) -> String {
    let end = packet.iter().rposition(|b| *b != 0).map_or(1, |i| i + 1);
    let mut s = p::hex(&packet[..end]);
    if end < packet.len() {
        s.push_str(" …");
    }
    s
}
