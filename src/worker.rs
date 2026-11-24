//! Background thread that owns the keyboard connection, so USB I/O never
//! blocks the UI. It reconnects automatically and sends a status heartbeat to
//! notice when the cable is unplugged.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use fizz::device::{Dir, Ident, Keyboard};
use fizz::profile::{self, Profile, SectionResult};
use fizz::protocol::{
    self as p, KeyAction, KeyTrigger, Layer, LightConfig, Macro, ModeList, Packet, ProductInfo, SocdPair,
    SocdPower, Status, TriggerConfig, Zone,
};

const TIMEOUT: Duration = Duration::from_millis(400);
const PAGE_IDLE: Duration = Duration::from_millis(300);
const RECONNECT_EVERY: Duration = Duration::from_secs(1);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(2);


pub enum Request {
    Refresh,
    ApplyLight(LightConfig),
    SaveKeymap(Layer, Vec<KeyAction>),
    /// Actuation settings, grouped: each setting with the slots it applies to.
    SaveTriggers(Vec<(KeyTrigger, Vec<usize>)>),
    SetPollRate(u32),
    /// Per-key colours for colour layer 0 or 1.
    SaveColours(u8, Vec<[u8; 3]>),
    /// All 20 SOCD slots.
    SaveSocdPairs(Vec<Option<SocdPair>>),
    SetSocdPower(bool),
    /// All 10 macro slots (they can only be saved together).
    SaveMacros(Vec<Option<Macro>>),
    /// Read every setting and save it to this file.
    SaveProfile(PathBuf),
    /// Back up the current settings into `dir`, then write this profile.
    RestoreProfile { profile: Box<Profile>, dir: PathBuf },
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub status: Option<Status>,
    pub product: Option<ProductInfo>,
    pub serial: Option<String>,
    pub poll_rate_hz: Option<u32>,
    pub modes: Option<ModeList>,
    pub light: Option<LightConfig>,
    pub keymap_default: Option<Vec<KeyAction>>,
    pub keymap_fn: Option<Vec<KeyAction>>,
    pub trigger_config: Option<TriggerConfig>,
    /// Actuation per slot (`KEY_SLOTS` long); `None` for empty slots or keys that didn't answer.
    pub triggers: Option<Vec<Option<KeyTrigger>>>,
    /// The colour layer the Custom effect shows, and its per-slot colours.
    pub colour_layer: Option<u8>,
    pub colours: Option<Vec<[u8; 3]>>,
    pub socd_pairs: Option<Vec<Option<SocdPair>>>,
    pub socd_power: Option<SocdPower>,
    pub macros: Option<Vec<Option<Macro>>>,
}

pub struct LogEntry {
    pub at: Instant,
    pub dir: Dir,
    pub packet: Packet,
}

pub enum Update {
    Connected(Ident),
    Disconnected,
    Snapshot(Box<Snapshot>),
    LightApplied(LightConfig),
    KeymapSaved(Layer, Vec<KeyAction>),
    TriggersSaved(Vec<(usize, Option<KeyTrigger>)>),
    /// The polling rate read back after setting it.
    PollRateSet(Option<u32>),
    ColoursSaved(u8, Vec<[u8; 3]>),
    SocdPairsSaved(Vec<Option<SocdPair>>),
    SocdPowerSet(Option<SocdPower>),
    MacrosSaved(Vec<Option<Macro>>),
    ProfileSaved(Result<PathBuf, String>),
    /// What happened to each section, and where the previous settings were backed up.
    ProfileRestored(Result<(Vec<SectionResult>, PathBuf), String>),
    Error(String),
    Packet(LogEntry),
}

pub struct Worker {
    pub tx: Sender<Request>,
    pub rx: Receiver<Update>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (req_tx, req_rx) = mpsc::channel();
        let (upd_tx, upd_rx) = mpsc::channel();
        thread::Builder::new()
            .name("keyboard".into())
            .spawn(move || run(ctx, req_rx, upd_tx))
            .expect("couldn't start the keyboard thread");
        Self {
            tx: req_tx,
            rx: upd_rx,
        }
    }
}

struct Link {
    ctx: egui::Context,
    tx: Sender<Update>,
}

impl Link {
    fn send(&self, update: Update) -> bool {
        let ok = self.tx.send(update).is_ok();
        self.ctx.request_repaint();
        ok
    }
}

