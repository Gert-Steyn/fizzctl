//! Turns a stream of key-down and key-up events into macro steps.
//!
//! The recorder knows nothing about egui or the keyboard: callers feed it
//! HID usage codes with a timestamp in milliseconds. It appends steps to the
//! macro being edited and keeps the macro valid at every moment, so a
//! recording can be stopped at any time:
//! - every press has a release later in the list (keys still held when
//!   recording ends are released at that moment);
//! - the 40-step limit is never exceeded, including the releases still owed.

use crate::protocol::{MACRO_MAX_STEPS, MacroStep};

/// Wait after a step when nothing better is known (the vendor app's default).
pub const DEFAULT_DELAY_MS: u16 = 50;
/// Longest delay a step can store sensibly.
pub const MAX_DELAY_MS: u16 = 60_000;

/// How delays are filled in while recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// Keep the pauses the user made between events.
    Recorded,
    /// Wait the same number of milliseconds after every step.
    Fixed(u16),
}

/// True when every press is released later and no release comes first.
pub fn is_balanced(steps: &[MacroStep]) -> bool {
    let mut held: Vec<(bool, u8)> = Vec::new();
    for s in steps {
        let id = (s.mouse, s.code);
        if s.press {
            held.push(id);
        } else if let Some(i) = held.iter().position(|h| *h == id) {
            held.remove(i);
        } else {
            return false;
        }
    }
    held.is_empty()
}

/// State of one recording session. The steps themselves live in the macro;
/// they are passed to each call.
#[derive(Clone, Debug)]
pub struct Recorder {
    timing: Timing,
    /// Number of steps that were already there when recording started.
    base_len: usize,
    /// Keyboard keys currently down, in the order they were pressed.
    held: Vec<u8>,
    last_ms: Option<f64>,
    full: bool,
}

impl Recorder {
    /// Start recording after the first `base_len` steps.
    pub fn new(base_len: usize, timing: Timing) -> Self {
        Self {
            timing,
            base_len,
            held: Vec::new(),
            last_ms: None,
            full: false,
        }
    }

    /// The step limit was reached, so further presses are being ignored.
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// How many steps this session has added to `steps`.
    pub fn recorded(&self, steps: &[MacroStep]) -> usize {
        steps.len().saturating_sub(self.base_len)
    }

    /// Time of the latest event, if any.
    pub fn last_event_ms(&self) -> Option<f64> {
        self.last_ms
    }

    fn new_delay(&self) -> u16 {
        match self.timing {
            Timing::Fixed(ms) => ms.min(MAX_DELAY_MS),
            Timing::Recorded => DEFAULT_DELAY_MS,
        }
    }

    /// Record the pause since the previous event on the step before this one.
    fn stamp(&mut self, steps: &mut [MacroStep], at_ms: f64) {
        if self.timing == Timing::Recorded
            && let Some(prev) = self.last_ms
            && steps.len() > self.base_len
            && let Some(last) = steps.last_mut()
        {
            let gap = (at_ms - prev).round().clamp(0.0, MAX_DELAY_MS as f64);
            last.delay_ms = gap as u16;
        }
        self.last_ms = Some(at_ms);
    }

    /// A key went down. Repeats of a key already down are ignored.
    pub fn key_down(&mut self, steps: &mut Vec<MacroStep>, code: u8, at_ms: f64) {
        if self.held.contains(&code) {
            return;
        }
        // Room for this press, its release, and the releases already owed.
        if steps.len() + self.held.len() + 2 > MACRO_MAX_STEPS {
            self.full = true;
            return;
        }
        self.stamp(steps, at_ms);
        self.held.push(code);
        steps.push(MacroStep {
            press: true,
            mouse: false,
            code,
            delay_ms: self.new_delay(),
        });
    }

    /// A key came up. Releases of keys that weren't recorded going down are ignored.
    pub fn key_up(&mut self, steps: &mut Vec<MacroStep>, code: u8, at_ms: f64) {
        let Some(i) = self.held.iter().position(|h| *h == code) else {
            return;
        };
        self.stamp(steps, at_ms);
        self.held.remove(i);
        steps.push(MacroStep {
            press: false,
            mouse: false,
            code,
            delay_ms: self.new_delay(),
        });
    }

