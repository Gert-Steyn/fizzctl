//! The "Backup and restore" section of the Settings page: save every
//! setting on the keyboard to a file, and restore one of the saved files.
//!
//! Files live in the app's data folder (`profiles`). Restoring always saves
//! the current settings first, so a restore can be undone.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use egui::{Align, Layout, Ui};
use lucide_icons::Icon;

use fizz::profile::{Outcome, Profile, SectionResult, file_stamp, format_utc};

use crate::graphite::{self, Tone, Type, size, space};
use crate::worker::Request;

/// Name the app is registered under, which decides the data folder.
const APP_ID: &str = "Fizz control";
/// Files larger than this aren't settings files.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

struct SavedFile {
    name: String,
    profile: Profile,
}

impl SavedFile {
    fn is_pre_restore(&self) -> bool {
        self.name.starts_with("before-restore")
    }
}

#[derive(Default)]
pub struct BackupSection {
    dir: Option<PathBuf>,
    files: Vec<SavedFile>,
    loaded: bool,
    saving: bool,
    restoring: bool,
    /// Index into `files` of the file the restore dialog is open for.
    confirm: Option<usize>,
}

impl BackupSection {
    pub fn new() -> Self {
        Self {
            dir: eframe::storage_dir(APP_ID).map(|d| d.join("profiles")),
            ..Self::default()
        }
    }

    /// Forget the file list so it is read again next time the section is shown.
    pub fn invalidate(&mut self) {
        self.loaded = false;
        self.confirm = None;
    }

    fn refresh(&mut self) {
        self.loaded = true;
        self.files.clear();
        let Some(dir) = &self.dir else { return };
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
            let small = entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
            if !is_json || !small {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(profile) = Profile::from_json(&text) else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            self.files.push(SavedFile { name, profile });
        }
        self.files.sort_by_key(|f| std::cmp::Reverse(f.profile.saved_at));
    }

    /// A request failed or the keyboard went away: stop showing progress.
    pub fn on_error(&mut self) {
        self.saving = false;
        self.restoring = false;
    }