fn run(ctx: egui::Context, rx: Receiver<Request>, tx: Sender<Update>) {
    let link = Link { ctx, tx };
    let mut api = loop {
        match hidapi::HidApi::new() {
            Ok(api) => break api,
            Err(e) => {
                link.send(Update::Error(format!("Couldn't start USB HID access: {e}")));
                thread::sleep(RECONNECT_EVERY);
            }
        }
    };

    let mut kb: Option<Keyboard> = None;
    loop {
        let Some(keyboard) = kb.as_mut() else {
            match rx.recv_timeout(RECONNECT_EVERY) {
                Ok(
                    Request::ApplyLight(_)
                    | Request::SaveKeymap(..)
                    | Request::SaveTriggers(_)
                    | Request::SetPollRate(_)
                    | Request::SaveColours(..)
                    | Request::SaveSocdPairs(_)
                    | Request::SetSocdPower(_)
                    | Request::SaveMacros(_)
                    | Request::SaveProfile(_)
                    | Request::RestoreProfile { .. },
                ) => {
                    link.send(Update::Error("The keyboard isn't connected.".into()));
                }
                Ok(Request::Refresh) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            let _ = api.refresh_devices();
            if let Ok(mut opened) = Keyboard::open(&api) {
                let log_tx = link.tx.clone();
                opened.on_packet = Some(Box::new(move |dir, packet| {
                    let _ = log_tx.send(Update::Packet(LogEntry {
                        at: Instant::now(),
                        dir,
                        packet: *packet,
                    }));
                }));
                link.send(Update::Connected(opened.ident.clone()));
                let snapshot = read_snapshot(&mut opened);
                link.send(Update::Snapshot(Box::new(snapshot)));
                kb = Some(opened);
            }
            continue;
        };

        let result = match rx.recv_timeout(HEARTBEAT_EVERY) {
            Ok(Request::Refresh) => {
                let snapshot = read_snapshot(keyboard);
                link.send(Update::Snapshot(Box::new(snapshot)));
                Ok(())
            }
            Ok(Request::ApplyLight(cfg)) => apply_light(keyboard, &cfg, &link),
            Ok(Request::SaveKeymap(layer, keys)) => save_keymap(keyboard, layer, &keys, &link),
            Ok(Request::SaveTriggers(groups)) => save_triggers(keyboard, &groups, &link),
            Ok(Request::SetPollRate(hz)) => set_poll_rate(keyboard, hz, &link),
            Ok(Request::SaveColours(layer, colours)) => save_colours(keyboard, layer, &colours, &link),
            Ok(Request::SaveSocdPairs(pairs)) => save_socd_pairs(keyboard, &pairs, &link),
            Ok(Request::SetSocdPower(on)) => set_socd_power(keyboard, on, &link),
            Ok(Request::SaveMacros(macros)) => save_macros(keyboard, &macros, &link),
            Ok(Request::SaveProfile(file)) => save_profile(keyboard, &file, &link),
            Ok(Request::RestoreProfile { profile, dir }) => {
                restore_profile(keyboard, &profile, &dir, &link)
            }
            Err(RecvTimeoutError::Timeout) => heartbeat(keyboard),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Err(e) = result {
            // A failed write almost always means the cable was pulled.
            if matches!(e, fizz::device::Error::Hid(_)) {
                kb = None;
                link.send(Update::Disconnected);
            } else {
                link.send(Update::Error(e.to_string()));
            }
        }
    }
}

/// Status request with packet logging muted, so the log isn't flooded.
fn heartbeat(kb: &mut Keyboard) -> fizz::device::Result<()> {
    let log = kb.on_packet.take();
    let result = kb.request_same("status", &p::status_request(), TIMEOUT);
    kb.on_packet = log;
    match result {
        // A missed heartbeat reply isn't worth an error; a pulled cable shows
        // up as a failed write instead.
        Ok(_) | Err(fizz::device::Error::Timeout(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

fn apply_light(kb: &mut Keyboard, cfg: &LightConfig, link: &Link) -> fizz::device::Result<()> {
    match kb.write_light(Zone::Keys, cfg)? {
        Some(stored) => link.send(Update::LightApplied(stored)),
        None => link.send(Update::Error(
            "The keyboard sent an unexpected lighting reply.".into(),
        )),
    };
    Ok(())
}

fn save_keymap(
    kb: &mut Keyboard,
    layer: Layer,
    keys: &[KeyAction],
    link: &Link,
) -> fizz::device::Result<()> {
    let result = match kb.write_keymap(layer, keys) {
        Ok(result) => result,
        Err(fizz::device::Error::Incomplete) => {
            link.send(Update::Error("The key map is incomplete, so it wasn't saved.".into()));
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    match result.stored {
        Some(stored) => {
            link.send(Update::KeymapSaved(layer, stored));
            if !result.verified {
                link.send(Update::Error(
                    "The keyboard saved a different key map than was sent. The page now shows what it stored."
                        .into(),
                ));
            }
        }
        None => {
            link.send(Update::Error(
                "Couldn't read the key map back after saving. Read the keyboard again to check it."
                    .into(),
            ));
        }
    }
    Ok(())
}

fn save_triggers(
    kb: &mut Keyboard,
    groups: &[(KeyTrigger, Vec<usize>)],
    link: &Link,
) -> fizz::device::Result<()> {
    let result = kb.write_key_triggers(groups)?;
    link.send(Update::TriggersSaved(result.stored));
    if !result.verified {
        link.send(Update::Error(
            "Some keys reported different actuation settings after saving. The page now shows what they report."
                .into(),
        ));
    }
    Ok(())
}

fn set_poll_rate(kb: &mut Keyboard, hz: u32, link: &Link) -> fizz::device::Result<()> {
    let stored = match kb.set_poll_rate(hz) {
        Ok(stored) => stored,
        Err(fizz::device::Error::Invalid(_)) => {
            link.send(Update::Error(format!("{hz} Hz isn't a polling rate this keyboard supports.")));
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    link.send(Update::PollRateSet(stored));
    if stored != Some(hz) {
        link.send(Update::Error(format!(
            "The keyboard didn't switch to {hz} Hz. It reports {}.",
            stored.map_or("no rate".to_owned(), |r| format!("{r} Hz"))
        )));
    }
    Ok(())
}

fn save_colours(
    kb: &mut Keyboard,
    layer: u8,
    colours: &[[u8; 3]],
    link: &Link,
) -> fizz::device::Result<()> {
    let result = match kb.write_colours(layer, colours) {
        Ok(r) => r,
        Err(fizz::device::Error::Incomplete) => {
            link.send(Update::Error("The key colours are incomplete, so they weren't saved.".into()));
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    match result.stored {
        Some(stored) => {
            link.send(Update::ColoursSaved(layer, stored));
            if !result.verified {
                link.send(Update::Error(
                    "The keyboard saved different key colours than were sent. The page now shows what it stored."
                        .into(),
                ));
            }
        }
        None => {
            link.send(Update::Error(
                "Couldn't read the key colours back after saving. Read the keyboard again to check them."
                    .into(),
            ));
        }
    }
    Ok(())
}

fn save_socd_pairs(
    kb: &mut Keyboard,
    pairs: &[Option<SocdPair>],
    link: &Link,
) -> fizz::device::Result<()> {
    match kb.write_socd_pairs(pairs) {
        Ok(Some(stored)) => {
            let verified = stored == pairs;
            link.send(Update::SocdPairsSaved(stored));
            if !verified {
                link.send(Update::Error(
                    "The keyboard saved different SOCD pairs than were sent. The page now shows what it stored."
                        .into(),
                ));
            }
        }
        Ok(None) => {
            link.send(Update::Error(
                "Couldn't read the SOCD pairs back after saving. Read the keyboard again to check them."
                    .into(),
            ));
        }
        Err(fizz::device::Error::Incomplete) => {
            link.send(Update::Error("The SOCD list is incomplete, so it wasn't saved.".into()));
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

fn write_profile_file(profile: &Profile, file: &Path) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create the folder: {e}."))?;
    }
    std::fs::write(file, profile.to_json()).map_err(|e| format!("Couldn't save the file: {e}."))
}

fn save_profile(kb: &mut Keyboard, file: &Path, link: &Link) -> fizz::device::Result<()> {
    let profile = profile::read(kb)?;
    let result = write_profile_file(&profile, file).map(|()| file.to_path_buf());
    link.send(Update::ProfileSaved(result));
    Ok(())
}

fn restore_profile(
    kb: &mut Keyboard,
    wanted: &Profile,
    dir: &Path,
    link: &Link,
) -> fizz::device::Result<()> {
    // Never write without a way back: save what the keyboard holds now first.
    let current = profile::read(kb)?;
    let backup = dir.join(format!("before-restore-{}.json", profile::file_stamp(current.saved_at)));
    if let Err(e) = write_profile_file(&current, &backup) {
        link.send(Update::ProfileRestored(Err(format!(
            "{e} Nothing was changed on the keyboard."
        ))));
        return Ok(());
    }
    let outcome = match profile::apply(kb, wanted) {
        Ok(results) => Ok((results, backup)),
        Err(problems) => Err(format!(
            "The file can't be restored: {} Nothing was changed on the keyboard.",
            problems.join(" ")
        )),
    };
    link.send(Update::ProfileRestored(outcome));
    // Show what the keyboard holds now.
    let snapshot = read_snapshot(kb);
    link.send(Update::Snapshot(Box::new(snapshot)));
    Ok(())
}

fn save_macros(kb: &mut Keyboard, macros: &[Option<Macro>], link: &Link) -> fizz::device::Result<()> {
    match kb.write_macros(macros) {
        Ok(Some(stored)) => {
            let verified = stored == macros;
            link.send(Update::MacrosSaved(stored));
            if !verified {
                link.send(Update::Error(
                    "The keyboard saved different macros than were sent. The page now shows what it stored."
                        .into(),
                ));
            }
        }
        Ok(None) => {
            link.send(Update::Error(
                "Couldn't read the macros back after saving. Read the keyboard again to check them."
                    .into(),
            ));
        }
        Err(fizz::device::Error::Incomplete) => {
            link.send(Update::Error("The macro list is incomplete, so it wasn't saved.".into()));
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

fn set_socd_power(kb: &mut Keyboard, on: bool, link: &Link) -> fizz::device::Result<()> {
    let stored = kb.set_socd_power(on)?;
    link.send(Update::SocdPowerSet(stored));
    if stored.map(|p| p.on) != Some(on) {
        link.send(Update::Error(format!(
            "The keyboard didn't turn SOCD {}.",
            if on { "on" } else { "off" }
        )));
    }
    Ok(())
}

fn read_triggers(kb: &mut Keyboard) -> Option<Vec<Option<KeyTrigger>>> {
    let slots: Vec<usize> = fizz::keymap::LAYOUT.iter().map(|k| k.slot).collect();
    let read = kb.read_key_triggers(&slots).ok()?;
    let mut all = vec![None; p::KEY_SLOTS];
    for (slot, t) in slots.into_iter().zip(read) {
        all[slot] = t;
    }
    Some(all)
}

/// Read everything the overview and lighting pages show. Each query is
/// independent: one failing leaves that field empty.
fn read_snapshot(kb: &mut Keyboard) -> Snapshot {
    let status = kb
        .request_same("status", &p::status_request(), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_status(&r));
    let product = kb
        .request_pages("product info", &p::product_info_request(), 1, TIMEOUT, PAGE_IDLE)
        .ok()
        .and_then(|pages| p::parse_product_info(&pages));
    let serial = kb
        .request_same("serial number", &p::serial_request(), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_serial(&r));
    let poll_rate_hz = kb.read_poll_rate().ok().flatten();
    let modes = kb
        .request_same("light modes", &p::light_modes_request(Zone::Keys), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_mode_list(Zone::Keys, &r));
    let light = kb
        .request_same("light config", &p::light_config_request(Zone::Keys), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_light_config(Zone::Keys, &r));
    let keymap_default = kb.read_keymap(Layer::Default).ok().flatten();
    let keymap_fn = kb.read_keymap(Layer::Fn).ok().flatten();
    let high_precision = status.as_ref().is_some_and(|s| s.support.high_precision);
    let trigger_config = kb.read_trigger_config(high_precision).ok().flatten();
    let triggers = read_triggers(kb);
    let colour_layer = kb.read_colour_layer().ok().flatten();
    let colours = colour_layer.and_then(|layer| kb.read_colours(layer).ok().flatten());
    let socd_pairs = kb.read_socd_pairs().ok().flatten();
    let socd_power = kb.read_socd_power().ok().flatten();
    let macros = kb.read_macros().ok().flatten();
    Snapshot {
        status,
        product,
        serial,
        poll_rate_hz,
        modes,
        light,
        keymap_default,
        keymap_fn,
        trigger_config,
        triggers,
        colour_layer,
        colours,
        socd_pairs,
        socd_power,
        macros,
    }
}
