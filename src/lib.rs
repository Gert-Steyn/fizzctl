//! Protocol and USB HID access for the Redragon Fizz Magnetic Plus.
//!
//! `protocol` is pure (packet builders and reply parsers, no I/O) so it can be
//! unit-tested. `device` talks to the keyboard through `hidapi`, which works on
//! Windows and Linux. See `PROTOCOL.md` for the reverse-engineered details.

pub mod device;
pub mod keymap;
pub mod protocol;
