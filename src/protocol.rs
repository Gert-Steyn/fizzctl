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
    pub const SERIAL: u8 = 0x15;
    pub const PROFILE_TRIGGER: u8 = 0x21;

    /// Commands this app must never send: `0x0B` enters the firmware
    /// bootloader, and `0x14` with sub-command `0x02` is built identically for
    /// both "factory reset" and "read sleep timer" in the vendor app.
    pub const fn is_blocked(packet: &super::Packet) -> bool {
        packet[0] == 0x0B || (packet[0] == 0x14 && packet[1] == 0x02)
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
    fn dangerous_commands_are_blocked() {
        assert!(cmd::is_blocked(&reply(&[0x0B])));
        assert!(cmd::is_blocked(&reply(&[0x14, 0x02])));
        assert!(!cmd::is_blocked(&status_request()));
        assert!(!cmd::is_blocked(&light_config_request(Zone::Keys)));
    }
}
