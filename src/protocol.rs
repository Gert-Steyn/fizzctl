//! Packet builders and reply parsers. No I/O lives here.
//!
//! Every packet is 63 bytes (the HID report ID `0x01` is added by the device
//! layer). Layout, both directions:
//!
//! ```text
//! [0] command  [1] sub-command  [2..=3] page (big-endian)  [4] payload length  [5..] payload
//! ```

pub const PACKET_LEN: usize = 63;
pub const PAYLOAD_OFFSET: usize = 5;
pub const MAX_PAYLOAD: usize = PACKET_LEN - PAYLOAD_OFFSET;

pub type Packet = [u8; PACKET_LEN];

/// Command bytes (byte 0).
pub mod cmd {
    pub const STATUS: u8 = 0x01;
    pub const SIDE_LIGHT_MODES: u8 = 0x02;
    pub const LIGHT: u8 = 0x07;
    pub const SIDE_LIGHT: u8 = 0x08;
    pub const LIGHT_MODES: u8 = 0x0A;
    pub const PRODUCT_INFO: u8 = 0x0D;
    pub const KEYMAP: u8 = 0x18;
    pub const CUSTOM_COLOURS: u8 = 0x09;
    pub const SOCD: u8 = 0x24;
    pub const MACRO: u8 = 0x19;
    pub const SERIAL: u8 = 0x15;
    pub const PROFILE_TRIGGER: u8 = 0x21;

    /// Commands this app must never send:
    /// - `0x0B` enters the firmware bootloader;
    /// - `0x14 02` is built identically for "factory reset" and "read sleep
    ///   timer" in the vendor app;
    /// - `0x21 .. 18 02` starts the live travel test, which made the keyboard
    ///   drop off USB and reset on 2026-11-24 (firmware 3.17.01). Unblock only
    ///   for a deliberate, agreed retry.
    pub const fn is_blocked(packet: &super::Packet) -> bool {
        packet[0] == 0x0B
            || (packet[0] == 0x14 && packet[1] == 0x02)
            || (packet[0] == PROFILE_TRIGGER && packet[4] == 0x18 && packet[5] == 0x02)
    }
}

fn packet(command: u8) -> Packet {
    let mut p = [0u8; PACKET_LEN];
    p[0] = command;
    p
}

/// Page index carried in bytes 2..=3.
pub fn page(reply: &Packet) -> u16 {
    u16::from_be_bytes([reply[2], reply[3]])
}

/// The payload of a reply, clamped to the packet.
pub fn payload(reply: &Packet) -> &[u8] {
    let len = (reply[4] as usize).min(MAX_PAYLOAD);
    &reply[PAYLOAD_OFFSET..PAYLOAD_OFFSET + len]
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

pub fn status_request() -> Packet {
    packet(cmd::STATUS)
}

pub fn product_info_request() -> Packet {
    packet(cmd::PRODUCT_INFO)
}

pub fn serial_request() -> Packet {
    packet(cmd::SERIAL)
}

pub fn poll_rate_request() -> Packet {
    let mut p = packet(cmd::PROFILE_TRIGGER);
    p[4] = 0x01;
    p[5] = 0x09;
    p
}

/// Which lighting zone a lighting command targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    /// The per-key backlight.
    Keys,
    /// The side light strip.
    Side,
}

impl Zone {
    fn config_cmd(self) -> u8 {
        match self {
            Zone::Keys => cmd::LIGHT,
            Zone::Side => cmd::SIDE_LIGHT,
        }
    }

    fn modes_cmd(self) -> u8 {
        match self {
            Zone::Keys => cmd::LIGHT_MODES,
            Zone::Side => cmd::SIDE_LIGHT_MODES,
        }
    }
}

pub fn light_modes_request(zone: Zone) -> Packet {
    packet(zone.modes_cmd())
}

pub fn light_config_request(zone: Zone) -> Packet {
    let mut p = packet(zone.config_cmd());
    p[1] = 0x01;
    p
}

pub fn light_config_write(zone: Zone, cfg: &LightConfig) -> Packet {
    let mut p = packet(zone.config_cmd());
    p[4] = 0x0E;
    p[5] = cfg.mode;
    p[6] = cfg.brightness;
    p[7] = cfg.speed;
    p[8..11].copy_from_slice(&cfg.foreground);
    p[11..14].copy_from_slice(&cfg.background);
    p[14] = cfg.direction;
    p[15] = cfg.full_color as u8;
    p[16] = cfg.power;
    p[17] = cfg.timer;
    p
}

// ---------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------

/// Feature flags from the status reply. Names follow the vendor app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Support {
    pub raw: Vec<u8>,
    pub lock_keys: bool,
    pub any_key_calibration: bool,
    pub custom_light: bool,
    pub poll_rate: bool,
    pub dead_band: bool,
    pub music_rhythm: bool,
    pub high_precision: bool,
    pub gamepad: bool,
    pub rapid_trigger: bool,
    pub dks: bool,
    pub calibrated: bool,
    pub six_key_mode: bool,
    pub storing: bool,
}

impl Support {
    /// Human-readable names of the supported features, for display.
    pub fn features(&self) -> Vec<&'static str> {
        let all = [
            (self.custom_light, "Per-key RGB"),
            (self.poll_rate, "Polling rate"),
            (self.dead_band, "Dead zones"),
            (self.rapid_trigger, "Rapid trigger"),
            (self.dks, "Dynamic keystrokes"),
            (self.music_rhythm, "Music lighting"),
            (self.high_precision, "High precision"),
            (self.gamepad, "Gamepad mode"),
            (self.lock_keys, "Key locks"),
            (self.any_key_calibration, "Key calibration"),
        ];
        all.into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub mode: u8,
    pub support: Support,
}

