//! What keys can do, and where the keys are.
//!
//! The catalogue mirrors the vendor app's `config/keys.json`, translated to
//! English and stored in the order the bytes go on the wire. The layout
//! mirrors `config/keys/7153USHEXYXACPARGB.json` (US 60%, 61 keys).

use crate::protocol::{KEY_SLOTS, KeyAction};

/// Action kinds (first byte of a key slot).
pub mod kind {
    pub const KEY: u8 = 0x00;
    pub const FN: u8 = 0x01;
    pub const LIGHT_EFFECT: u8 = 0x02;
    pub const LIGHT_BRIGHTNESS: u8 = 0x03;
    pub const LIGHT_SPEED: u8 = 0x04;
    pub const LIGHT_DIRECTION: u8 = 0x05;
    pub const LIGHT_COLOUR: u8 = 0x06;
    pub const MEDIA: u8 = 0x07;
    pub const MACRO: u8 = 0x10;
    pub const LOCK: u8 = 0x11;
    /// Mouse. On the wire the vendor app swaps two bytes: `[0x12, arg, button, 0]`.
    pub const MOUSE: u8 = 0x12;
    /// Modifier + key, e.g. Win+D: `[0x13, modifier, key, 0]`.
    pub const SHORTCUT: u8 = 0x13;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Letters,
    Modifiers,
    Navigation,
    Function,
    Numpad,
    Media,
    Mouse,
    Shortcuts,
    Lighting,
    Special,
}

