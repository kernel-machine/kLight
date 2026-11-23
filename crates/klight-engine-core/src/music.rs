//! Musical clock: BPM, beat, bar and phase, with exact integer arithmetic.
//!
//! Beat positions are fixed point (32 fractional bits). The clock is
//! anchored at `(anchor_us, anchor_beat)`; changing the BPM re-anchors at the
//! current position, so the phase never jumps. If the active external
//! source disappears, the clock freewheels at the last known BPM.

use klight_model::{TempoSource, TempoStatus};

/// One beat in fixed point.
pub const BEAT: u64 = 1 << 32;

const MIN_BPM_MILLI: u32 = 20_000;
const MAX_BPM_MILLI: u32 = 400_000;
/// A tap after this gap starts a new tap sequence.
const TAP_RESET_US: u64 = 2_000_000;
/// External sources silent for longer than this are considered lost.
const SOURCE_TIMEOUT_US: u64 = 1_000_000;
const MIDI_PPQ: usize = 24;

#[derive(Debug, Clone)]
pub struct MusicClock {
    bpm_milli: u32,
    anchor_us: u64,
    anchor_beat: u64,
    source: TempoSource,
    freewheel: bool,
    last_source_us: u64,
    taps: [u64; 8],
    tap_len: usize,
    pulses: [u64; MIDI_PPQ + 1],
    pulse_len: usize,
    beats_per_bar: u8,
}

impl Default for MusicClock {
    fn default() -> Self {
        Self::new(120.0)
    }
}

/// Converts a BPM to milli-BPM, clamped to the supported range.
pub fn bpm_to_milli(bpm: f64) -> u32 {
    if !bpm.is_finite() {
        return 120_000;
    }
    let m = (bpm * 1000.0).round();
    if m <= f64::from(MIN_BPM_MILLI) {
        MIN_BPM_MILLI
    } else if m >= f64::from(MAX_BPM_MILLI) {
        MAX_BPM_MILLI
    } else {
        m as u32
    }
}

/// Converts a duration in beats to fixed point.
pub fn beats_to_fixed(beats: f32) -> u64 {
    if !beats.is_finite() || beats <= 0.0 {
        return 0;
    }
    let v = f64::from(beats) * BEAT as f64;
    if v >= u64::MAX as f64 {
        u64::MAX
    } else {
        v.round() as u64
    }
}

impl MusicClock {
    pub fn new(bpm: f64) -> Self {
        Self {
            bpm_milli: bpm_to_milli(bpm),
            anchor_us: 0,
            anchor_beat: 0,
            source: TempoSource::Internal,
            freewheel: false,
            last_source_us: 0,
            taps: [0; 8],
            tap_len: 0,
            pulses: [0; MIDI_PPQ + 1],
            pulse_len: 0,
            beats_per_bar: 4,
        }
    }

    pub fn bpm(&self) -> f64 {
        f64::from(self.bpm_milli) / 1000.0
    }

    /// Beat position at `t_us` (fixed point). Times before the anchor clamp to it.
    pub fn beat_at(&self, t_us: u64) -> u64 {
        let dt = u128::from(t_us.saturating_sub(self.anchor_us));
        // beats = dt * bpm_milli / 60e9 ; fixed = beats << 32
        let num = (dt * u128::from(self.bpm_milli)) << 32;
        let delta = num / 60_000_000_000u128;
        self.anchor_beat
            .saturating_add(u64::try_from(delta).unwrap_or(u64::MAX))
    }

    /// Microseconds corresponding to `beats` (fixed point) at the current tempo.
    pub fn beats_to_us(&self, beats: u64) -> u64 {
        let us = (u128::from(beats) * 60_000_000_000u128 / u128::from(self.bpm_milli.max(1))) >> 32;
        u64::try_from(us).unwrap_or(u64::MAX)
    }

    /// Sets the tempo without a phase jump.
    pub fn set_bpm(&mut self, now_us: u64, bpm_milli: u32) {
        let bpm_milli = bpm_milli.clamp(MIN_BPM_MILLI, MAX_BPM_MILLI);
        self.anchor_beat = self.beat_at(now_us);
        self.anchor_us = now_us.max(self.anchor_us);
        self.bpm_milli = bpm_milli;
    }

    pub fn set_manual_bpm(&mut self, now_us: u64, bpm: f64) {
        self.set_bpm(now_us, bpm_to_milli(bpm));
        self.source = TempoSource::Internal;
        self.freewheel = false;
    }

    /// Registers a tap. From the second tap the tempo follows the average
    /// interval and the phase is aligned so that the tap falls on a beat.
    pub fn tap(&mut self, now_us: u64) {
        let last = self
            .tap_len
            .checked_sub(1)
            .and_then(|i| self.taps.get(i))
            .copied();
        if last.is_none_or(|l| now_us.saturating_sub(l) > TAP_RESET_US || now_us < l) {
            self.tap_len = 0;
        }
        if self.tap_len == self.taps.len() {
            self.taps.rotate_left(1);
            self.tap_len -= 1;
        }
        if let Some(slot) = self.taps.get_mut(self.tap_len) {
            *slot = now_us;
            self.tap_len += 1;
        }
        if self.tap_len >= 2 {
            let first = self.taps.first().copied().unwrap_or(now_us);
            let intervals = (self.tap_len - 1) as u64;
            let avg = now_us.saturating_sub(first) / intervals.max(1);
            if avg > 0 {
                let milli = (60_000_000_000u64 + avg / 2) / avg;
                self.set_bpm(now_us, u32::try_from(milli).unwrap_or(MAX_BPM_MILLI));
            }
            // Align phase: the tap is a beat.
            let b = self.beat_at(now_us);
            let rounded = (b + BEAT / 2) & !(BEAT - 1);
            self.anchor_beat = rounded;
            self.anchor_us = now_us;
            self.source = TempoSource::Tap;
            self.freewheel = false;
        }
    }

