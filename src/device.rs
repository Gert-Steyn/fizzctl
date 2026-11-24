//! USB HID access to the keyboard's vendor configuration interface.
//!
//! The keyboard is a composite USB device; only the collection on usage page
//! `0xFF1B` accepts configuration packets. The typing interfaces are left
//! alone, so the keyboard keeps working normally while this is open.

use std::fmt;
use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::protocol::{self, KeyAction, Layer, PACKET_LEN, Packet};

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

    /// Send a command that the device doesn't answer.
    pub fn write_only(&mut self, packet: &Packet) -> Result<()> {
        self.drain()?;
        self.send(packet)
    }
}
