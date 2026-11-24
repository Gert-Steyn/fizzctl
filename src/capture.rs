//! Turns egui's keyboard events into HID key events for the macro recorder.
//!
//! A macro stores HID usage codes, which name key *positions*, so keys are
//! read from `physical_key` (layout independent) and only fall back to the
//! logical key when the platform doesn't report a position.

use egui::{Event, Key};

/// HID usage code of a keyboard key, for the keys egui can report.
/// Numpad digits come through as the top-row digits, and keys without a
/// macro equivalent (media keys and similar) give `None`.
pub fn hid_code(key: Key) -> Option<u8> {
    use Key::*;
    Some(match key {
        A => 0x04,
        B => 0x05,
        C => 0x06,
        D => 0x07,
        E => 0x08,
        F => 0x09,
        G => 0x0A,
        H => 0x0B,
        I => 0x0C,
        J => 0x0D,
        K => 0x0E,
        L => 0x0F,
        M => 0x10,
        N => 0x11,
        O => 0x12,
        P => 0x13,
        Q => 0x14,
        R => 0x15,
        S => 0x16,
        T => 0x17,
        U => 0x18,
        V => 0x19,
        W => 0x1A,
        X => 0x1B,
        Y => 0x1C,
        Z => 0x1D,
        Num1 => 0x1E,
        Num2 => 0x1F,
        Num3 => 0x20,
        Num4 => 0x21,
        Num5 => 0x22,
        Num6 => 0x23,
        Num7 => 0x24,
        Num8 => 0x25,
        Num9 => 0x26,
        Num0 => 0x27,
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equals => 0x2E,
        OpenBracket => 0x2F,
        CloseBracket => 0x30,
        Backslash => 0x31,
        Semicolon => 0x33,
        Quote => 0x34,
        Backtick => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash => 0x38,
        CapsLock => 0x39,
        F1 => 0x3A,
        F2 => 0x3B,
        F3 => 0x3C,
        F4 => 0x3D,
        F5 => 0x3E,
        F6 => 0x3F,
        F7 => 0x40,
        F8 => 0x41,
        F9 => 0x42,
        F10 => 0x43,
        F11 => 0x44,
        F12 => 0x45,
        PrintScreen => 0x46,
        ScrollLock => 0x47,
        Pause => 0x48,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        NumLock => 0x53,
        Plus => 0x57,
        IntlBackslash => 0x64,
        Menu => 0x65,
        F13 => 0x68,
        F14 => 0x69,
        F15 => 0x6A,
        F16 => 0x6B,
        F17 => 0x6C,
        F18 => 0x6D,
        F19 => 0x6E,
        F20 => 0x6F,
        F21 => 0x70,
        F22 => 0x71,
        F23 => 0x72,
        F24 => 0x73,
        ControlLeft => 0xE0,
        ShiftLeft => 0xE1,
        AltLeft => 0xE2,
        SuperLeft => 0xE3,
        ControlRight => 0xE4,
        ShiftRight => 0xE5,
        AltRight => 0xE6,
        SuperRight => 0xE7,
        _ => return None,
    })
}

/// HID codes for the keys egui turns into clipboard events: with Ctrl held,
/// pressing C, X or V arrives as `Copy`, `Cut` or `Paste` and no key event.
const KEY_C: u8 = 0x06;
const KEY_X: u8 = 0x1B;
const KEY_V: u8 = 0x19;

/// The keyboard activity in one frame's events, as `(HID code, pressed)` in
/// order. Auto-repeat is dropped. Ctrl+Insert and Shift+Insert/Delete, which
/// egui also reports as clipboard events, are recorded as C, V and X.
pub fn key_events(events: &[Event]) -> Vec<(u8, bool)> {
    let mut out = Vec::new();
    for event in events {
        match event {
            Event::Key {
                key,
                physical_key,
                pressed,
                repeat: false,
                ..
            } => {
                if let Some(code) = hid_code(physical_key.unwrap_or(*key)) {
                    out.push((code, *pressed));
                }
            }
            Event::Copy => out.push((KEY_C, true)),
            Event::Cut => out.push((KEY_X, true)),
            Event::Paste(_) | Event::PasteImage(_) => out.push((KEY_V, true)),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;
    use fizz::keymap::{self, kind};
    use fizz::protocol::KeyAction;

    fn key(key: Key, physical_key: Option<Key>, pressed: bool, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn every_mapped_code_is_a_key_the_app_knows_by_name() {
        // Everything the recorder can capture must show up properly in the
        // steps list, which names keys from the catalogue.
        for k in Key::ALL {
            if let Some(code) = hid_code(*k) {
                let def = keymap::lookup(KeyAction::key(code))
                    .unwrap_or_else(|| panic!("{k:?} -> {code:#04x} isn't in the catalogue"));
                assert_eq!(def.action.kind, kind::KEY);
            }
        }
    }

    #[test]
    fn spot_checks_against_the_hid_usage_table() {
        assert_eq!(hid_code(Key::A), Some(0x04));
        assert_eq!(hid_code(Key::Num1), Some(0x1E));
        assert_eq!(hid_code(Key::Num0), Some(0x27));
        assert_eq!(hid_code(Key::Enter), Some(0x28));
        assert_eq!(hid_code(Key::F1), Some(0x3A));
        assert_eq!(hid_code(Key::F12), Some(0x45));
        assert_eq!(hid_code(Key::ArrowUp), Some(0x52));
        assert_eq!(hid_code(Key::ShiftRight), Some(0xE5));
        assert_eq!(hid_code(Key::ControlLeft), Some(0xE0));
        assert_eq!(hid_code(Key::MediaPlayPause), None);
    }

    #[test]
    fn keys_are_read_by_position_and_repeats_are_dropped() {
        let events = [
            // The Z position on an AZERTY layout reports logical W, physical Z.
            key(Key::W, Some(Key::Z), true, false),
            key(Key::W, Some(Key::Z), true, true), // auto-repeat
            key(Key::W, Some(Key::Z), false, false),
            // No physical key reported: fall back to the logical one.
            key(Key::B, None, true, false),
            key(Key::MediaStop, None, true, false), // unmappable
        ];
        assert_eq!(key_events(&events), vec![(0x1D, true), (0x1D, false), (0x05, true)]);
    }

    #[test]
    fn clipboard_shortcuts_count_as_the_key_going_down() {
        // Ctrl+C: Ctrl down, Copy event (no key-down for C), C up, Ctrl up.
        let events = [
            key(Key::ControlLeft, Some(Key::ControlLeft), true, false),
            Event::Copy,
            key(Key::C, Some(Key::C), false, false),
            key(Key::ControlLeft, Some(Key::ControlLeft), false, false),
            Event::Paste("text".into()),
            Event::Cut,
        ];
        assert_eq!(
            key_events(&events),
            vec![
                (0xE0, true),
                (0x06, true),
                (0x06, false),
                (0xE0, false),
                (0x19, true),
                (0x1B, true),
            ]
        );
    }
}