impl Group {
    pub const ALL: [Group; 10] = [
        Group::Letters,
        Group::Modifiers,
        Group::Navigation,
        Group::Function,
        Group::Numpad,
        Group::Media,
        Group::Mouse,
        Group::Shortcuts,
        Group::Lighting,
        Group::Special,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Letters => "Letters, numbers and symbols",
            Group::Modifiers => "Modifiers and editing",
            Group::Navigation => "Navigation",
            Group::Function => "Function keys",
            Group::Numpad => "Numpad",
            Group::Media => "Media and apps",
            Group::Mouse => "Mouse",
            Group::Shortcuts => "Shortcuts",
            Group::Lighting => "Lighting controls",
            Group::Special => "Keyboard",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct KeyDef {
    pub name: &'static str,
    pub group: Group,
    pub action: KeyAction,
}

const fn k(name: &'static str, group: Group, hid: u8) -> KeyDef {
    KeyDef {
        name,
        group,
        action: KeyAction::key(hid),
    }
}

const fn x(name: &'static str, group: Group, kind: u8, code: u8, arg1: u8) -> KeyDef {
    KeyDef {
        name,
        group,
        action: KeyAction::new(kind, code, arg1, 0),
    }
}

use Group::*;

pub static CATALOGUE: &[KeyDef] = &[
    // Letters, numbers, symbols
    k("A", Letters, 0x04), k("B", Letters, 0x05), k("C", Letters, 0x06), k("D", Letters, 0x07),
    k("E", Letters, 0x08), k("F", Letters, 0x09), k("G", Letters, 0x0A), k("H", Letters, 0x0B),
    k("I", Letters, 0x0C), k("J", Letters, 0x0D), k("K", Letters, 0x0E), k("L", Letters, 0x0F),
    k("M", Letters, 0x10), k("N", Letters, 0x11), k("O", Letters, 0x12), k("P", Letters, 0x13),
    k("Q", Letters, 0x14), k("R", Letters, 0x15), k("S", Letters, 0x16), k("T", Letters, 0x17),
    k("U", Letters, 0x18), k("V", Letters, 0x19), k("W", Letters, 0x1A), k("X", Letters, 0x1B),
    k("Y", Letters, 0x1C), k("Z", Letters, 0x1D),
    k("1", Letters, 0x1E), k("2", Letters, 0x1F), k("3", Letters, 0x20), k("4", Letters, 0x21),
    k("5", Letters, 0x22), k("6", Letters, 0x23), k("7", Letters, 0x24), k("8", Letters, 0x25),
    k("9", Letters, 0x26), k("0", Letters, 0x27),
    k("-", Letters, 0x2D), k("=", Letters, 0x2E), k("[", Letters, 0x2F), k("]", Letters, 0x30),
    k("\\", Letters, 0x31), k(";", Letters, 0x33), k("'", Letters, 0x34), k("`", Letters, 0x35),
    k(",", Letters, 0x36), k(".", Letters, 0x37), k("/", Letters, 0x38),
    k("ISO \\", Letters, 0x64),
    // Modifiers and editing
    k("Esc", Modifiers, 0x29), k("Tab", Modifiers, 0x2B), k("Caps Lock", Modifiers, 0x39),
    k("Backspace", Modifiers, 0x2A), k("Enter", Modifiers, 0x28), k("Space", Modifiers, 0x2C),
    k("Left Ctrl", Modifiers, 0xE0), k("Right Ctrl", Modifiers, 0xE4),
    k("Left Shift", Modifiers, 0xE1), k("Right Shift", Modifiers, 0xE5),
    k("Left Alt", Modifiers, 0xE2), k("Right Alt", Modifiers, 0xE6),
    k("Left Win", Modifiers, 0xE3), k("Right Win", Modifiers, 0xE7), k("Menu", Modifiers, 0x65),
    // Navigation
    k("Left", Navigation, 0x50), k("Up", Navigation, 0x52), k("Right", Navigation, 0x4F),
    k("Down", Navigation, 0x51), k("Insert", Navigation, 0x49), k("Delete", Navigation, 0x4C),
    k("Home", Navigation, 0x4A), k("End", Navigation, 0x4D), k("Page Up", Navigation, 0x4B),
    k("Page Down", Navigation, 0x4E), k("Print Screen", Navigation, 0x46),
    k("Scroll Lock", Navigation, 0x47), k("Pause", Navigation, 0x48),
    // Function keys
    k("F1", Function, 0x3A), k("F2", Function, 0x3B), k("F3", Function, 0x3C),
    k("F4", Function, 0x3D), k("F5", Function, 0x3E), k("F6", Function, 0x3F),
    k("F7", Function, 0x40), k("F8", Function, 0x41), k("F9", Function, 0x42),
    k("F10", Function, 0x43), k("F11", Function, 0x44), k("F12", Function, 0x45),
    k("F13", Function, 0x68), k("F14", Function, 0x69), k("F15", Function, 0x6A),
    k("F16", Function, 0x6B), k("F17", Function, 0x6C), k("F18", Function, 0x6D),
    k("F19", Function, 0x6E), k("F20", Function, 0x6F), k("F21", Function, 0x70),
    k("F22", Function, 0x71), k("F23", Function, 0x72), k("F24", Function, 0x73),
    // Numpad
    k("Num Lock", Numpad, 0x53), k("Num 1", Numpad, 0x59), k("Num 2", Numpad, 0x5A),
    k("Num 3", Numpad, 0x5B), k("Num 4", Numpad, 0x5C), k("Num 5", Numpad, 0x5D),
    k("Num 6", Numpad, 0x5E), k("Num 7", Numpad, 0x5F), k("Num 8", Numpad, 0x60),
    k("Num 9", Numpad, 0x61), k("Num 0", Numpad, 0x62), k("Num .", Numpad, 0x63),
    k("Num /", Numpad, 0x54), k("Num *", Numpad, 0x55), k("Num -", Numpad, 0x56),
    k("Num +", Numpad, 0x57), k("Num Enter", Numpad, 0x58),
    // Media and apps
    x("Play / pause", Media, kind::MEDIA, 0x13, 0), x("Stop", Media, kind::MEDIA, 0x16, 0),
    x("Previous track", Media, kind::MEDIA, 0x15, 0), x("Next track", Media, kind::MEDIA, 0x14, 0),
    x("Mute", Media, kind::MEDIA, 0x12, 0), x("Volume up", Media, kind::MEDIA, 0x10, 0),
    x("Volume down", Media, kind::MEDIA, 0x11, 0), x("Media player", Media, kind::MEDIA, 0x30, 0),
    x("This PC", Media, kind::MEDIA, 0x34, 0), x("Calculator", Media, kind::MEDIA, 0x32, 0),
    x("Mail", Media, kind::MEDIA, 0x20, 0), x("Browser home", Media, kind::MEDIA, 0x23, 0),
    x("Browser search", Media, kind::MEDIA, 0x21, 0), x("Browser back", Media, kind::MEDIA, 0x24, 0),
    x("Browser forward", Media, kind::MEDIA, 0x25, 0),
    x("Browser favourites", Media, kind::MEDIA, 0x22, 0),
    x("Browser refresh", Media, kind::MEDIA, 0x27, 0), x("Browser stop", Media, kind::MEDIA, 0x26, 0),
    // Mouse (wire order: [0x12, arg, button, 0])
    x("Left click", Mouse, kind::MOUSE, 0x00, 0x01), x("Right click", Mouse, kind::MOUSE, 0x00, 0x02),
    x("Middle click", Mouse, kind::MOUSE, 0x00, 0x04), x("Mouse forward", Mouse, kind::MOUSE, 0x00, 0x08),
    x("Mouse back", Mouse, kind::MOUSE, 0x00, 0x10), x("Scroll up", Mouse, kind::MOUSE, 0x01, 0x01),
    x("Scroll down", Mouse, kind::MOUSE, 0x01, 0xFF),
    // Shortcuts: [0x13, modifier, key, 0]
    x("Show desktop", Shortcuts, kind::SHORTCUT, 0xE3, 0x07),
    x("Close window", Shortcuts, kind::SHORTCUT, 0xE2, 0x3D),
    x("Lock PC", Shortcuts, kind::SHORTCUT, 0xE3, 0x0F),
    x("Undo", Shortcuts, kind::SHORTCUT, 0xE0, 0x1D), x("Copy", Shortcuts, kind::SHORTCUT, 0xE0, 0x06),
    x("Paste", Shortcuts, kind::SHORTCUT, 0xE0, 0x19), x("Cut", Shortcuts, kind::SHORTCUT, 0xE0, 0x1B),
    x("Save", Shortcuts, kind::SHORTCUT, 0xE0, 0x16), x("New", Shortcuts, kind::SHORTCUT, 0xE0, 0x11),
    x("Print", Shortcuts, kind::SHORTCUT, 0xE0, 0x13),
    // Lighting controls
    x("Next effect", Lighting, kind::LIGHT_EFFECT, 0x02, 0),
    x("Brightness up", Lighting, kind::LIGHT_BRIGHTNESS, 0x00, 0),
    x("Brightness down", Lighting, kind::LIGHT_BRIGHTNESS, 0x01, 0),
    x("Speed up", Lighting, kind::LIGHT_SPEED, 0x00, 0),
    x("Speed down", Lighting, kind::LIGHT_SPEED, 0x01, 0),
    x("Next direction", Lighting, kind::LIGHT_DIRECTION, 0x02, 0),
    x("Rainbow on or off", Lighting, kind::LIGHT_COLOUR, 0x00, 0),
    x("Next colour", Lighting, kind::LIGHT_COLOUR, 0x01, 0),
    x("Next background colour", Lighting, kind::LIGHT_COLOUR, 0x02, 0),
    x("Lights on or off", Lighting, kind::LOCK, 0x04, 0),
    // Keyboard
    x("Fn", Special, kind::FN, 0xFA, 0),
    x("Lock keyboard", Special, kind::LOCK, 0x00, 0),
    x("Lock Win key", Special, kind::LOCK, 0x02, 0),
    k("Yen (JIS)", Special, 0x89), k("Ro (JIS)", Special, 0x87),
    k("Muhenkan (JIS)", Special, 0x8B), k("Henkan (JIS)", Special, 0x8A),
    k("Kana (JIS)", Special, 0x88),
];

/// A physical key: its slot in the key map, its keycap label, what it does
/// out of the box, and its position in layout units (keys are 35 wide on a
/// 38 pitch).
#[derive(Clone, Copy, Debug)]
pub struct PhysKey {
    pub slot: usize,
    pub label: &'static str,
    pub default: KeyAction,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Width and height of the layout, in the same units as [`PhysKey`].
pub const LAYOUT_SIZE: (f32, f32) = (587.0, 207.0);

pub static LAYOUT: &[PhysKey] = &[
    PhysKey { slot: 22, label: "Esc", default: KeyAction::key(0x29), x: 10.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 23, label: "1!", default: KeyAction::key(0x1E), x: 48.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 24, label: "2@", default: KeyAction::key(0x1F), x: 86.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 25, label: "3#", default: KeyAction::key(0x20), x: 124.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 26, label: "4$", default: KeyAction::key(0x21), x: 162.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 27, label: "5%", default: KeyAction::key(0x22), x: 200.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 28, label: "6^", default: KeyAction::key(0x23), x: 238.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 29, label: "7&", default: KeyAction::key(0x24), x: 276.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 30, label: "8*", default: KeyAction::key(0x25), x: 314.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 31, label: "9(", default: KeyAction::key(0x26), x: 352.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 32, label: "0)", default: KeyAction::key(0x27), x: 390.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 33, label: "-_", default: KeyAction::key(0x2D), x: 428.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 34, label: "=+", default: KeyAction::key(0x2E), x: 466.0, y: 10.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 36, label: "Back", default: KeyAction::key(0x2A), x: 504.0, y: 10.0, w: 73.0, h: 35.0 },
    PhysKey { slot: 44, label: "Tab", default: KeyAction::key(0x2B), x: 10.0, y: 48.0, w: 54.0, h: 35.0 },
    PhysKey { slot: 45, label: "Q", default: KeyAction::key(0x14), x: 67.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 46, label: "W", default: KeyAction::key(0x1A), x: 105.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 47, label: "E", default: KeyAction::key(0x08), x: 143.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 48, label: "R", default: KeyAction::key(0x15), x: 181.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 49, label: "T", default: KeyAction::key(0x17), x: 219.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 50, label: "Y", default: KeyAction::key(0x1C), x: 257.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 51, label: "U", default: KeyAction::key(0x18), x: 295.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 52, label: "I", default: KeyAction::key(0x0C), x: 333.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 53, label: "O", default: KeyAction::key(0x12), x: 371.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 54, label: "P", default: KeyAction::key(0x13), x: 409.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 55, label: "[{", default: KeyAction::key(0x2F), x: 447.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 56, label: "]}", default: KeyAction::key(0x30), x: 485.0, y: 48.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 58, label: "\\|", default: KeyAction::key(0x31), x: 523.0, y: 48.0, w: 54.0, h: 35.0 },
    PhysKey { slot: 66, label: "Caps", default: KeyAction::key(0x39), x: 10.0, y: 86.0, w: 63.0, h: 35.0 },
    PhysKey { slot: 68, label: "A", default: KeyAction::key(0x04), x: 76.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 69, label: "S", default: KeyAction::key(0x16), x: 114.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 70, label: "D", default: KeyAction::key(0x07), x: 152.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 71, label: "F", default: KeyAction::key(0x09), x: 190.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 72, label: "G", default: KeyAction::key(0x0A), x: 228.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 73, label: "H", default: KeyAction::key(0x0B), x: 266.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 74, label: "J", default: KeyAction::key(0x0D), x: 304.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 75, label: "K", default: KeyAction::key(0x0E), x: 342.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 76, label: "L", default: KeyAction::key(0x0F), x: 380.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 77, label: ";:", default: KeyAction::key(0x33), x: 418.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 78, label: "'\"", default: KeyAction::key(0x34), x: 456.0, y: 86.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 80, label: "Enter", default: KeyAction::key(0x28), x: 494.0, y: 86.0, w: 83.0, h: 35.0 },
    PhysKey { slot: 88, label: "L-Shift", default: KeyAction::key(0xE1), x: 10.0, y: 124.0, w: 83.0, h: 35.0 },
    PhysKey { slot: 90, label: "Z", default: KeyAction::key(0x1D), x: 96.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 91, label: "X", default: KeyAction::key(0x1B), x: 134.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 92, label: "C", default: KeyAction::key(0x06), x: 172.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 93, label: "V", default: KeyAction::key(0x19), x: 210.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 94, label: "B", default: KeyAction::key(0x05), x: 248.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 95, label: "N", default: KeyAction::key(0x11), x: 286.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 96, label: "M", default: KeyAction::key(0x10), x: 324.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 97, label: ",<", default: KeyAction::key(0x36), x: 362.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 98, label: ".>", default: KeyAction::key(0x37), x: 400.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 99, label: "/?", default: KeyAction::key(0x38), x: 438.0, y: 124.0, w: 35.0, h: 35.0 },
    PhysKey { slot: 100, label: "R-Shift", default: KeyAction::key(0xE5), x: 476.0, y: 124.0, w: 101.0, h: 35.0 },
    PhysKey { slot: 110, label: "L-Ctrl", default: KeyAction::key(0xE0), x: 10.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 111, label: "L-Win", default: KeyAction::key(0xE3), x: 60.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 112, label: "L-Alt", default: KeyAction::key(0xE2), x: 110.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 116, label: "Space", default: KeyAction::key(0x2C), x: 160.0, y: 162.0, w: 217.0, h: 35.0 },
    PhysKey { slot: 119, label: "R-Alt", default: KeyAction::key(0xE6), x: 380.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 120, label: "Fn", default: KeyAction::new(1, 0xFA, 0, 0), x: 430.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 121, label: "Menu", default: KeyAction::key(0x65), x: 480.0, y: 162.0, w: 47.0, h: 35.0 },
    PhysKey { slot: 122, label: "R-Ctrl", default: KeyAction::key(0xE4), x: 530.0, y: 162.0, w: 47.0, h: 35.0 },
];

/// Catalogue entry for an action, if it's a known one.
pub fn lookup(action: KeyAction) -> Option<&'static KeyDef> {
    CATALOGUE.iter().find(|d| d.action == action)
}

/// Human-readable name for any action, including unknown ones.
pub fn describe(action: KeyAction) -> String {
    if action == KeyAction::default() {
        return "Not set".to_owned();
    }
    if let Some(def) = lookup(action) {
        return def.name.to_owned();
    }
    match action.kind {
        kind::MACRO => format!("Macro {}", action.arg1),
        _ => format!(
            "Custom {:02X} {:02X} {:02X} {:02X}",
            action.kind, action.code, action.arg1, action.arg2
        ),
    }
}

/// The factory default-layer map, built from the layout.
pub fn default_layer() -> Vec<KeyAction> {
    let mut keys = vec![KeyAction::default(); KEY_SLOTS];
    for key in LAYOUT {
        keys[key.slot] = key.default;
    }
    keys
}

/// True when some key in the layer is the Fn key.
pub fn has_fn_key(keys: &[KeyAction]) -> bool {
    LAYOUT
        .iter()
        .any(|k| keys.get(k.slot).is_some_and(|a| a.kind == kind::FN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_slots_are_unique_and_in_range() {
        let mut seen = std::collections::HashSet::new();
        for key in LAYOUT {
            assert!(key.slot < KEY_SLOTS);
            assert!(seen.insert(key.slot), "duplicate slot {}", key.slot);
        }
        assert_eq!(LAYOUT.len(), 61);
    }

    #[test]
    fn catalogue_actions_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for def in CATALOGUE {
            assert!(seen.insert(def.action), "duplicate action for {}", def.name);
        }
    }

    #[test]
    fn every_default_key_is_in_the_catalogue() {
        for key in LAYOUT {
            assert!(lookup(key.default).is_some(), "{} missing", key.label);
        }
    }

    #[test]
    fn default_layer_matches_the_real_keyboard() {
        // Non-empty slots read from the keyboard on 2026-11-24.
        let keys = default_layer();
        assert_eq!(keys[22], KeyAction::key(0x29)); // Esc
        assert_eq!(keys[120], KeyAction::new(1, 0xFA, 0, 0)); // Fn
        assert_eq!(keys[121], KeyAction::key(0x65)); // Menu
        assert!(has_fn_key(&keys));
        assert_eq!(keys.iter().filter(|k| **k != KeyAction::default()).count(), 61);
    }
}
