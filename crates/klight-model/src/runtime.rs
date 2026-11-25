//! Runtime state: what is playing right now. Published by the engine as
//! immutable snapshots and saved frequently so the supervisor can restore
//! it after a restart.

use serde::{Deserialize, Serialize};

use crate::attributes::AttributeValue;
use crate::command::OperatingMode;
use crate::function::Target;
use crate::ids::{FunctionId, UniverseId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum TempoSource {
    #[default]
    Internal,
    Tap,
    MidiClock,
    External,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TempoStatus {
    pub bpm: f64,
    pub source: TempoSource,
    /// The active source was lost: running at the last known BPM.
    pub freewheel: bool,
    pub beats_per_bar: u8,
}

/// Longest a tempo lamp stays lit on each beat.
pub const BEAT_FLASH_MAX_MS: f64 = 120.0;

/// A lamp that blinks on the beat (tap buttons, tempo LEDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BeatFlash {
    pub on: bool,
    /// The beat starts a bar (accent).
    pub down: bool,
}

/// Lamp state at musical position `beat` (in beats) for the given tempo: lit
/// at the start of every beat for a third of the beat, at most
/// [`BEAT_FLASH_MAX_MS`]. Invalid input (NaN, no tempo) keeps the lamp off.
pub fn beat_flash(beat: f64, bpm: f64, beats_per_bar: u8) -> BeatFlash {
    if !(bpm.is_finite() && bpm > 0.0 && beat.is_finite()) {
        return BeatFlash::default();
    }
    let beat_ms = 60_000.0 / bpm;
    let whole = beat.floor();
    let into_ms = (beat - whole) * beat_ms;
    let bar = f64::from(beats_per_bar.max(1));
    BeatFlash {
        on: into_ms < BEAT_FLASH_MAX_MS.min(beat_ms * 0.3),
        down: whole.rem_euclid(bar) == 0.0,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ActiveFunction {
    pub id: FunctionId,
    pub level: f32,
    /// Current cue (cue lists) or step (chasers).
    #[serde(default)]
    pub position: Option<u32>,
    #[serde(default)]
    pub paused: bool,
    /// Function played by the current step (chasers).
    #[serde(default)]
    pub step_function: Option<FunctionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProgrammerEntry {
    pub target: Target,
    pub value: AttributeValue,
}

/// A DMX channel set directly in the Simple Desk (overrides the patch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChannelOverride {
    pub universe: UniverseId,
    /// 1-based DMX address.
    pub address: u16,
    pub value: u8,
}

/// Persistable runtime state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RuntimeState {
    pub mode: OperatingMode,
    pub grand_master: f32,
    pub blackout: bool,
    pub active: Vec<ActiveFunction>,
    pub programmer: Vec<ProgrammerEntry>,
    pub tempo: TempoStatus,
    /// Raw channel values from the Simple Desk.
    #[serde(default)]
    pub channels: Vec<ChannelOverride>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            mode: OperatingMode::Design,
            grand_master: 1.0,
            blackout: false,
            active: Vec::new(),
            programmer: Vec::new(),
            tempo: TempoStatus {
                bpm: 120.0,
                source: TempoSource::Internal,
                freewheel: false,
                beats_per_bar: 4,
            },
            channels: Vec::new(),
        }
    }
}

/// Snapshot published to clients (runtime state plus diagnostics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RuntimeSnapshot {
    /// Increases on every published change.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    pub state: RuntimeState,
    pub undo_depth: u32,
    pub redo_depth: u32,
    /// Problems found while compiling the show (unresolved references,
    /// ignored attributes, conflicts).
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_flash_lights_the_start_of_each_beat() {
        // 120 BPM: a beat every 500 ms, lit for 120 ms.
        let f = |beat| beat_flash(beat, 120.0, 4);
        assert_eq!(
            f(8.0),
            BeatFlash {
                on: true,
                down: true
            }
        );
        assert!(f(8.2).on); // 100 ms in
        assert!(!f(8.3).on); // 150 ms in
        assert_eq!(
            f(9.0),
            BeatFlash {
                on: true,
                down: false
            }
        );
        assert_eq!(
            f(12.0),
            BeatFlash {
                on: true,
                down: true
            }
        );
    }

    #[test]
    fn beat_flash_is_shorter_at_fast_tempos_and_safe_on_bad_input() {
        // 300 BPM: 200 ms beat, lit for 30% = 60 ms.
        assert!(beat_flash(1.25, 300.0, 4).on); // 50 ms in
        assert!(!beat_flash(1.35, 300.0, 4).on); // 70 ms in
        assert!(!beat_flash(1.0, 0.0, 4).on);
        assert!(!beat_flash(f64::NAN, 120.0, 4).on);
        assert!(!beat_flash(1.0, f64::INFINITY, 4).on);
        // Negative positions are fine too (bar counted backwards).
        assert!(beat_flash(-4.0, 120.0, 4).down);
    }
}