pub fn parse_status(reply: &Packet) -> Option<Status> {
    if reply[0] != cmd::STATUS {
        return None;
    }
    let s = &reply[5..0x17];
    let bit = |byte: u8, mask: u8| byte & mask != 0;
    Some(Status {
        mode: reply[1],
        support: Support {
            raw: s.to_vec(),
            lock_keys: s[0] == 1,
            any_key_calibration: bit(s[12], 0x01),
            custom_light: bit(s[12], 0x02),
            poll_rate: bit(s[12], 0x08),
            dead_band: bit(s[12], 0x10),
            music_rhythm: bit(s[12], 0x20),
            high_precision: bit(s[13], 0x01),
            gamepad: bit(s[13], 0x04),
            rapid_trigger: bit(s[13], 0x10),
            dks: bit(s[13], 0x20),
            calibrated: s[14] == 1,
            six_key_mode: bit(s[17], 0x01),
            storing: bit(s[17], 0x02),
        },
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductInfo {
    /// The raw comma-separated string the keyboard reports.
    pub raw: String,
    pub fields: Vec<String>,
    pub build_date: Option<String>,
}

impl ProductInfo {
    pub fn product(&self) -> Option<&str> {
        self.fields.get(4).map(String::as_str).filter(|s| !s.is_empty())
    }

    /// Firmware version, cleaned the same way the vendor app does it
    /// (`V1_02` → `1.02`).
    pub fn firmware(&self) -> Option<String> {
        let v = self.fields.get(5)?;
        let v: String = v
            .chars()
            .filter(|c| !matches!(*c as u32, 0x00..=0x08 | 0x0E..=0x1F))
            .filter(|c| *c != 'V')
            .map(|c| if c == '_' { '.' } else { c })
            .collect();
        Some(v.trim().to_owned()).filter(|s| !s.is_empty())
    }
}

/// Payload as text, up to the first NUL (the device leaves stale bytes after it).
fn payload_text(reply: &Packet) -> String {
    let p = payload(reply);
    let end = p.iter().position(|b| *b == 0).unwrap_or(p.len());
    String::from_utf8_lossy(&p[..end]).trim().to_owned()
}

/// Parse product info from its pages: page 0 is the identity string, page 1
/// (when present) is the firmware build date, e.g. `Oct 22 2025,16:24:10`.
pub fn parse_product_info(pages: &[Packet]) -> Option<ProductInfo> {
    let first = pages.iter().find(|r| r[0] == cmd::PRODUCT_INFO && page(r) == 0)?;
    let raw = payload_text(first);
    let fields = raw.split(',').map(|f| f.trim().to_owned()).collect();
    let build_date = pages
        .iter()
        .find(|r| r[0] == cmd::PRODUCT_INFO && page(r) == 1)
        .map(payload_text)
        .map(|t| t.replace(',', " "))
        .filter(|t| !t.is_empty());
    Some(ProductInfo {
        raw,
        fields,
        build_date,
    })
}

pub fn parse_serial(reply: &Packet) -> Option<String> {
    if reply[0] != cmd::SERIAL {
        return None;
    }
    let len = match reply[4] {
        0 => 0x20,
        n => n as usize,
    }
    .min(MAX_PAYLOAD);
    let text = String::from_utf8_lossy(&reply[PAYLOAD_OFFSET..PAYLOAD_OFFSET + len])
        .trim_end_matches('\0')
        .trim()
        .to_owned();
    Some(text).filter(|s| !s.is_empty())
}

/// Polling rate in Hz, from the reply to [`poll_rate_request`].
/// Polling rates the keyboard accepts, in Hz.
pub const POLL_RATES_HZ: [u32; 4] = [1000, 2000, 4000, 8000];

/// Set the polling rate. The keyboard doesn't reply; read it back with
/// [`poll_rate_request`]. Returns `None` for an unsupported rate.
pub fn poll_rate_write(hz: u32) -> Option<Packet> {
    let code = match hz {
        1000 => 1,
        2000 => 2,
        4000 => 4,
        8000 => 8,
        _ => return None,
    };
    let mut p = packet(cmd::PROFILE_TRIGGER);
    p[5] = 0x09;
    p[6] = 0x01;
    p[7] = code;
    Some(p)
}

pub fn parse_poll_rate(reply: &Packet) -> Option<u32> {
    if reply[0] != cmd::PROFILE_TRIGGER || reply[5] != 0x09 {
        return None;
    }
    Some(match reply[6] {
        1 => 1000,
        2 => 2000,
        4 => 4000,
        8 => 8000,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeList {
    pub modes: Vec<u8>,
    pub max_brightness: u8,
    pub max_speed: u8,
}

pub fn parse_mode_list(zone: Zone, reply: &Packet) -> Option<ModeList> {
    if reply[0] != zone.modes_cmd() {
        return None;
    }
    let p = payload(reply);
    if p.len() < 4 {
        return None;
    }
    Some(ModeList {
        modes: p[..p.len() - 4].to_vec(),
        max_brightness: p[p.len() - 1],
        max_speed: p[p.len() - 2],
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LightConfig {
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    /// 0–5; see [`direction_name`].
    pub direction: u8,
    /// Rainbow colours instead of the foreground colour.
    pub full_color: bool,
    /// Passed through unchanged: meaning not yet confirmed.
    pub power: u8,
    /// Passed through unchanged: meaning not yet confirmed.
    pub timer: u8,
}

pub fn parse_light_config(zone: Zone, reply: &Packet) -> Option<LightConfig> {
    if reply[0] != zone.config_cmd() {
        return None;
    }
    let p = payload(reply);
    if p.len() < 12 {
        return None;
    }
    let mut direction = p[9];
    if direction >= 6 {
        direction = 0;
    }
    Some(LightConfig {
        mode: p[0],
        brightness: p[1],
        speed: p[2],
        foreground: [p[3], p[4], p[5]],
        background: [p[6], p[7], p[8]],
        direction,
        full_color: p[10] != 0,
        power: p[11],
        timer: p.get(12).copied().unwrap_or(0),
    })
}

/// Effect names, indexed by mode number. Taken from the order of the vendor
/// app's preview renderer; index 10 (Custom) matches the mode the app adds
/// for per-key lighting, which confirms the order.
pub fn mode_name(mode: u8) -> String {
    const NAMES: [&str; 17] = [
        "Static",
        "Breathing",
        "Wave",
        "Neon",
        "Radar",
        "Rolling",
        "Reactive",
        "Laser",
        "Ripple",
        "Twinkle",
        "Custom",
        "Cross",
        "Action response",
        "",
        "Auto ripple",
        "Striation",
        "Drip",
    ];
    match mode {
        100 => "Music".to_owned(),
        m => NAMES
            .get(m as usize)
            .filter(|n| !n.is_empty())
            .map(|n| (*n).to_owned())
            .unwrap_or_else(|| format!("Mode {m}")),
    }
}

/// Direction names, inferred from the vendor app's wave preview.
pub fn direction_name(direction: u8) -> &'static str {
    match direction {
        0 => "Horizontal",
        1 => "Horizontal, reversed",
        2 => "Vertical",
        3 => "Vertical, reversed",
        4 => "From centre",
        5 => "To centre",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------
// Key mapping (command 0x18)
// ---------------------------------------------------------------------------

/// Key slots in a layer: 6 rows × 22 columns, slot = `22 * row + col`.
pub const KEY_SLOTS: usize = 132;
const KEY_BYTES: usize = 4;
/// Keys per page when reading (56 bytes of payload).
const KEYS_PER_PAGE: usize = 14;
const WRITE_PAGE_BYTES: usize = KEYS_PER_PAGE * KEY_BYTES;
/// Last page index of a layer read: 10 pages × 14 keys ≥ 132.
pub const KEYMAP_LAST_PAGE: u16 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Default,
    Fn,
}

impl Layer {
    fn read_sub(self) -> u8 {
        match self {
            Layer::Default => 0x80,
            Layer::Fn => 0x82,
        }
    }

    pub fn write_sub(self) -> u8 {
        match self {
            Layer::Default => 0x00,
            Layer::Fn => 0x02,
        }
    }
}

/// What one key slot does: the four raw bytes the keyboard stores.
///
/// `kind` 0 is an ordinary key with `code` = HID usage. Other kinds (media,
/// mouse, shortcut, lighting…) are listed in `keymap.rs`. Unknown values are
/// kept as-is so writing a layer back never changes keys we didn't touch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct KeyAction {
    pub kind: u8,
    pub code: u8,
    pub arg1: u8,
    pub arg2: u8,
}

impl KeyAction {
    pub const fn new(kind: u8, code: u8, arg1: u8, arg2: u8) -> Self {
        Self {
            kind,
            code,
            arg1,
            arg2,
        }
    }

    pub const fn key(hid: u8) -> Self {
        Self::new(0, hid, 0, 0)
    }
}

pub fn keymap_request(layer: Layer) -> Packet {
    let mut p = packet(cmd::KEYMAP);
    p[1] = layer.read_sub();
    p
}

/// Assemble a layer from its read pages. Missing pages leave `None`, so a
/// partial read can be detected and never written back.
pub fn parse_keymap(layer: Layer, pages: &[Packet]) -> Option<Vec<KeyAction>> {
    let mut slots: Vec<Option<KeyAction>> = vec![None; KEY_SLOTS];
    for reply in pages {
        if reply[0] != cmd::KEYMAP || reply[1] != layer.read_sub() {
            continue;
        }
        let first = page(reply) as usize * KEYS_PER_PAGE;
        for (i, k) in payload(reply).as_chunks::<KEY_BYTES>().0.iter().enumerate() {
            if let Some(slot) = slots.get_mut(first + i) {
                *slot = Some(KeyAction::new(k[0], k[1], k[2], k[3]));
            }
        }
    }
    slots.into_iter().collect()
}

/// Packets that write a whole layer. The keyboard acknowledges each one.
pub fn keymap_write(layer: Layer, keys: &[KeyAction]) -> Vec<Packet> {
    let mut bytes = vec![0u8; KEY_SLOTS * KEY_BYTES];
    for (slot, k) in keys.iter().take(KEY_SLOTS).enumerate() {
        bytes[slot * KEY_BYTES..][..KEY_BYTES].copy_from_slice(&[k.kind, k.code, k.arg1, k.arg2]);
    }
    bytes
        .chunks(WRITE_PAGE_BYTES)
        .enumerate()
        .map(|(n, chunk)| {
            let mut p = packet(cmd::KEYMAP);
            p[1] = layer.write_sub();
            p[2..4].copy_from_slice(&(n as u16).to_be_bytes());
            p[4] = chunk.len() as u8;
            p[PAYLOAD_OFFSET..PAYLOAD_OFFSET + chunk.len()].copy_from_slice(chunk);
            p
        })
        .collect()
}

/// True when a status reply says the keyboard is still saving to flash.
pub fn is_storing(status_reply: &Packet) -> bool {
    status_reply[0] == cmd::STATUS && status_reply[0x16] & 0x02 != 0
}

// ---------------------------------------------------------------------------
// Actuation and rapid trigger (command 0x21, byte 4 = 0x18)
// ---------------------------------------------------------------------------

const TRIGGER_GROUP: u8 = 0x18;
pub const TRIGGER_COLS: usize = 22;

/// Trigger modes stored per key.
pub mod trigger_mode {
    /// Fixed actuation point.
    pub const NORMAL: u8 = 0x00;
    /// Older rapid-trigger mode; treated like `RT_SINGLE`.
    pub const RT_LEGACY: u8 = 0x01;
    /// Rapid trigger, one sensitivity for press and release.
    pub const RT_SINGLE: u8 = 0x0C;
    /// Rapid trigger, separate press and release sensitivities.
    pub const RT_SPLIT: u8 = 0x0D;

    pub const fn is_rapid(mode: u8) -> bool {
        matches!(mode, RT_LEGACY | RT_SINGLE | RT_SPLIT)
    }
}

fn trigger_packet(sub: u8) -> Packet {
    let mut p = packet(cmd::PROFILE_TRIGGER);
    p[4] = TRIGGER_GROUP;
    p[5] = sub;
    p
}

pub fn trigger_config_request() -> Packet {
    trigger_packet(0x04)
}

/// Per-key trigger settings for the key at `slot` (`22 * row + col`).
pub fn key_trigger_request(slot: usize) -> Packet {
    let mut p = trigger_packet(0x05);
    p[6] = (slot / TRIGGER_COLS) as u8;
    p[7] = (slot % TRIGGER_COLS) as u8;
    p
}

/// Travel limits and step size, from the reply to [`trigger_config_request`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriggerConfig {
    /// Millimetres per step. Every travel value on the wire is in steps.
    pub unit_mm: f32,
    /// Shallowest actuation point, in steps.
    pub min_steps: u16,
    /// Deepest actuation point, in steps.
    pub max_steps: u16,
    /// Millimetres per rapid-trigger sensitivity step.
    pub rt_unit_mm: f32,
}

impl TriggerConfig {
    pub fn mm(&self, steps: u16) -> f32 {
        steps as f32 * self.unit_mm
    }

    pub fn steps(&self, mm: f32) -> u16 {
        (mm / self.unit_mm).round().max(0.0) as u16
    }

    /// Largest rapid-trigger sensitivity the vendor app offers: half the full travel.
    pub fn rt_max_steps(&self) -> u16 {
        self.max_steps.div_ceil(2).max(1)
    }
}

/// `high_precision` comes from the status support flags: it switches the
/// unit divisor from 100 to 1000.
pub fn parse_trigger_config(reply: &Packet, high_precision: bool) -> Option<TriggerConfig> {
    if reply[0] != cmd::PROFILE_TRIGGER || reply[5] != 0x04 {
        return None;
    }
    let divisor = if high_precision { 1000.0 } else { 100.0 };
    let unit_raw = match reply[7] {
        0 => 10,
        n => n,
    };
    let rt_unit_raw = match reply[8] {
        0 => 10,
        n => n,
    };
    let unit_mm = unit_raw as f32 / divisor;
    let max_steps = u16::from_le_bytes([reply[6], reply[9]]);
    let min_steps = match reply[10] {
        // Fallbacks the vendor app uses when the keyboard reports 0.
        0 if (unit_mm - 0.04).abs() < 1e-6 => 3,
        0 => 1,
        n => n as u16,
    };
    Some(TriggerConfig {
        unit_mm,
        min_steps,
        max_steps,
        rt_unit_mm: rt_unit_raw as f32 / divisor,
    })
}

/// One key's actuation settings. Travel values are in [`TriggerConfig`] steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct KeyTrigger {
    pub mode: u8,
    /// Actuation point.
    pub travel: u16,
    /// Rapid trigger: how far the key must move down again to re-press.
    pub press: u16,
    /// Rapid trigger: how far the key must rise to release.
    pub release: u16,
}

pub fn parse_key_trigger(reply: &Packet) -> Option<KeyTrigger> {
    if reply[0] != cmd::PROFILE_TRIGGER || reply[5] != 0x05 {
        return None;
    }
    Some(KeyTrigger {
        mode: reply[6],
        travel: u16::from_le_bytes([reply[7], reply[11]]),
        press: u16::from_le_bytes([reply[9], reply[13]]),
        release: u16::from_le_bytes([reply[10], reply[14]]),
    })
}

/// Set the same trigger settings on every key in `slots`. One packet covers
/// any number of keys: a bitmap with one byte per column, one bit per row.
pub fn key_trigger_write(trigger: &KeyTrigger, slots: &[usize]) -> Packet {
    let mut p = packet(cmd::PROFILE_TRIGGER);
    p[4] = TRIGGER_GROUP;
    p[5] = trigger.mode;
    for &slot in slots {
        let (row, col) = (slot / TRIGGER_COLS, slot % TRIGGER_COLS);
        if row < 8 {
            p[6 + col] |= 1 << row;
        }
    }
    let [t_lo, t_hi] = trigger.travel.to_le_bytes();
    let [p_lo, p_hi] = trigger.press.to_le_bytes();
    let [r_lo, r_hi] = trigger.release.to_le_bytes();
    p[28..36].copy_from_slice(&[t_lo, t_lo, p_lo, r_lo, t_hi, t_hi, p_hi, r_hi]);
    p
}

// ---------------------------------------------------------------------------
// Per-key colours (command 0x09)
// ---------------------------------------------------------------------------

/// Keys per colour page: 54 bytes = 18 × RGB.
const COLOURS_PER_PAGE: usize = 18;
/// Last page of a colour layer read: 8 pages × 18 ≥ 132 slots.
pub const COLOUR_LAST_PAGE: u16 = 7;

/// Which of the two stored per-key colour layouts the "Custom" effect shows
/// (the vendor app calls them colour 1 and colour 2).
pub fn colour_layer_request() -> Packet {
    let mut p = packet(cmd::CUSTOM_COLOURS);
    p[1] = 0x20;
    p
}

pub fn parse_colour_layer(reply: &Packet) -> Option<u8> {
    (reply[0] == cmd::CUSTOM_COLOURS && reply[1] <= 1).then_some(reply[1])
}

/// Read every key's colour in `layer` (0 or 1); 8 pages come back.
pub fn colours_request(layer: u8) -> Packet {
    let mut p = packet(cmd::CUSTOM_COLOURS);
    p[1] = 0x80 | (layer & 1);
    p
}

/// Assemble a colour layer from its pages. `None` if a page is missing.
pub fn parse_colours(layer: u8, pages: &[Packet]) -> Option<Vec<[u8; 3]>> {
    let mut slots: Vec<Option<[u8; 3]>> = vec![None; KEY_SLOTS];
    for reply in pages {
        if reply[0] != cmd::CUSTOM_COLOURS || reply[1] != 0x80 | (layer & 1) {
            continue;
        }
        let first = page(reply) as usize * COLOURS_PER_PAGE;
        for (i, rgb) in payload(reply).as_chunks::<3>().0.iter().enumerate() {
            if let Some(slot) = slots.get_mut(first + i) {
                *slot = Some(*rgb);
            }
        }
    }
    slots.into_iter().collect()
}

/// Packets writing a whole colour layer. The keyboard acks each with `[09, layer]`.
pub fn colours_write(layer: u8, colours: &[[u8; 3]]) -> Vec<Packet> {
    let bytes: Vec<u8> = colours.iter().take(KEY_SLOTS).flatten().copied().collect();
    bytes
        .chunks(COLOURS_PER_PAGE * 3)
        .enumerate()
        .map(|(n, chunk)| {
            let mut p = packet(cmd::CUSTOM_COLOURS);
            p[1] = layer & 1;
            p[2..4].copy_from_slice(&(n as u16).to_be_bytes());
            p[4] = chunk.len() as u8;
            p[PAYLOAD_OFFSET..PAYLOAD_OFFSET + chunk.len()].copy_from_slice(chunk);
            p
        })
        .collect()
}

/// The lighting mode number of the "Custom" (per-key colour) effect.
pub const CUSTOM_LIGHT_MODE: u8 = 10;

// ---------------------------------------------------------------------------
// SOCD / snap tap (command 0x24; the vendor code calls it "PRCS")
// ---------------------------------------------------------------------------

/// Number of SOCD pair slots.
pub const SOCD_SLOTS: usize = 20;
const SOCD_PER_PAGE: usize = 10;
pub const SOCD_LAST_PAGE: u16 = 1;

/// How a SOCD pair resolves both keys being held.
pub mod socd_mode {
    /// The key pressed first is released: last input wins.
    pub const LAST_INPUT: u8 = 0;
    /// Key 1 always wins over key 2.
    pub const KEY1_PRIORITY: u8 = 1;
    /// Key 2 always wins over key 1.
    pub const KEY2_PRIORITY: u8 = 2;
    /// The later key doesn't fire, it only releases the first: neutral.
    pub const NEUTRAL: u8 = 3;

    pub const ALL: [u8; 4] = [LAST_INPUT, KEY1_PRIORITY, KEY2_PRIORITY, NEUTRAL];

    pub fn name(mode: u8) -> &'static str {
        match mode {
            LAST_INPUT => "Last input wins",
            KEY1_PRIORITY => "First key has priority",
            KEY2_PRIORITY => "Second key has priority",
            NEUTRAL => "Neutral (both cancel)",
            _ => "Unknown mode",
        }
    }
}

/// One SOCD pair: two keys (HID usage codes) and how they resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SocdPair {
    /// Stored as 1 for an active pair by the vendor app.
    pub state: u8,
    pub mode: u8,
    pub key1: u8,
    pub key2: u8,
}

pub fn socd_pairs_request() -> Packet {
    let mut p = packet(cmd::SOCD);
    p[1] = 0x01;
    p
}

/// The 20 pair slots; `None` for an empty slot. `None` overall if a page is missing.
pub fn parse_socd_pairs(pages: &[Packet]) -> Option<Vec<Option<SocdPair>>> {
    let mut slots: Vec<Option<Option<SocdPair>>> = vec![None; SOCD_SLOTS];
    for reply in pages {
        if reply[0] != cmd::SOCD || reply[1] != 0x01 {
            continue;
        }
        let first = page(reply) as usize * SOCD_PER_PAGE;
        let body = &reply[PAYLOAD_OFFSET..PAYLOAD_OFFSET + SOCD_PER_PAGE * 4];
        for (i, e) in body.as_chunks::<4>().0.iter().enumerate() {
            let Some(slot) = slots.get_mut(first + i) else { continue };
            // State 0 is empty; 07 FF FF FF is erased flash.
            let empty = e[0] == 0 || *e == [0x07, 0xFF, 0xFF, 0xFF];
            *slot = Some((!empty).then_some(SocdPair {
                state: e[0],
                mode: e[1],
                key1: e[2],
                key2: e[3],
            }));
        }
    }
    slots.into_iter().collect()
}

/// Packets writing all 20 slots. The keyboard acks each with `[24, 00]`.
pub fn socd_pairs_write(slots: &[Option<SocdPair>]) -> Vec<Packet> {
    (0..SOCD_SLOTS / SOCD_PER_PAGE)
        .map(|n| {
            let mut p = packet(cmd::SOCD);
            p[2..4].copy_from_slice(&(n as u16).to_be_bytes());
            p[4] = (SOCD_PER_PAGE * 4) as u8;
            for i in 0..SOCD_PER_PAGE {
                if let Some(Some(pair)) = slots.get(n * SOCD_PER_PAGE + i) {
                    let at = PAYLOAD_OFFSET + i * 4;
                    p[at..at + 4].copy_from_slice(&[pair.state, pair.mode, pair.key1, pair.key2]);
                }
            }
            p
        })
        .collect()
}

/// Whether SOCD is on, plus the two codes the keyboard reports with it
/// (on this keyboard `FA E5`, which look like an Fn + Right Shift toggle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocdPower {
    pub on: bool,
    pub code1: u8,
    pub code2: u8,
}

pub fn socd_power_request() -> Packet {
    let mut p = packet(cmd::SOCD);
    p[1] = 0x02;
    p
}

pub fn parse_socd_power(reply: &Packet) -> Option<SocdPower> {
    if reply[0] != cmd::SOCD || reply[1] != 0x02 {
        return None;
    }
    let p = payload(reply);
    Some(SocdPower {
        on: p.first() == Some(&1),
        code1: p.get(4).copied().unwrap_or(0),
        code2: p.get(5).copied().unwrap_or(0),
    })
}

/// Turn SOCD on or off. No reply; read it back with [`socd_power_request`].
pub fn socd_power_write(on: bool) -> Packet {
    let mut p = packet(cmd::SOCD);
    p[1] = 0x03;
    p[4] = 0x01;
    p[5] = on as u8;
    p
}

// ---------------------------------------------------------------------------
// Macros (command 0x19)
// ---------------------------------------------------------------------------

pub const MACRO_SLOTS: usize = 10;
/// Bytes stored per macro slot.
pub const MACRO_BYTES: usize = 256;
const MACRO_PAGE_BYTES: usize = 58;
pub const MACRO_LAST_PAGE: u16 = 4;
const MACRO_HEADER: usize = 8;
/// The vendor app's limit: 20 key presses, each a press and a release.
pub const MACRO_MAX_STEPS: usize = 40;

/// How a macro plays when its key is pressed.
pub mod macro_play {
    pub const ONCE: u8 = 0;
    /// Play `count` times (1–99).
    pub const TIMES: u8 = 1;
    /// Press to start, press again to stop.
    pub const TOGGLE: u8 = 2;
    /// Repeat while the key is held.
    pub const WHILE_HELD: u8 = 3;

    pub const ALL: [u8; 4] = [ONCE, TIMES, TOGGLE, WHILE_HELD];

    pub fn name(play: u8) -> &'static str {
        match play {
            ONCE => "Play once",
            TIMES => "Play a number of times",
            TOGGLE => "Press to start, press again to stop",
            WHILE_HELD => "Repeat while held",
            _ => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MacroStep {
    /// Key down (true) or key up.
    pub press: bool,
    /// A mouse button (`code` is a button bit: 1 left, 2 right, 4 middle,
    /// 8 forward, 16 back) instead of a key (`code` is a HID usage).
    pub mouse: bool,
    pub code: u8,
    /// Wait after this step before the next one, in milliseconds.
    pub delay_ms: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Macro {
    pub play: u8,
    /// Repeat count, used when `play` is [`macro_play::TIMES`].
    pub count: u16,
    pub steps: Vec<MacroStep>,
}

pub fn macro_request(slot: usize) -> Packet {
    let mut p = packet(cmd::MACRO);
    p[1] = 0x80 + slot as u8;
    p
}

/// Reassemble a slot's 256 raw bytes from its pages. Handles both layouts
/// the vendor app accepts: flat (pages are consecutive 58-byte chunks) and
/// "paged" (every page repeats the 8-byte header before its steps).
pub fn macro_raw(slot: usize, pages: &[Packet]) -> Option<[u8; MACRO_BYTES]> {
    let mut flat = [0u8; MACRO_BYTES];
    let first = pages
        .iter()
        .find(|r| r[0] == cmd::MACRO && r[1] == 0x80 + slot as u8 && page(r) == 0)?;
    let take = (first[4] as usize).min(MACRO_PAGE_BYTES);
    flat[..take].copy_from_slice(&first[PAYLOAD_OFFSET..PAYLOAD_OFFSET + take]);
    let header = flat;
    let mut paged = None;
    for r in pages {
        let n = page(r) as usize;
        if r[0] != cmd::MACRO || r[1] != 0x80 + slot as u8 || n == 0 || n > MACRO_LAST_PAGE as usize {
            continue;
        }
        let len = (r[4] as usize).min(MACRO_PAGE_BYTES);
        let body = &r[PAYLOAD_OFFSET..PAYLOAD_OFFSET + len];
        let is_paged = *paged.get_or_insert(
            len >= MACRO_HEADER
                && body[0] & 0x7F == header[0] & 0x7F
                && body[1] == header[1]
                && body[3] == header[3],
        );
        if is_paged {
            // Steps after a repeated header, 50 bytes of steps per page.
            let at = MACRO_HEADER + 50 * n;
            let data = &body[MACRO_HEADER.min(len)..];
            if at + data.len() <= MACRO_BYTES {
                flat[at..at + data.len()].copy_from_slice(data);
            }
        } else {
            let at = MACRO_PAGE_BYTES * n;
            let end = (at + len).min(MACRO_BYTES);
            flat[at..end].copy_from_slice(&body[..end - at]);
        }
    }
    Some(flat)
}

/// Decode a slot's raw bytes. `None` for an empty slot (erased `FF`s or zeros).
pub fn parse_macro(raw: &[u8; MACRO_BYTES]) -> Option<Macro> {
    let step_bytes = raw[3] as usize;
    if !(4..=MACRO_BYTES - MACRO_HEADER).contains(&step_bytes) {
        return None;
    }
    let steps = raw[MACRO_HEADER..MACRO_HEADER + step_bytes]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|s| MacroStep {
            press: s[1] >> 4 == 1,
            mouse: s[1] & 0x0F == 1,
            code: s[0],
            delay_ms: u16::from_be_bytes([s[2], s[3]]),
        })
        .collect();
    Some(Macro {
        play: raw[1],
        count: u16::from_be_bytes([raw[4], raw[5]]).max(1),
        steps,
    })
}

/// The 256 bytes for a slot; `None` writes an empty (all-zero) slot.
pub fn macro_bytes(slot: usize, m: Option<&Macro>) -> [u8; MACRO_BYTES] {
    let mut raw = [0u8; MACRO_BYTES];
    let Some(m) = m else { return raw };
    let steps = &m.steps[..m.steps.len().min((MACRO_BYTES - MACRO_HEADER) / 4)];
    raw[0] = slot as u8 & 0x7F;
    raw[1] = m.play;
    raw[3] = (steps.len() * 4) as u8;
    let count = if m.play == macro_play::TIMES { m.count.max(1) } else { 1 };
    raw[4..6].copy_from_slice(&count.to_be_bytes());
    for (i, s) in steps.iter().enumerate() {
        let at = MACRO_HEADER + i * 4;
        let [hi, lo] = s.delay_ms.to_be_bytes();
        raw[at..at + 4].copy_from_slice(&[s.code, (s.press as u8) << 4 | s.mouse as u8, hi, lo]);
    }
    raw
}

/// The vendor app's "delete macro" command for a macro index.
pub fn macro_delete(index: usize) -> Packet {
    let mut p = packet(cmd::MACRO);
    p[1] = 0x03;
    p[4] = index as u8;
    p
}

/// Packets writing a slot's raw bytes (flat layout, 5 pages). The keyboard
/// confirms each page with `[19, slot]`.
pub fn macro_write(slot: usize, raw: &[u8; MACRO_BYTES]) -> Vec<Packet> {
    raw.chunks(MACRO_PAGE_BYTES)
        .enumerate()
        .map(|(n, chunk)| {
            let mut p = packet(cmd::MACRO);
            p[1] = slot as u8;
            p[2..4].copy_from_slice(&(n as u16).to_be_bytes());
            p[4] = chunk.len() as u8;
            p[PAYLOAD_OFFSET..PAYLOAD_OFFSET + chunk.len()].copy_from_slice(chunk);
            p
        })
        .collect()
}

/// Start the live travel test for `slots`: the keyboard then streams
/// [`parse_travel`] packets while keys move. Stop it with [`travel_test_stop`].
pub fn travel_test_start(slots: &[usize]) -> Packet {
    let mut p = trigger_packet(0x02);
    for &slot in slots {
        let (row, col) = (slot / TRIGGER_COLS, slot % TRIGGER_COLS);
        if row < 8 {
            p[6 + col] |= 1 << row;
        }
    }
    p
}

pub fn travel_test_stop() -> Packet {
    trigger_packet(0x03)
}

/// A pushed travel report during the live test: `(slot, travel in steps)`.
pub fn parse_travel(packet: &Packet) -> Option<(usize, u16)> {
    if packet[0] != cmd::PROFILE_TRIGGER || packet[5] != 0x01 {
        return None;
    }
    let (row, col) = (packet[6] as usize, packet[7] as usize);
    if col >= TRIGGER_COLS {
        return None;
    }
    Some((row * TRIGGER_COLS + col, u16::from_le_bytes([packet[8], packet[9]])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(bytes: &[u8]) -> Packet {
        let mut p = [0u8; PACKET_LEN];
        p[..bytes.len()].copy_from_slice(bytes);
        p
    }

    #[test]
    fn light_config_round_trips() {
        let cfg = LightConfig {
            mode: 2,
            brightness: 4,
            speed: 3,
            foreground: [255, 0, 10],
            background: [1, 2, 3],
            direction: 4,
            full_color: true,
            power: 0,
            timer: 7,
        };
        let mut written = light_config_write(Zone::Keys, &cfg);
        assert_eq!(&written[..5], &[0x07, 0, 0, 0, 0x0E]);
        // The device answers reads with the same payload layout.
        written[1] = 0x01;
        assert_eq!(parse_light_config(Zone::Keys, &written), Some(cfg));
    }

    #[test]
    fn mode_list_splits_trailer() {
        let r = reply(&[0x0A, 0, 0, 0, 7, 0, 1, 2, 0, 0, 5, 4]);
        let list = parse_mode_list(Zone::Keys, &r).unwrap();
        assert_eq!(list.modes, vec![0, 1, 2]);
        assert_eq!(list.max_speed, 5);
        assert_eq!(list.max_brightness, 4);
    }

    #[test]
    fn product_firmware_is_cleaned() {
        // Captured from a real keyboard (firmware 3.17.01).
        let text = b"W669,34,KB,FR,7153USHEXYXACPARGB,V3.17.01";
        let mut bytes = vec![0x0D, 0, 0, 0, text.len() as u8];
        bytes.extend_from_slice(text);
        let date = b"Oct 22 2025,16:24:10\0\0\0\0Oct ";
        let mut bytes2 = vec![0x0D, 0, 0, 1, date.len() as u8];
        bytes2.extend_from_slice(date);
        let info = parse_product_info(&[reply(&bytes), reply(&bytes2)]).unwrap();
        assert_eq!(info.product(), Some("7153USHEXYXACPARGB"));
        assert_eq!(info.firmware().as_deref(), Some("3.17.01"));
        assert_eq!(info.build_date.as_deref(), Some("Oct 22 2025 16:24:10"));
    }

    #[test]
    fn keymap_write_round_trips_through_read_format() {
        let mut keys = vec![KeyAction::default(); KEY_SLOTS];
        keys[22] = KeyAction::key(0x29);
        keys[120] = KeyAction::new(1, 0xFA, 0, 0);
        keys[131] = KeyAction::new(0x12, 0x01, 0xFF, 0);
        let packets = keymap_write(Layer::Default, &keys);
        // 132 keys × 4 bytes = 9 full pages of 56 bytes + 24, as the keyboard sends.
        assert_eq!(packets.len(), 10);
        assert_eq!(packets[9][4], 24);
        assert!(packets.iter().all(|p| p[0] == 0x18 && p[1] == 0x00));
        // Turn the writes into read replies and parse them back.
        let replies: Vec<Packet> = packets
            .iter()
            .map(|p| {
                let mut r = *p;
                r[1] = 0x80;
                r
            })
            .collect();
        assert_eq!(parse_keymap(Layer::Default, &replies), Some(keys));
        // A missing page must not parse as a complete layer.
        assert_eq!(parse_keymap(Layer::Default, &replies[..9]), None);
    }

    #[test]
    fn trigger_replies_from_the_real_keyboard_parse() {
        // Captured 2026-11-24: config, then the Esc key.
        let config = reply(&[0x21, 0, 0, 0, 0x06, 0x04, 0x54, 0x01, 0x01, 0x01, 0x0A]);
        let c = parse_trigger_config(&config, false).unwrap();
        assert_eq!((c.min_steps, c.max_steps), (10, 340));
        assert!((c.unit_mm - 0.01).abs() < 1e-6);
        assert!((c.mm(c.max_steps) - 3.4).abs() < 1e-4);

        let esc = reply(&[0x21, 0, 0, 0, 0x0C, 0x05, 0x0C, 0xAA, 0xAA, 0x33, 0x33, 0, 0, 0, 0, 1, 0]);
        let t = parse_key_trigger(&esc).unwrap();
        assert_eq!(t, KeyTrigger { mode: trigger_mode::RT_SINGLE, travel: 170, press: 51, release: 51 });
    }

    #[test]
    fn trigger_write_sets_bitmap_and_values() {
        let t = KeyTrigger { mode: trigger_mode::RT_SPLIT, travel: 0x0123, press: 40, release: 60 };
        // Esc (row 1, col 0), Fn (row 5, col 10), Q (row 2, col 1).
        let p = key_trigger_write(&t, &[22, 120, 45]);
        assert_eq!(&p[..6], &[0x21, 0, 0, 0, 0x18, 0x0D]);
        assert_eq!(p[6], 1 << 1); // column 0: row 1
        assert_eq!(p[7], 1 << 2); // column 1: row 2
        assert_eq!(p[16], 1 << 5); // column 10: row 5
        assert_eq!(&p[28..36], &[0x23, 0x23, 40, 60, 0x01, 0x01, 0, 0]);
    }

    #[test]
    fn poll_rate_write_matches_vendor_layout() {
        let p = poll_rate_write(8000).unwrap();
        assert_eq!(&p[..8], &[0x21, 0, 0, 0, 0, 0x09, 0x01, 0x08]);
        assert!(poll_rate_write(500).is_none());
        // The read reply format the keyboard sent: [6] = rate code.
        let reply = reply(&[0x21, 0, 0, 0, 0x02, 0x09, 0x08]);
        assert_eq!(parse_poll_rate(&reply), Some(8000));
    }

    #[test]
    fn travel_test_packets() {
        let start = travel_test_start(&[22, 120]);
        assert_eq!(&start[..6], &[0x21, 0, 0, 0, 0x18, 0x02]);
        assert_eq!((start[6], start[16]), (1 << 1, 1 << 5));
        assert_eq!(&travel_test_stop()[..6], &[0x21, 0, 0, 0, 0x18, 0x03]);
        let push = reply(&[0x21, 0, 0, 0, 0x04, 0x01, 5, 10, 0x2C, 0x01]);
        assert_eq!(parse_travel(&push), Some((120, 300)));
    }

    #[test]
    fn colour_layer_round_trips_through_read_format() {
        let mut colours = vec![[0u8; 3]; KEY_SLOTS];
        colours[22] = [0xFF, 0x00, 0x10];
        colours[131] = [1, 2, 3];
        let packets = colours_write(1, &colours);
        // 396 bytes: 7 full pages of 54 plus 18, as the keyboard sends.
        assert_eq!(packets.len(), 8);
        assert_eq!(packets[7][4], 18);
        assert!(packets.iter().all(|p| p[0] == 0x09 && p[1] == 0x01));
        let replies: Vec<Packet> = packets
            .iter()
            .map(|p| {
                let mut r = *p;
                r[1] = 0x81;
                r
            })
            .collect();
        assert_eq!(parse_colours(1, &replies), Some(colours));
        assert_eq!(parse_colours(1, &replies[..7]), None);
        assert_eq!(parse_colours(0, &replies), None);
    }

    #[test]
    fn socd_replies_from_the_real_keyboard_parse() {
        // Captured 2026-11-24: one pair A + D, last input wins; SOCD off.
        let page0 = reply(&[0x24, 0x01, 0, 0, 0x28, 0x01, 0x00, 0x04, 0x07]);
        let page1 = reply(&[0x24, 0x01, 0, 1, 0x28]);
        let slots = parse_socd_pairs(&[page0, page1]).unwrap();
        assert_eq!(slots.len(), SOCD_SLOTS);
        assert_eq!(
            slots[0],
            Some(SocdPair { state: 1, mode: socd_mode::LAST_INPUT, key1: 0x04, key2: 0x07 })
        );
        assert!(slots[1..].iter().all(Option::is_none));
        assert_eq!(parse_socd_pairs(&[page0]), None);

        let power = reply(&[0x24, 0x02, 0, 0, 0x06, 0x00, 0, 0, 0, 0xFA, 0xE5]);
        assert_eq!(parse_socd_power(&power), Some(SocdPower { on: false, code1: 0xFA, code2: 0xE5 }));
    }

    #[test]
    fn socd_write_round_trips() {
        let mut slots = vec![None; SOCD_SLOTS];
        slots[0] = Some(SocdPair { state: 1, mode: 0, key1: 0x04, key2: 0x07 });
        slots[13] = Some(SocdPair { state: 1, mode: 3, key1: 0x1A, key2: 0x16 });
        let packets = socd_pairs_write(&slots);
        assert_eq!(packets.len(), 2);
        assert!(packets.iter().all(|p| p[0] == 0x24 && p[1] == 0x00 && p[4] == 0x28));
        let replies: Vec<Packet> = packets
            .iter()
            .map(|p| {
                let mut r = *p;
                r[1] = 0x01;
                r
            })
            .collect();
        assert_eq!(parse_socd_pairs(&replies), Some(slots));
        assert_eq!(&socd_power_write(true)[..6], &[0x24, 0x03, 0, 0, 0x01, 0x01]);
    }

    #[test]
    fn macro_round_trips_and_empty_slots_parse_as_empty() {
        let m = Macro {
            play: macro_play::TIMES,
            count: 3,
            steps: vec![
                MacroStep { press: true, mouse: false, code: 0x04, delay_ms: 30 },
                MacroStep { press: false, mouse: false, code: 0x04, delay_ms: 500 },
                MacroStep { press: true, mouse: true, code: 0x01, delay_ms: 20 },
                MacroStep { press: false, mouse: true, code: 0x01, delay_ms: 0 },
            ],
        };
        let raw = macro_bytes(7, Some(&m));
        assert_eq!(&raw[..8], &[7, 1, 0, 16, 0, 3, 0, 0]);
        let packets = macro_write(7, &raw);
        assert_eq!(packets.len(), 5);
        assert_eq!(packets[4][4], 24); // 4 × 58 + 24 = 256
        let replies: Vec<Packet> = packets
            .iter()
            .map(|p| {
                let mut r = *p;
                r[1] = 0x80 + 7;
                r
            })
            .collect();
        let back = macro_raw(7, &replies).unwrap();
        assert_eq!(back, raw);
        assert_eq!(parse_macro(&back), Some(m));

        // Erased flash (as read from the real keyboard) and zeros are empty.
        assert_eq!(parse_macro(&[0xFF; MACRO_BYTES]), None);
        assert_eq!(parse_macro(&macro_bytes(0, None)), None);
    }

    #[test]
    fn dangerous_commands_are_blocked() {
        assert!(cmd::is_blocked(&reply(&[0x0B])));
        assert!(cmd::is_blocked(&reply(&[0x14, 0x02])));
        assert!(cmd::is_blocked(&travel_test_start(&[22])));
        assert!(!cmd::is_blocked(&travel_test_stop()));
        assert!(!cmd::is_blocked(&key_trigger_write(&KeyTrigger::default(), &[22])));
        assert!(!cmd::is_blocked(&status_request()));
        assert!(!cmd::is_blocked(&light_config_request(Zone::Keys)));
    }
}
