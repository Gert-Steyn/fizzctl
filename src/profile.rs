//! A whole-keyboard settings file.
//!
//! [`read`] collects everything the app can configure into a [`Profile`],
//! which saves as JSON; [`apply`] writes a profile back, section by section,
//! checking each one by reading it back. Sections missing from a file are
//! left alone, so partial files work too.
//!
//! The file holds no serial number, only the product code and firmware of
//! the keyboard it came from, so it is safe to share.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::device::{Error, Keyboard, Result};
use crate::keymap::LAYOUT;
use crate::protocol::{
    self, KEY_SLOTS, KeyAction, KeyTrigger, Layer, LightConfig, MACRO_MAX_STEPS, MACRO_SLOTS,
    Macro, POLL_RATES_HZ, SOCD_SLOTS, SocdPair, Zone, macro_play, socd_mode,
};
use crate::recorder::is_balanced;

/// Version of the file layout. Files with a higher number come from a newer app.
pub const PROFILE_FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub format: u32,
    /// When it was saved, in seconds since 1970 (UTC).
    pub saved_at: u64,
    /// Product code of the keyboard it came from.
    pub product: Option<String>,
    pub firmware: Option<String>,
    pub keymap_default: Option<Vec<KeyAction>>,
    pub keymap_fn: Option<Vec<KeyAction>>,
    /// Actuation per key slot; `None` for slots without a key.
    pub triggers: Option<Vec<Option<KeyTrigger>>>,
    pub light: Option<LightConfig>,
    /// Which of the two colour layers `colours` belongs to.
    pub colour_layer: Option<u8>,
    pub colours: Option<Vec<[u8; 3]>>,
    pub poll_rate_hz: Option<u32>,
    pub socd_pairs: Option<Vec<Option<SocdPair>>>,
    pub socd_on: Option<bool>,
    pub macros: Option<Vec<Option<Macro>>>,
}

/// The parts of a profile, in the order they are applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Macros,
    KeyMapDefault,
    KeyMapFn,
    Colours,
    Actuation,
    SocdPairs,
    SocdPower,
    PollRate,
    /// Last, because writing colours switches the keyboard to the Custom effect.
    Lighting,
}

impl Section {
    pub const ALL: [Section; 9] = [
        Section::Macros,
        Section::KeyMapDefault,
        Section::KeyMapFn,
        Section::Colours,
        Section::Actuation,
        Section::SocdPairs,
        Section::SocdPower,
        Section::PollRate,
        Section::Lighting,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Section::Macros => "Macros",
            Section::KeyMapDefault => "Default key map",
            Section::KeyMapFn => "Fn key map",
            Section::Colours => "Key colours",
            Section::Actuation => "Actuation",
            Section::SocdPairs => "Snap tap pairs",
            Section::SocdPower => "Snap tap on or off",
            Section::PollRate => "Polling rate",
            Section::Lighting => "Lighting",
        }
    }
}

impl Profile {
    /// An empty profile: every section absent.
    pub fn empty() -> Self {
        Self {
            format: PROFILE_FORMAT,
            saved_at: now(),
            product: None,
            firmware: None,
            keymap_default: None,
            keymap_fn: None,
            triggers: None,
            light: None,
            colour_layer: None,
            colours: None,
            poll_rate_hz: None,
            socd_pairs: None,
            socd_on: None,
            macros: None,
        }
    }

    pub fn has(&self, section: Section) -> bool {
        match section {
            Section::Macros => self.macros.is_some(),
            Section::KeyMapDefault => self.keymap_default.is_some(),
            Section::KeyMapFn => self.keymap_fn.is_some(),
            Section::Colours => self.colours.is_some(),
            Section::Actuation => self.triggers.is_some(),
            Section::SocdPairs => self.socd_pairs.is_some(),
            Section::SocdPower => self.socd_on.is_some(),
            Section::PollRate => self.poll_rate_hz.is_some(),
            Section::Lighting => self.light.is_some(),
        }
    }

    /// The sections this profile contains.
    pub fn sections(&self) -> Vec<Section> {
        Section::ALL.into_iter().filter(|s| self.has(*s)).collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a profile always serializes")
    }

    pub fn from_json(text: &str) -> std::result::Result<Self, String> {
        // Editors such as Notepad save a byte-order mark that JSON doesn't allow.
        let text = text.trim_start_matches('\u{feff}');
        serde_json::from_str(text).map_err(|e| format!("This isn't a Fizz control settings file: {e}."))
    }

