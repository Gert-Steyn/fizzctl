//! USB HID access to the keyboard's vendor configuration interface.
//!
//! The keyboard is a composite USB device; only the collection on usage page
//! `0xFF1B` accepts configuration packets. The typing interfaces are left
//! alone, so the keyboard keeps working normally while this is open.

use std::fmt;
use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::protocol::{
    self, KeyAction, KeyTrigger, Layer, PACKET_LEN, Packet, SocdPair, SocdPower, TriggerConfig,
};

pub const VENDOR_ID: u16 = 0x2E3C;
/// `C365` is the normal mode; `C366`/`C367` are the gamepad modes and `C368`
/// is listed alongside them by the vendor app.
pub const PRODUCT_IDS: [u16; 4] = [0xC365, 0xC366, 0xC367, 0xC368];
pub const USAGE_PAGE: u16 = 0xFF1B;
pub const USAGE: u16 = 0x91;
/// The vendor interface number, used when the OS doesn't report usage pages.
const CONFIG_INTERFACE: i32 = 2;
const REPORT_ID: u8 = 0x01;

#[derive(Debug)]
pub enum Error {
    NotFound,
    Hid(hidapi::HidError),
    Timeout(&'static str),
    Blocked,
    Incomplete,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => write!(f, "keyboard not found"),
            Error::Hid(e) => write!(f, "{e}"),
            Error::Timeout(what) => write!(f, "no reply to {what}"),
            Error::Blocked => write!(f, "refused to send a blocked command"),
            Error::Incomplete => write!(f, "refused to write an incomplete key map"),
        }
    }
}

impl std::error::Error for Error {}

impl From<hidapi::HidError> for Error {
    fn from(e: hidapi::HidError) -> Self {
        Error::Hid(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Identity of the opened keyboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_name: String,
    pub manufacturer: String,
    pub path: String,
}

const REPLY_TIMEOUT: Duration = Duration::from_millis(400);
const PAGE_IDLE: Duration = Duration::from_millis(300);
const ACK_TIMEOUT: Duration = Duration::from_millis(300);
const STORE_TIMEOUT: Duration = Duration::from_secs(2);
const STORE_POLL: Duration = Duration::from_millis(60);
const STORE_SETTLE: Duration = Duration::from_millis(250);

/// Result of [`Keyboard::write_keymap`].
#[derive(Clone, Debug)]
pub struct KeymapWrite {
    pub pages: usize,
    pub acked: usize,
    pub storing_seen: bool,
    /// The layer as read back after saving; `None` if the read-back was incomplete.
    pub stored: Option<Vec<KeyAction>>,
    /// True when the read-back matches exactly what was written.
    pub verified: bool,
}

/// How long to wait for the keyboard to confirm a macro page (the vendor app waits 100 ms).
const MACRO_ACK_TIMEOUT: Duration = Duration::from_millis(100);

/// Result of [`Keyboard::write_colours`].
#[derive(Clone, Debug)]
pub struct ColourWrite {
    pub pages: usize,
    pub acked: usize,
    pub stored: Option<Vec<[u8; 3]>>,
    pub verified: bool,
}

/// Pause between trigger write packets, which the keyboard doesn't acknowledge.
const TRIGGER_WRITE_GAP: Duration = Duration::from_millis(30);

/// Result of [`Keyboard::write_key_triggers`].
#[derive(Clone, Debug)]
pub struct TriggerWrite {
    /// Each written slot with what the keyboard reports back (`None` if it didn't answer).
    pub stored: Vec<(usize, Option<KeyTrigger>)>,
    pub verified: bool,
}

/// Direction of a logged packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Out,
    In,
}

fn is_config_interface(info: &DeviceInfo) -> bool {
    info.vendor_id() == VENDOR_ID
        && PRODUCT_IDS.contains(&info.product_id())
        && if info.usage_page() != 0 {
            info.usage_page() == USAGE_PAGE
        } else {
            info.interface_number() == CONFIG_INTERFACE
        }
}

/// Called for every packet sent or received, for the packet log.
pub type PacketHook = Box<dyn FnMut(Dir, &Packet) + Send>;

pub struct Keyboard {
    dev: HidDevice,
    pub ident: Ident,
    pub on_packet: Option<PacketHook>,
}

impl Keyboard {
    /// Find and open the configuration interface of the first matching keyboard.
    pub fn open(api: &HidApi) -> Result<Self> {
        let info = api
            .device_list()
            .find(|i| is_config_interface(i))
            .ok_or(Error::NotFound)?;
        let dev = info.open_device(api)?;
        dev.set_blocking_mode(false)?;
        let ident = Ident {
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            product_name: info.product_string().unwrap_or_default().to_owned(),
            manufacturer: info.manufacturer_string().unwrap_or_default().to_owned(),
            path: info.path().to_string_lossy().into_owned(),
        };
        Ok(Self {
            dev,
            ident,
            on_packet: None,
        })
    }

