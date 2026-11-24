//! Background thread that owns the keyboard connection, so USB I/O never
//! blocks the UI. It reconnects automatically and sends a status heartbeat to
//! notice when the cable is unplugged.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use fizz::device::{Dir, Ident, Keyboard};
use fizz::protocol::{
    self as p, KeyAction, KeyTrigger, Layer, LightConfig, ModeList, Packet, ProductInfo, Status,
    TriggerConfig, Zone,
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
                Ok(Request::ApplyLight(_) | Request::SaveKeymap(..) | Request::SaveTriggers(_)) => {
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
    kb.write_only(&p::light_config_write(Zone::Keys, cfg))?;
    // Give the firmware a moment, then read back what it actually stored.
    thread::sleep(Duration::from_millis(50));
    let reply = kb.request_same("light config", &p::light_config_request(Zone::Keys), TIMEOUT)?;
    match p::parse_light_config(Zone::Keys, &reply) {
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
    let poll_rate_hz = kb
        .request(
            "polling rate",
            &p::poll_rate_request(),
            |r| r[0] == p::cmd::PROFILE_TRIGGER && r[5] == 0x09,
            TIMEOUT,
        )
        .ok()
        .and_then(|r| p::parse_poll_rate(&r));
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
    }
}
