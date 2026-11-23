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
