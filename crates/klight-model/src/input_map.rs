//! Global input mappings (MIDI/OSC → actions), stored in the show.

use serde::{Deserialize, Serialize};

use crate::ids::{MappingId, WidgetId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputMessage {
    /// MIDI channel is 0-based (0..=15).
    MidiNote {
        channel: u8,
        note: u8,
    },
    MidiCc {
        channel: u8,
        controller: u8,
    },
    Osc {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputTarget {
    Widget { id: WidgetId },
    GrandMaster,
    Blackout,
    TapTempo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InputMapping {
    pub id: MappingId,
    /// Device name (MIDI port name or OSC listener name). Devices are
    /// matched by name so they are recognised after hot-plug.
    pub device: String,
    pub message: InputMessage,
    pub target: InputTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputDeviceKind {
    Midi,
    Osc { port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InputDeviceConfig {
    pub name: String,
    pub kind: InputDeviceKind,
    /// Controller profile for feedback (file stem in `profiles/controllers`).
    #[serde(default)]
    pub profile: Option<String>,
}