    /// Result of a save: a message for the notice at the top of the page.
    pub fn on_saved(&mut self, result: Result<PathBuf, String>) -> Result<String, String> {
        self.saving = false;
        self.refresh();
        result.map(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            format!("Settings saved as {name}.")
        })
    }

    /// Result of a restore: a message for the notice at the top of the page.
    pub fn on_restored(
        &mut self,
        result: Result<(Vec<SectionResult>, PathBuf), String>,
    ) -> Result<String, String> {
        self.restoring = false;
        self.refresh();
        let (results, backup) = result?;
        let bad: Vec<String> = results
            .iter()
            .filter_map(|r| match &r.outcome {
                Outcome::Mismatch(_) | Outcome::Failed(_) => Some(r.section.name().to_owned()),
                _ => None,
            })
            .collect();
        let backup_name = backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if bad.is_empty() {
            Ok("Settings restored. Every section was written and checked.".to_owned())
        } else {
            Err(format!(
                "Restored, but these sections didn't match afterwards: {}. The settings from before are saved as {backup_name}.",
                bad.join(", ")
            ))
        }
    }

    pub fn show(&mut self, ui: &mut Ui, tx: &Sender<Request>, connected: bool, product: Option<&str>) {
        if !self.loaded {
            self.refresh();
        }
        let p = graphite::palette(ui);
        let busy = self.saving || self.restoring;
        ui.label(graphite::text("Backup and restore", Type::Heading));
        ui.add_space(space::S2);

        let mut ask = None;
        graphite::card(ui, |ui| {
            graphite::setting_row(
                ui,
                "Save settings",
                Some("Saves the key map, lighting, key colours, actuation, snap tap, macros and polling rate to a file."),
                |ui| {
                    if self.saving {
                        ui.add(egui::Spinner::new().size(size::ICON));
                    }
                    let can = connected && !busy && self.dir.is_some();
                    if ui.add_enabled(can, egui::Button::new("Save settings")).clicked()
                        && let Some(dir) = &self.dir
                    {
                        self.saving = true;
                        let file = dir.join(format!("fizz-{}.json", file_stamp(unix_now())));
                        let _ = tx.send(Request::SaveProfile(file));
                    }
                },
            );
            if let Some(dir) = &self.dir {
                graphite::divider(ui);
                graphite::setting_row(
                    ui,
                    "Saved files",
                    Some(&dir.display().to_string()),
                    |ui| {
                        if graphite::ghost_button(ui, "Open folder").clicked() {
                            open_folder(dir);
                        }
                    },
                );
            }
            if !connected {
                graphite::divider(ui);
                ui.add_space(space::S2);
                ui.label(
                    graphite::text("Connect the keyboard to save or restore settings.", Type::Small)
                        .color(p.text_muted),
                );
                ui.add_space(space::S2);
            }
            for (i, file) in self.files.iter().enumerate() {
                graphite::divider(ui);
                let when = format!("{} UTC", format_utc(file.profile.saved_at));
                let kind = if file.is_pre_restore() { "Saved before a restore" } else { "Saved settings" };
                let firmware = file.profile.firmware.as_deref().map(|f| format!(", firmware {f}")).unwrap_or_default();
                let detail = format!("{kind}. {} sections{firmware}.", file.profile.sections().len());
                graphite::setting_row(ui, &when, Some(&detail), |ui| {
                    let can = connected && !busy;
                    if ui.add_enabled(can, egui::Button::new("Restore")).clicked() {
                        ask = Some(i);
                    }
                });
            }
            if self.files.is_empty() && self.dir.is_some() {
                graphite::divider(ui);
                ui.add_space(space::S2);
                ui.label(graphite::text("No saved files yet.", Type::Small).color(p.text_muted));
                ui.add_space(space::S2);
            }
        });
        if ask.is_some() {
            self.confirm = ask;
        }
        self.dialog(ui, tx, product);
    }

    /// The "Restore settings?" confirmation.
    fn dialog(&mut self, ui: &mut Ui, tx: &Sender<Request>, product: Option<&str>) {
        let Some(i) = self.confirm else { return };
        let Some(file) = self.files.get(i) else {
            self.confirm = None;
            return;
        };
        let p = graphite::palette(ui);
        let sections: Vec<&str> = file.profile.sections().iter().map(|s| s.name()).collect();
        let other_model = match (file.profile.product.as_deref(), product) {
            (Some(file), Some(this)) if file != this => Some(file.to_owned()),
            _ => None,
        };
        let mut restore = false;
        let mut cancel = false;
        let id = ui.make_persistent_id("restore-settings-dialog");
        let response = egui::Modal::new(id).show(ui.ctx(), |ui| {
            ui.set_width(size::DIALOG);
            ui.label(graphite::text("Restore settings?", Type::Title));
            ui.add_space(space::S3);
            ui.label(
                graphite::text(
                    format!(
                        "This replaces what the keyboard holds with the saved values for: {}.",
                        sections.join(", ")
                    ),
                    Type::Body,
                )
                .color(p.text_secondary),
            );
            ui.add_space(space::S2);
            ui.label(
                graphite::text(
                    "Your current settings are saved first, so you can restore them again afterwards.",
                    Type::Body,
                )
                .color(p.text_secondary),
            );
            if let Some(model) = &other_model {
                ui.add_space(space::S3);
                graphite::banner(
                    ui,
                    Tone::Warning,
                    Icon::TriangleAlert.unicode(),
                    &format!("This file came from a different keyboard model ({model}). It may not suit this keyboard."),
                );
            }
            ui.add_space(space::S6);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                restore = graphite::primary_button(ui, "Restore settings").clicked();
                cancel = graphite::ghost_button(ui, "Cancel").clicked();
            });
        });
        if restore {
            self.restoring = true;
            self.confirm = None;
            if let (Some(dir), Some(file)) = (self.dir.clone(), self.files.get(i)) {
                let _ = tx.send(Request::RestoreProfile {
                    profile: Box::new(file.profile.clone()),
                    dir,
                });
            }
        } else if cancel || response.should_close() {
            self.confirm = None;
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Show a folder in the system file manager, creating it first if needed.
fn open_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}