    fn log(&mut self, dir: Dir, packet: &Packet) {
        if let Some(f) = self.on_packet.as_mut() {
            f(dir, packet);
        }
    }

    /// Send one packet as output report `0x01`.
    pub fn send(&mut self, packet: &Packet) -> Result<()> {
        if protocol::cmd::is_blocked(packet) {
            return Err(Error::Blocked);
        }
        self.send_raw(packet)
    }

    /// Send a packet even if it's on the blocked list. Only for one-off
    /// experiments Gert has agreed to, run from `fizz-probe` with
    /// `--allow-blocked`. The app must never call this.
    pub fn send_overriding_block(&mut self, packet: &Packet) -> Result<()> {
        self.drain()?;
        self.send_raw(packet)
    }

    fn send_raw(&mut self, packet: &Packet) -> Result<()> {
        let mut buf = [0u8; PACKET_LEN + 1];
        buf[0] = REPORT_ID;
        buf[1..].copy_from_slice(packet);
        self.dev.write(&buf)?;
        self.log(Dir::Out, packet);
        Ok(())
    }

    /// Read one input report, waiting at most `timeout`. Reports with another
    /// report ID are skipped.
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut buf = [0u8; PACKET_LEN + 2];
            let n = self.dev.read_timeout(&mut buf, left.as_millis() as i32)?;
            if n == 0 {
                return Ok(None);
            }
            // Windows and Linux hidraw both prefix numbered reports with the ID.
            let data = if n > PACKET_LEN {
                if buf[0] != REPORT_ID {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                    continue;
                }
                &buf[1..=PACKET_LEN]
            } else {
                &buf[..n]
            };
            let mut packet = [0u8; PACKET_LEN];
            packet[..data.len()].copy_from_slice(data);
            self.log(Dir::In, &packet);
            return Ok(Some(packet));
        }
    }

    /// Throw away anything already queued, so a reply can't be confused with
    /// an older, unrelated packet.
    pub fn drain(&mut self) -> Result<()> {
        while self.recv(Duration::ZERO)?.is_some() {}
        Ok(())
    }

    /// Send a request and wait for the first reply that `matches`.
    pub fn request(
        &mut self,
        what: &'static str,
        packet: &Packet,
        matches: impl Fn(&Packet) -> bool,
        timeout: Duration,
    ) -> Result<Packet> {
        self.drain()?;
        self.send(packet)?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            if let Some(reply) = self.recv(left)?
                && matches(&reply)
            {
                return Ok(reply);
            }
        }
        Err(Error::Timeout(what))
    }

    /// Request where the reply has the same command byte as the request.
    pub fn request_same(
        &mut self,
        what: &'static str,
        packet: &Packet,
        timeout: Duration,
    ) -> Result<Packet> {
        let command = packet[0];
        self.request(what, packet, |r| r[0] == command, timeout)
    }

    /// Send a request whose reply spans several pages (bytes 2..=3), and
    /// collect pages with the same command byte until `last_page` arrives or
    /// the device goes quiet for `idle`.
    pub fn request_pages(
        &mut self,
        what: &'static str,
        packet: &Packet,
        last_page: u16,
        timeout: Duration,
        idle: Duration,
    ) -> Result<Vec<Packet>> {
        let command = packet[0];
        let first = self.request_same(what, packet, timeout)?;
        let mut pages = vec![first];
        if protocol::page(&first) >= last_page {
            return Ok(pages);
        }
        while let Some(reply) = self.recv(idle)? {
            if reply[0] != command {
                continue;
            }
            pages.push(reply);
            if protocol::page(&reply) >= last_page {
                break;
            }
        }
        Ok(pages)
    }

    /// Read one key-map layer. `None` when a page was missing.
    pub fn read_keymap(&mut self, layer: Layer) -> Result<Option<Vec<KeyAction>>> {
        let pages = self.request_pages(
            "key map",
            &protocol::keymap_request(layer),
            protocol::KEYMAP_LAST_PAGE,
            REPLY_TIMEOUT,
            PAGE_IDLE,
        )?;
        Ok(protocol::parse_keymap(layer, &pages))
    }

    /// Poll the status "storing" flag until the keyboard has finished writing
    /// to flash. Like the vendor app: stop once the flag has been seen and
    /// cleared, or after a short settle time if it never appears. Returns
    /// whether the flag was seen. Status packets are not logged.
    pub fn wait_until_stored(&mut self) -> Result<bool> {
        let log = self.on_packet.take();
        let start = Instant::now();
        let mut seen = false;
        let result = loop {
            match self.request_same("status", &protocol::status_request(), REPLY_TIMEOUT) {
                Ok(reply) if protocol::is_storing(&reply) => seen = true,
                Ok(_) if seen || start.elapsed() >= STORE_SETTLE => break Ok(seen),
                Ok(_) | Err(Error::Timeout(_)) => {}
                Err(e) => break Err(e),
            }
            if start.elapsed() >= STORE_TIMEOUT {
                break Ok(seen);
            }
            std::thread::sleep(STORE_POLL);
        };
        self.on_packet = log;
        result
    }

    /// Write a whole key-map layer, wait for it to be stored, and read it
    /// back. Check [`KeymapWrite::verified`] before trusting the result.
    pub fn write_keymap(&mut self, layer: Layer, keys: &[KeyAction]) -> Result<KeymapWrite> {
        if keys.len() != protocol::KEY_SLOTS {
            return Err(Error::Incomplete);
        }
        let sub = layer.write_sub();
        let packets = protocol::keymap_write(layer, keys);
        let mut acked = 0;
        for packet in &packets {
            match self.request(
                "key map page",
                packet,
                |r| r[0] == protocol::cmd::KEYMAP && (r[1] == 0 || r[1] == sub),
                ACK_TIMEOUT,
            ) {
                Ok(_) => acked += 1,
                // Not every firmware acknowledges every page; the read-back
                // is the real check.
                Err(Error::Timeout(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let storing_seen = self.wait_until_stored()?;
        let stored = self.read_keymap(layer)?;
        Ok(KeymapWrite {
            pages: packets.len(),
            acked,
            storing_seen,
            verified: stored.as_deref() == Some(keys),
            stored,
        })
    }

    /// Which colour layer (0 or 1) the Custom effect shows.
    pub fn read_colour_layer(&mut self) -> Result<Option<u8>> {
        let reply = self.request(
            "colour layer",
            &protocol::colour_layer_request(),
            |r| r[0] == protocol::cmd::CUSTOM_COLOURS && r[1] != 0x80 && r[1] != 0x81,
            REPLY_TIMEOUT,
        )?;
        Ok(protocol::parse_colour_layer(&reply))
    }

    /// Every slot's colour in `layer`. `None` when a page was missing.
    pub fn read_colours(&mut self, layer: u8) -> Result<Option<Vec<[u8; 3]>>> {
        let pages = self.request_pages(
            "colours",
            &protocol::colours_request(layer),
            protocol::COLOUR_LAST_PAGE,
            REPLY_TIMEOUT,
            PAGE_IDLE,
        )?;
        Ok(protocol::parse_colours(layer, &pages))
    }

    /// Write a whole colour layer (each page acknowledged), then read it back.
    pub fn write_colours(&mut self, layer: u8, colours: &[[u8; 3]]) -> Result<ColourWrite> {
        if colours.len() != protocol::KEY_SLOTS {
            return Err(Error::Incomplete);
        }
        let packets = protocol::colours_write(layer, colours);
        let mut acked = 0;
        for packet in &packets {
            match self.request(
                "colour page",
                packet,
                |r| r[0] == protocol::cmd::CUSTOM_COLOURS && r[1] == layer & 1,
                ACK_TIMEOUT,
            ) {
                Ok(_) => acked += 1,
                Err(Error::Timeout(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let stored = self.read_colours(layer)?;
        Ok(ColourWrite {
            pages: packets.len(),
            acked,
            verified: stored.as_deref() == Some(colours),
            stored,
        })
    }

    /// A macro slot's 256 raw bytes. `None` when page 0 was missing.
    pub fn read_macro_raw(&mut self, slot: usize) -> Result<Option<[u8; protocol::MACRO_BYTES]>> {
        let pages = self.request_pages(
            "macro",
            &protocol::macro_request(slot),
            protocol::MACRO_LAST_PAGE,
            REPLY_TIMEOUT,
            PAGE_IDLE,
        )?;
        Ok(protocol::macro_raw(slot, &pages))
    }

    /// Write a macro slot's raw bytes (waiting briefly for each page's
    /// confirmation), then read the slot back.
    pub fn write_macro_raw(
        &mut self,
        slot: usize,
        raw: &[u8; protocol::MACRO_BYTES],
    ) -> Result<Option<[u8; protocol::MACRO_BYTES]>> {
        for packet in protocol::macro_write(slot, raw) {
            match self.request(
                "macro page",
                &packet,
                |r| r[0] == protocol::cmd::MACRO && r[1] as usize == slot,
                MACRO_ACK_TIMEOUT,
            ) {
                Ok(_) | Err(Error::Timeout(_)) => {}
                Err(e) => return Err(e),
            }
        }
        self.read_macro_raw(slot)
    }

    /// Every macro slot, decoded; `None` overall if any slot didn't read.
    pub fn read_macros(&mut self) -> Result<Option<Vec<Option<protocol::Macro>>>> {
        let mut out = Vec::with_capacity(protocol::MACRO_SLOTS);
        for slot in 0..protocol::MACRO_SLOTS {
            match self.read_macro_raw(slot)? {
                Some(raw) => out.push(protocol::parse_macro(&raw)),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    /// Save all 10 macro slots in order from slot 0, then read them back.
    /// Macro memory is flash that only erases when the full set is written
    /// from slot 0, so slots can never be saved one at a time.
    pub fn write_macros(
        &mut self,
        macros: &[Option<protocol::Macro>],
    ) -> Result<Option<Vec<Option<protocol::Macro>>>> {
        if macros.len() != protocol::MACRO_SLOTS {
            return Err(Error::Incomplete);
        }
        for (slot, m) in macros.iter().enumerate() {
            let raw = protocol::macro_bytes(slot, m.as_ref());
            for packet in protocol::macro_write(slot, &raw) {
                match self.request(
                    "macro page",
                    &packet,
                    |r| r[0] == protocol::cmd::MACRO && r[1] as usize == slot,
                    MACRO_ACK_TIMEOUT,
                ) {
                    Ok(_) | Err(Error::Timeout(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        self.read_macros()
    }

    /// All 20 SOCD pair slots. `None` when a page was missing.
    pub fn read_socd_pairs(&mut self) -> Result<Option<Vec<Option<SocdPair>>>> {
        let pages = self.request_pages(
            "SOCD pairs",
            &protocol::socd_pairs_request(),
            protocol::SOCD_LAST_PAGE,
            REPLY_TIMEOUT,
            PAGE_IDLE,
        )?;
        Ok(protocol::parse_socd_pairs(&pages))
    }

    pub fn read_socd_power(&mut self) -> Result<Option<SocdPower>> {
        let reply = self.request(
            "SOCD power",
            &protocol::socd_power_request(),
            |r| r[0] == protocol::cmd::SOCD && r[1] == 0x02,
            REPLY_TIMEOUT,
        )?;
        Ok(protocol::parse_socd_power(&reply))
    }

    /// Write all SOCD slots (each page acknowledged), then read them back.
    /// `Ok(None)` if the read-back was incomplete.
    pub fn write_socd_pairs(
        &mut self,
        slots: &[Option<SocdPair>],
    ) -> Result<Option<Vec<Option<SocdPair>>>> {
        if slots.len() != protocol::SOCD_SLOTS {
            return Err(Error::Incomplete);
        }
        for packet in protocol::socd_pairs_write(slots) {
            match self.request(
                "SOCD page",
                &packet,
                |r| r[0] == protocol::cmd::SOCD && r[1] == 0x00,
                ACK_TIMEOUT,
            ) {
                Ok(_) | Err(Error::Timeout(_)) => {}
                Err(e) => return Err(e),
            }
        }
        self.read_socd_pairs()
    }

    /// Turn SOCD on or off and read the state back.
    pub fn set_socd_power(&mut self, on: bool) -> Result<Option<SocdPower>> {
        self.write_only(&protocol::socd_power_write(on))?;
        std::thread::sleep(Duration::from_millis(50));
        self.read_socd_power()
    }

    /// Actuation range and step size.
    pub fn read_trigger_config(&mut self, high_precision: bool) -> Result<Option<TriggerConfig>> {
        let reply = self.request(
            "trigger config",
            &protocol::trigger_config_request(),
            |r| r[0] == protocol::cmd::PROFILE_TRIGGER && r[5] == 0x04,
            REPLY_TIMEOUT,
        )?;
        Ok(protocol::parse_trigger_config(&reply, high_precision))
    }

    /// Actuation settings of each key in `slots`, in the same order. The
    /// reply echoes the key's row and column; a mismatch counts as missing.
    pub fn read_key_triggers(&mut self, slots: &[usize]) -> Result<Vec<Option<KeyTrigger>>> {
        let mut out = Vec::with_capacity(slots.len());
        for &slot in slots {
            let row = (slot / protocol::TRIGGER_COLS) as u8;
            let col = (slot % protocol::TRIGGER_COLS) as u8;
            let reply = self.request(
                "key trigger",
                &protocol::key_trigger_request(slot),
                |r| {
                    r[0] == protocol::cmd::PROFILE_TRIGGER
                        && r[5] == 0x05
                        && r[15] == row
                        && r[16] == col
                },
                REPLY_TIMEOUT,
            );
            match reply {
                Ok(r) => out.push(protocol::parse_key_trigger(&r)),
                Err(Error::Timeout(_)) => out.push(None),
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    /// Write actuation settings, one packet per distinct setting, then read
    /// every written key back. `verified` is true when all match.
    pub fn write_key_triggers(
        &mut self,
        groups: &[(KeyTrigger, Vec<usize>)],
    ) -> Result<TriggerWrite> {
        for (trigger, slots) in groups {
            if !slots.is_empty() {
                self.write_only(&protocol::key_trigger_write(trigger, slots))?;
                std::thread::sleep(TRIGGER_WRITE_GAP);
            }
        }
        let slots: Vec<usize> = groups.iter().flat_map(|(_, s)| s.iter().copied()).collect();
        let stored = self.read_key_triggers(&slots)?;
        let verified = groups
            .iter()
            .flat_map(|(t, s)| s.iter().map(move |_| *t))
            .zip(&stored)
            .all(|(want, got)| *got == Some(want));
        Ok(TriggerWrite {
            stored: slots.into_iter().zip(stored).collect(),
            verified,
        })
    }

    /// Send a command that the device doesn't answer.
    pub fn write_only(&mut self, packet: &Packet) -> Result<()> {
        self.drain()?;
        self.send(packet)
    }
}