    /// Everything wrong with the contents that would make them unsafe to
    /// write to a keyboard. Empty when the profile is fine.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.format == 0 || self.format > PROFILE_FORMAT {
            out.push(format!(
                "The file is format {}, but this version of Fizz control reads format {PROFILE_FORMAT}.",
                self.format
            ));
        }
        let len = |name: &str, got: Option<usize>, want: usize, out: &mut Vec<String>| {
            if let Some(got) = got
                && got != want
            {
                out.push(format!("{name} has {got} entries, expected {want}."));
            }
        };
        len("The default key map", self.keymap_default.as_ref().map(Vec::len), KEY_SLOTS, &mut out);
        len("The Fn key map", self.keymap_fn.as_ref().map(Vec::len), KEY_SLOTS, &mut out);
        len("The actuation list", self.triggers.as_ref().map(Vec::len), KEY_SLOTS, &mut out);
        len("The key colours", self.colours.as_ref().map(Vec::len), KEY_SLOTS, &mut out);
        len("The snap tap list", self.socd_pairs.as_ref().map(Vec::len), SOCD_SLOTS, &mut out);
        len("The macro list", self.macros.as_ref().map(Vec::len), MACRO_SLOTS, &mut out);
        if self.colours.is_some() && self.colour_layer.is_some_and(|l| l > 1) {
            out.push("The key colours name a colour layer the keyboard doesn't have.".into());
        }
        if let Some(hz) = self.poll_rate_hz
            && !POLL_RATES_HZ.contains(&hz)
        {
            out.push(format!("{hz} Hz isn't a polling rate the keyboard supports."));
        }
        for (i, pair) in self.socd_pairs.iter().flatten().enumerate() {
            if let Some(pair) = pair
                && !socd_mode::ALL.contains(&pair.mode)
            {
                out.push(format!("Snap tap pair {} has an unknown mode.", i + 1));
            }
        }
        for (i, m) in self.macros.iter().flatten().enumerate() {
            let Some(m) = m else { continue };
            let n = i + 1;
            if !macro_play::ALL.contains(&m.play) {
                out.push(format!("Macro {n} has an unknown play mode."));
            }
            if !(1..=99).contains(&m.count) {
                out.push(format!("Macro {n} repeats {} times; it must be 1 to 99.", m.count));
            }
            if m.steps.len() > MACRO_MAX_STEPS {
                out.push(format!("Macro {n} has {} steps; the limit is {MACRO_MAX_STEPS}.", m.steps.len()));
            }
            if !is_balanced(&m.steps) {
                out.push(format!("Macro {n} has a key press with no matching release."));
            }
        }
        if let Some(light) = &self.light
            && light.direction > 5
        {
            out.push("The lighting direction is out of range.".into());
        }
        out
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Year, month and day (UTC) of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-11-24 09:31` (UTC) for a timestamp in seconds.
pub fn format_utc(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!("{y:04}-{mo:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// `20261124-093105` (UTC), for file names.
pub fn file_stamp(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!("{y:04}{mo:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// A section the keyboard didn't answer is simply left out of the profile;
/// only a real I/O failure (cable pulled) stops everything.
fn soft<T>(r: Result<Option<T>>) -> Result<Option<T>> {
    match r {
        Err(Error::Timeout(_)) => Ok(None),
        other => other,
    }
}

/// Read every setting off the keyboard.
pub fn read(kb: &mut Keyboard) -> Result<Profile> {
    let mut profile = Profile::empty();
    let status = soft(kb.read_status())?;
    let high_precision = status.as_ref().is_some_and(|s| s.support.high_precision);
    if let Some(info) = soft(kb.read_product_info())? {
        profile.product = info.product().map(str::to_owned);
        profile.firmware = info.firmware();
    }

    profile.keymap_default = soft(kb.read_keymap(Layer::Default))?;
    profile.keymap_fn = soft(kb.read_keymap(Layer::Fn))?;

    if soft(kb.read_trigger_config(high_precision))?.is_some() {
        let slots: Vec<usize> = LAYOUT.iter().map(|k| k.slot).collect();
        let read = kb.read_key_triggers(&slots)?;
        // A partial read isn't saved: restoring it would leave keys unchanged
        // without saying so.
        if read.iter().all(Option::is_some) {
            let mut all = vec![None; KEY_SLOTS];
            for (slot, t) in slots.into_iter().zip(read) {
                all[slot] = t;
            }
            profile.triggers = Some(all);
        }
    }

    profile.light = soft(kb.read_light(Zone::Keys))?;
    profile.colour_layer = soft(kb.read_colour_layer())?;
    if let Some(layer) = profile.colour_layer {
        profile.colours = soft(kb.read_colours(layer))?;
    }
    profile.poll_rate_hz = soft(kb.read_poll_rate())?;
    profile.socd_pairs = soft(kb.read_socd_pairs())?;
    profile.socd_on = soft(kb.read_socd_power())?.map(|p| p.on);
    profile.macros = soft(kb.read_macros())?;
    Ok(profile)
}

// ---------------------------------------------------------------------------
// Applying
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Written and read back identical.
    Applied,
    /// The profile has nothing for this section.
    NotInProfile,
    /// Written, but the keyboard reports something different.
    Mismatch(String),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionResult {
    pub section: Section,
    pub outcome: Outcome,
}

/// Write a profile to the keyboard, one section at a time. Returns the
/// problems found when the profile isn't safe to write, otherwise a result
/// per section. A cable pull stops the run; the sections not reached are
/// reported as failed.
pub fn apply(kb: &mut Keyboard, profile: &Profile) -> std::result::Result<Vec<SectionResult>, Vec<String>> {
    let problems = profile.problems();
    if !problems.is_empty() {
        return Err(problems);
    }
    let high_precision = soft(kb.read_status())
        .ok()
        .flatten()
        .is_some_and(|s| s.support.high_precision);

    let mut results = Vec::new();
    let mut aborted = false;
    for section in Section::ALL {
        let outcome = if !profile.has(section) {
            Outcome::NotInProfile
        } else if aborted {
            Outcome::Failed("Not attempted: the keyboard disconnected.".into())
        } else {
            match apply_section(kb, profile, section, high_precision) {
                Ok(outcome) => outcome,
                Err(e) => {
                    aborted = matches!(e, Error::Hid(_));
                    Outcome::Failed(e.to_string())
                }
            }
        };
        results.push(SectionResult { section, outcome });
    }
    Ok(results)
}

fn verdict(matches: bool, what: &str) -> Outcome {
    if matches {
        Outcome::Applied
    } else {
        Outcome::Mismatch(format!("The keyboard reports different {what} than were written."))
    }
}

fn apply_section(
    kb: &mut Keyboard,
    profile: &Profile,
    section: Section,
    high_precision: bool,
) -> Result<Outcome> {
    Ok(match section {
        Section::Macros => {
            let macros = profile.macros.as_ref().expect("checked by has()");
            match kb.write_macros(macros)? {
                Some(stored) => verdict(stored == *macros, "macros"),
                None => Outcome::Mismatch("The macros couldn't be read back.".into()),
            }
        }
        Section::KeyMapDefault | Section::KeyMapFn => {
            let (layer, keys) = if section == Section::KeyMapDefault {
                (Layer::Default, profile.keymap_default.as_ref())
            } else {
                (Layer::Fn, profile.keymap_fn.as_ref())
            };
            let keys = keys.expect("checked by has()");
            verdict(kb.write_keymap(layer, keys)?.verified, "keys")
        }
        Section::Colours => {
            let colours = profile.colours.as_ref().expect("checked by has()");
            let layer = profile.colour_layer.unwrap_or(0);
            verdict(kb.write_colours(layer, colours)?.verified, "key colours")
        }
        Section::Actuation => {
            let triggers = profile.triggers.as_ref().expect("checked by has()");
            if let Some(cfg) = soft(kb.read_trigger_config(high_precision))? {
                let bad = triggers.iter().flatten().any(|t| {
                    !(cfg.min_steps..=cfg.max_steps).contains(&t.travel)
                        || (protocol::trigger_mode::is_rapid(t.mode)
                            && !((1..=cfg.rt_max_steps()).contains(&t.press)
                                && (1..=cfg.rt_max_steps()).contains(&t.release)))
                });
                if bad {
                    return Ok(Outcome::Failed(
                        "The actuation values are outside what this keyboard accepts.".into(),
                    ));
                }
            }
            // One packet per distinct setting.
            let mut groups: Vec<(KeyTrigger, Vec<usize>)> = Vec::new();
            for key in LAYOUT {
                let Some(Some(t)) = triggers.get(key.slot) else { continue };
                match groups.iter_mut().find(|(g, _)| g == t) {
                    Some((_, slots)) => slots.push(key.slot),
                    None => groups.push((*t, vec![key.slot])),
                }
            }
            verdict(kb.write_key_triggers(&groups)?.verified, "actuation settings")
        }
        Section::SocdPairs => {
            let pairs = profile.socd_pairs.as_ref().expect("checked by has()");
            match kb.write_socd_pairs(pairs)? {
                Some(stored) => verdict(stored == *pairs, "snap tap pairs"),
                None => Outcome::Mismatch("The snap tap pairs couldn't be read back.".into()),
            }
        }
        Section::SocdPower => {
            let on = profile.socd_on.expect("checked by has()");
            verdict(kb.set_socd_power(on)?.map(|p| p.on) == Some(on), "snap tap state")
        }
        Section::PollRate => {
            let hz = profile.poll_rate_hz.expect("checked by has()");
            verdict(kb.set_poll_rate(hz)? == Some(hz), "polling rate")
        }
        Section::Lighting => {
            let light = profile.light.as_ref().expect("checked by has()");
            verdict(kb.write_light(Zone::Keys, light)?.as_ref() == Some(light), "lighting")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MacroStep;

    fn sample() -> Profile {
        let mut p = Profile::empty();
        p.saved_at = 1_700_000_000;
        p.product = Some("7153USHEXYXACPARGB".into());
        p.firmware = Some("3.17.01".into());
        p.keymap_default = Some(vec![KeyAction::key(0x04); KEY_SLOTS]);
        p.keymap_fn = Some(vec![KeyAction::default(); KEY_SLOTS]);
        let t = KeyTrigger { mode: 0x0C, travel: 170, press: 51, release: 51 };
        let mut triggers = vec![None; KEY_SLOTS];
        triggers[22] = Some(t);
        p.triggers = Some(triggers);
        p.light = Some(LightConfig { mode: 2, brightness: 4, speed: 3, direction: 2, full_color: true, ..Default::default() });
        p.colour_layer = Some(1);
        p.colours = Some(vec![[1, 2, 3]; KEY_SLOTS]);
        p.poll_rate_hz = Some(8000);
        let mut pairs = vec![None; SOCD_SLOTS];
        pairs[0] = Some(SocdPair { state: 1, mode: 0, key1: 4, key2: 7 });
        p.socd_pairs = Some(pairs);
        p.socd_on = Some(false);
        let mut macros = vec![None; MACRO_SLOTS];
        macros[0] = Some(Macro {
            play: macro_play::TIMES,
            count: 2,
            steps: vec![
                MacroStep { press: true, mouse: false, code: 4, delay_ms: 30 },
                MacroStep { press: false, mouse: false, code: 4, delay_ms: 30 },
            ],
        });
        p.macros = Some(macros);
        p
    }

    #[test]
    fn a_full_profile_survives_json_and_has_no_problems() {
        let p = sample();
        assert_eq!(p.problems(), Vec::<String>::new());
        assert_eq!(p.sections().len(), Section::ALL.len());
        let back = Profile::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn missing_sections_are_simply_absent() {
        let p = Profile::from_json(r#"{"format":1,"saved_at":5,"poll_rate_hz":2000}"#).unwrap();
        assert_eq!(p.sections(), vec![Section::PollRate]);
        assert!(p.problems().is_empty());
        assert!(Profile::from_json("not json").is_err());
        // A byte-order mark from a text editor is tolerated.
        let with_bom = format!("\u{feff}{}", sample().to_json());
        assert_eq!(Profile::from_json(&with_bom).unwrap(), sample());
    }

    #[test]
    fn the_file_holds_no_serial_number() {
        let json = sample().to_json();
        assert!(!json.to_lowercase().contains("serial"));
    }

    #[test]
    fn unsafe_profiles_are_rejected_before_anything_is_written() {
        let mut p = sample();
        p.format = PROFILE_FORMAT + 1;
        p.keymap_default = Some(vec![KeyAction::default(); 5]);
        p.poll_rate_hz = Some(500);
        p.colour_layer = Some(2);
        p.macros.as_mut().unwrap()[0].as_mut().unwrap().steps.pop(); // press without release
        p.macros.as_mut().unwrap()[1] = Some(Macro { play: 9, count: 0, steps: vec![] });
        let problems = p.problems().join("\n");
        for needle in ["format", "default key map", "500 Hz", "colour layer", "no matching release", "unknown play mode", "1 to 99"] {
            assert!(problems.contains(needle), "missing: {needle}\n{problems}");
        }
    }

    #[test]
    fn dates_are_formatted_in_utc() {
        assert_eq!(format_utc(0), "1970-01-01 00:00");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13");
        assert_eq!(file_stamp(1_700_000_000), "20231114-221320");
        // A leap day.
        assert_eq!(format_utc(1_709_164_800), "2024-02-29 00:00");
    }
}
