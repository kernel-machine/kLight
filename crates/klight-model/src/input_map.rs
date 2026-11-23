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

/// What a controller LED does in one widget state, chosen by the user among
/// the behaviours of the controller profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LedBehavior {
    /// LED off.
    Off,
    /// Profile behaviour identified by its MIDI feedback channel (0-based),
    /// e.g. "On 100% brightness" or "Pulsing 1/4" on the Akai APC mini mk2.
    Channel { channel: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InputDeviceConfig {
    pub name: String,
    pub kind: InputDeviceKind,
    /// Controller profile for feedback: file stem in `profiles/controllers` (TOML)
    /// or `profiles/qlcplus` (QLC+ `.qxi`).
    #[serde(default)]
    pub profile: Option<String>,
    /// LED while the widget is on (its function runs); `None` = profile default.
    #[serde(default)]
    pub feedback_active: Option<LedBehavior>,
    /// LED while the widget is off; `None` = off.
    #[serde(default)]
    pub feedback_inactive: Option<LedBehavior>,
}