    /// End the session: keys still down are released, most recent first, at
    /// `at_ms` (or at the last event when `None`).
    pub fn finish(&mut self, steps: &mut Vec<MacroStep>, at_ms: Option<f64>) {
        let at = at_ms.or(self.last_ms).unwrap_or(0.0);
        while let Some(&code) = self.held.last() {
            self.key_up(steps, code, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u8 = 0x04;
    const B: u8 = 0x05;
    const SHIFT: u8 = 0xE1;

    fn step(press: bool, code: u8, delay_ms: u16) -> MacroStep {
        MacroStep { press, mouse: false, code, delay_ms }
    }

    #[test]
    fn recorded_timing_stores_the_pause_before_the_next_event() {
        let mut steps = Vec::new();
        let mut r = Recorder::new(0, Timing::Recorded);
        r.key_down(&mut steps, A, 1000.0);
        r.key_up(&mut steps, A, 1080.0);
        r.key_down(&mut steps, B, 1400.0);
        r.key_up(&mut steps, B, 1500.0);
        assert_eq!(
            steps,
            vec![
                step(true, A, 80),
                step(false, A, 320),
                step(true, B, 100),
                // Nothing follows the last step, so it keeps the default.
                step(false, B, DEFAULT_DELAY_MS),
            ]
        );
        assert!(is_balanced(&steps));
    }

    #[test]
    fn fixed_timing_ignores_the_clock() {
        let mut steps = Vec::new();
        let mut r = Recorder::new(0, Timing::Fixed(25));
        r.key_down(&mut steps, A, 0.0);
        r.key_up(&mut steps, A, 5000.0);
        assert!(steps.iter().all(|s| s.delay_ms == 25));
    }

    #[test]
    fn keys_still_held_are_released_when_recording_ends() {
        let mut steps = Vec::new();
        let mut r = Recorder::new(0, Timing::Recorded);
        r.key_down(&mut steps, SHIFT, 0.0);
        r.key_down(&mut steps, A, 100.0);
        r.finish(&mut steps, Some(300.0));
        assert_eq!(
            steps,
            vec![
                step(true, SHIFT, 100),
                step(true, A, 200),
                step(false, A, 0),
                step(false, SHIFT, DEFAULT_DELAY_MS),
            ]
        );
        assert!(is_balanced(&steps));
    }

    #[test]
    fn repeats_and_stray_releases_are_ignored() {
        let mut steps = Vec::new();
        let mut r = Recorder::new(0, Timing::Recorded);
        r.key_up(&mut steps, A, 0.0); // never went down
        r.key_down(&mut steps, A, 10.0);
        r.key_down(&mut steps, A, 20.0); // auto-repeat
        r.key_up(&mut steps, A, 30.0);
        r.key_up(&mut steps, A, 40.0); // second release
        assert_eq!(steps, vec![step(true, A, 20), step(false, A, DEFAULT_DELAY_MS)]);
    }

    #[test]
    fn recording_appends_without_touching_existing_steps() {
        let mut steps = vec![step(true, B, 7), step(false, B, 9)];
        let mut r = Recorder::new(2, Timing::Recorded);
        r.key_down(&mut steps, A, 500.0);
        r.key_up(&mut steps, A, 560.0);
        assert_eq!(&steps[..2], &[step(true, B, 7), step(false, B, 9)]);
        assert_eq!(steps[2], step(true, A, 60));
        assert_eq!(r.recorded(&steps), 2);
    }

    #[test]
    fn the_step_limit_is_never_exceeded_and_nothing_is_left_held() {
        // Fill up with taps, then try to go on.
        let mut steps = Vec::new();
        let mut r = Recorder::new(0, Timing::Recorded);
        for i in 0..30 {
            let t = i as f64 * 10.0;
            r.key_down(&mut steps, A, t);
            r.key_up(&mut steps, A, t + 5.0);
        }
        assert_eq!(steps.len(), MACRO_MAX_STEPS);
        assert!(r.is_full());
        assert!(is_balanced(&steps));

        // A key held across the limit still gets its release.
        let mut steps = vec![step(true, B, 0), step(false, B, 0)]; // 2 existing
        steps.extend((0..17).flat_map(|_| [step(true, B, 0), step(false, B, 0)])); // 36 total
        assert_eq!(steps.len(), 36);
        let mut r = Recorder::new(36, Timing::Recorded);
        r.key_down(&mut steps, SHIFT, 0.0); // 36 + 0 + 2 = 38 fits
        r.key_down(&mut steps, A, 10.0); // 37 + 1 + 2 = 40 fits
        r.key_down(&mut steps, B, 20.0); // 38 + 2 + 2 = 42 does not
        assert!(r.is_full());
        r.finish(&mut steps, Some(30.0));
        assert_eq!(steps.len(), 40);
        assert!(is_balanced(&steps));
    }

    #[test]
    fn balance_check_rejects_missing_and_early_releases() {
        assert!(is_balanced(&[]));
        assert!(!is_balanced(&[step(true, A, 0)]));
        assert!(!is_balanced(&[step(false, A, 0), step(true, A, 0)]));
        // Mouse button 4 is not keyboard key 4.
        let click = MacroStep { press: true, mouse: true, code: 0x04, delay_ms: 0 };
        assert!(!is_balanced(&[click, step(false, 0x04, 0)]));
    }
}