    /// MIDI clock pulse (24 per quarter note).
    pub fn clock_pulse(&mut self, at_us: u64) {
        let last = self
            .pulse_len
            .checked_sub(1)
            .and_then(|i| self.pulses.get(i))
            .copied();
        if last.is_some_and(|l| at_us < l || at_us - l > SOURCE_TIMEOUT_US) {
            self.pulse_len = 0;
        }
        if self.pulse_len == self.pulses.len() {
            self.pulses.rotate_left(1);
            self.pulse_len -= 1;
        }
        if let Some(slot) = self.pulses.get_mut(self.pulse_len) {
            *slot = at_us;
            self.pulse_len += 1;
        }
        self.last_source_us = at_us;
        if self.pulse_len >= MIDI_PPQ / 2 {
            let first = self.pulses.first().copied().unwrap_or(at_us);
            let n = (self.pulse_len - 1) as u64;
            let span = at_us.saturating_sub(first);
            if span > 0 {
                // bpm = 60e6 * n / (span * 24)
                let milli = (60_000_000_000u64 * n + span * 12) / (span * MIDI_PPQ as u64);
                let milli = u32::try_from(milli).unwrap_or(MAX_BPM_MILLI);
                // Ignore jitter below 0.1 BPM to keep the phase stable.
                if milli.abs_diff(self.bpm_milli) >= 100 || self.source != TempoSource::MidiClock {
                    self.set_bpm(at_us, milli);
                }
            }
            self.source = TempoSource::MidiClock;
            self.freewheel = false;
        }
    }

    /// External tempo report (e.g. an Ableton Link bridge process).
    pub fn external(&mut self, now_us: u64, bpm: f64, beat: f64) {
        self.bpm_milli = bpm_to_milli(bpm);
        self.anchor_us = now_us;
        self.anchor_beat = if beat.is_finite() && beat >= 0.0 {
            (beat * BEAT as f64) as u64
        } else {
            self.beat_at(now_us)
        };
        self.source = TempoSource::External;
        self.freewheel = false;
        self.last_source_us = now_us;
    }

    /// Detects lost sources. Returns true if the status changed.
    pub fn update(&mut self, now_us: u64) -> bool {
        let external = matches!(self.source, TempoSource::MidiClock | TempoSource::External);
        if external
            && !self.freewheel
            && now_us.saturating_sub(self.last_source_us) > SOURCE_TIMEOUT_US
        {
            self.freewheel = true;
            self.pulse_len = 0;
            return true;
        }
        false
    }

    pub fn status(&self) -> TempoStatus {
        TempoStatus {
            bpm: self.bpm(),
            source: self.source,
            freewheel: self.freewheel,
            beats_per_bar: self.beats_per_bar,
        }
    }

    /// Restores a persisted status (phase restarts at `now_us`).
    pub fn restore(&mut self, now_us: u64, status: &TempoStatus) {
        self.set_bpm(now_us, bpm_to_milli(status.bpm));
        self.beats_per_bar = status.beats_per_bar.max(1);
        self.source = status.source;
        self.freewheel = matches!(
            status.source,
            TempoSource::MidiClock | TempoSource::External
        );
        self.last_source_us = now_us;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_beats_at_120() {
        let c = MusicClock::new(120.0);
        assert_eq!(c.beat_at(500_000), BEAT);
        assert_eq!(c.beat_at(1_000_000), 2 * BEAT);
        assert_eq!(c.beats_to_us(BEAT), 500_000);
    }

    #[test]
    fn bpm_change_keeps_phase() {
        let mut c = MusicClock::new(120.0);
        let before = c.beat_at(750_000);
        c.set_bpm(750_000, 90_000);
        assert_eq!(c.beat_at(750_000), before);
        // 90 BPM: one beat lasts 666_666.67 us.
        let after = c.beat_at(750_000 + 666_667);
        assert!(after.abs_diff(before + BEAT) < BEAT / 100_000);
    }

    #[test]
    fn tap_tempo_sets_bpm_and_phase() {
        let mut c = MusicClock::new(90.0);
        for i in 0..4u64 {
            c.tap(1_000_000 + i * 400_000); // 150 BPM
        }
        assert!((c.bpm() - 150.0).abs() < 0.01);
        assert_eq!(c.beat_at(1_000_000 + 3 * 400_000) % BEAT, 0);
        assert_eq!(c.status().source, TempoSource::Tap);
    }

    #[test]
    fn midi_clock_then_freewheel() {
        let mut c = MusicClock::new(100.0);
        // 128 BPM: pulse every 60e6 / (128*24) us
        let period = 60_000_000.0 / (128.0 * 24.0);
        for i in 0..48 {
            c.clock_pulse((i as f64 * period) as u64);
        }
        assert!((c.bpm() - 128.0).abs() < 0.2, "{}", c.bpm());
        assert!(!c.update(1_000_000));
        assert!(c.update(3_000_000));
        let s = c.status();
        assert!(s.freewheel);
        assert!((s.bpm - 128.0).abs() < 0.2);
    }
}
