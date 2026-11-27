//! Commands: the only way to change the show or its runtime state.

use serde::{Deserialize, Serialize};

use crate::attributes::AttributeValue;
use crate::fixture_def::FixtureDefinition;
use crate::function::{Function, FunctionFolder, SceneValue, Target};
use crate::ids::{
    ClientId, FixtureId, FolderId, FunctionId, GroupId, PaletteId, UniverseId, WidgetId,
};
use crate::input_map::{InputDeviceConfig, InputMapping};
use crate::palette::{Palette, PaletteKind};
use crate::patch::{FixtureGroup, OutputAssignment, PatchedFixture, Universe};
use crate::show::{Show, ShowSettings};
use crate::validate::{AddressConflict, PatchError};
use crate::vc::VirtualConsole;

/// Where a command comes from. Determines its priority.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandSource {
    LocalUi,
    Remote {
        client: ClientId,
    },
    Midi {
        device: String,
    },
    Osc {
        device: String,
    },
    /// External tempo source (MIDI clock, Ableton Link bridge, ...).
    TimeSource {
        name: String,
    },
    /// Automatic programming (future music sync); lowest priority.
    Automation {
        name: String,
    },
    Internal,
}

impl CommandSource {
    /// Higher wins. Operators always beat automation.
    pub fn priority(&self) -> u8 {
        match self {
            CommandSource::Internal => 255,
            CommandSource::LocalUi
            | CommandSource::Remote { .. }
            | CommandSource::Midi { .. }
            | CommandSource::Osc { .. } => 200,
            CommandSource::TimeSource { .. } => 100,
            CommandSource::Automation { .. } => 10,
        }
    }

    pub fn is_operator(&self) -> bool {
        self.priority() == 200
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, CommandSource::Remote { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum OperatingMode {
    /// Structure can be edited.
    #[default]
    Design,
    /// Structural edits are rejected.
    Show,
}

/// Controls that operators and automation may compete for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum ControlKey {
    GrandMaster,
    Blackout,
    Function(FunctionId),
    Widget(WidgetId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // ---- Show structure (undoable, rejected in Show mode) ----
    LoadShow {
        show: Box<Show>,
    },
    SetShowName {
        name: String,
    },
    SetShowSettings {
        settings: ShowSettings,
    },
    AddUniverse {
        universe: Universe,
    },
    RenameUniverse {
        id: UniverseId,
        name: String,
    },
    /// Removes the universe and every fixture patched in it.
    RemoveUniverse {
        id: UniverseId,
    },
    SetUniverseOutput {
        id: UniverseId,
        output: Option<OutputAssignment>,
    },
    ImportDefinition {
        definition: Box<FixtureDefinition>,
    },
    /// Adds or replaces a fixture. Address conflicts are rejected unless `force`.
    PatchFixture {
        fixture: PatchedFixture,
        #[serde(default)]
        force: bool,
    },
    UnpatchFixture {
        id: FixtureId,
    },
    SetGroup {
        group: FixtureGroup,
    },
    RemoveGroup {
        id: GroupId,
    },
    SetPalette {
        palette: Palette,
    },
    RemovePalette {
        id: PaletteId,
    },
    SetFunction {
        function: Function,
    },
    /// Creates, renames or moves a folder of functions.
    SetFunctionFolder {
        folder: FunctionFolder,
    },
    /// Deletes a folder; its functions and subfolders move to its parent.
    RemoveFunctionFolder {
        id: FolderId,
    },
    RemoveFunction {
        id: FunctionId,
    },
    SetVirtualConsole {
        console: VirtualConsole,
    },
    SetInputDevices {
        devices: Vec<InputDeviceConfig>,
    },
    SetInputMappings {
        mappings: Vec<InputMapping>,
    },
    /// Saves the programmer contents into a scene (new or replacing `id`).
    StoreProgrammerAsScene {
        id: Option<FunctionId>,
        name: String,
    },
    /// Updates (or creates) a palette from the programmer values of that kind.
    StoreProgrammerInPalette {
        id: Option<PaletteId>,
        name: String,
        kind: PaletteKind,
    },
    Undo,
    Redo,

    // ---- Runtime (allowed in Show mode) ----
    StartFunction {
        id: FunctionId,
    },
    StopFunction {
        id: FunctionId,
    },
    StopAll,
    /// Playback level of a function (scales its intensity), `0..=1`.
    SetFunctionLevel {
        id: FunctionId,
        level: f32,
    },
    CueGo {
        id: FunctionId,
    },
    CueBack {
        id: FunctionId,
    },
    CuePause {
        id: FunctionId,
    },
    CueGoTo {
        id: FunctionId,
        index: u32,
    },
    /// Jumps a chaser to one of its steps (its place in the show's list),
    /// starting it if it is not running. The step fades in with its own fade
    /// and lasts its full hold from now.
    ChaserGoTo {
        id: FunctionId,
        index: u32,
    },
    ProgrammerSet {
        target: Target,
        value: AttributeValue,
    },
    /// Clears the programmer: intentions and raw channels.
    ProgrammerClear,
    /// Shows scene values on the lights, on top of everything but grand master,
    /// blackout and the raw desk channels, without touching the show: an editor
    /// trying them out. `members` are the lights of the scene. It ends by itself
    /// unless sent again (see `PREVIEW_HOLD_US` in the engine).
    SetPreview {
        values: Vec<SceneValue>,
        members: Vec<Target>,
    },
    ClearPreview,
    /// Simple Desk: sets one DMX channel directly (`None` releases it).
    /// Intensity channels stay subject to grand master and blackout.
    SetChannel {
        universe: UniverseId,
        /// 1-based DMX address.
        address: u16,
        value: Option<u8>,
    },
    /// Releases the raw channels of one universe (or all with `None`).
    ClearChannels {
        universe: Option<UniverseId>,
    },
    SetGrandMaster {
        level: f32,
    },
    SetBlackout {
        on: bool,
    },
    /// Operator releases a control: automation may drive it again.
    ReleaseControl {
        control: ControlKey,
    },
    TapTempo,
    SetBpm {
        bpm: f64,
    },
    /// MIDI clock pulse (24 per beat) at the given engine time.
    ClockPulse {
        at_us: u64,
    },
    /// External tempo source report (e.g. Link bridge): tempo and beat phase.
    ExternalTempo {
        name: String,
        bpm: f64,
        beat: f64,
        at_us: u64,
    },
    WidgetPress {
        id: WidgetId,
    },
    WidgetRelease {
        id: WidgetId,
    },
    WidgetSetValue {
        id: WidgetId,
        value: f32,
    },
    /// Switching to Design from a remote client requires the PIN.
    SetMode {
        mode: OperatingMode,
        #[serde(default)]
        pin: Option<String>,
    },
}

impl Command {
    /// Structural edits change the show and are undoable.
    pub fn is_edit(&self) -> bool {
        matches!(
            self,
            Command::LoadShow { .. }
                | Command::SetShowName { .. }
                | Command::SetShowSettings { .. }
                | Command::AddUniverse { .. }
                | Command::RemoveUniverse { .. }
                | Command::RenameUniverse { .. }
                | Command::SetUniverseOutput { .. }
                | Command::ImportDefinition { .. }
                | Command::PatchFixture { .. }
                | Command::UnpatchFixture { .. }
                | Command::SetGroup { .. }
                | Command::RemoveGroup { .. }
                | Command::SetPalette { .. }
                | Command::RemovePalette { .. }
                | Command::SetFunction { .. }
                | Command::RemoveFunction { .. }
                | Command::SetFunctionFolder { .. }
                | Command::RemoveFunctionFolder { .. }
                | Command::SetVirtualConsole { .. }
                | Command::SetInputDevices { .. }
                | Command::SetInputMappings { .. }
                | Command::StoreProgrammerAsScene { .. }
                | Command::StoreProgrammerInPalette { .. }
                | Command::Undo
                | Command::Redo
        )
    }

    /// Commands that only affect playback (allowed for "playback only" clients).
    pub fn is_playback(&self) -> bool {
        !self.is_edit()
            && !matches!(
                self,
                Command::SetMode { .. } | Command::SetPreview { .. } | Command::ClearPreview
            )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CommandEnvelope {
    /// Client-chosen id echoed in the result.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: u64,
    pub source: CommandSource,
    pub command: Command,
}

/// Typed command failure. Never a panic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandError {
    #[error("structural edits are not allowed in Show mode")]
    ShowModeLocked,
    #[error("wrong or missing PIN")]
    BadPin,
    #[error("not permitted for this client")]
    NotPermitted,
    #[error("{what} not found")]
    NotFound { what: String },
    #[error("invalid value: {reason}")]
    InvalidValue { reason: String },
    #[error("patch error: {error}")]
    Patch { error: PatchError },
    #[error("address conflicts")]
    AddressConflicts { conflicts: Vec<AddressConflict> },
    #[error("control is held by a higher priority source")]
    Overridden,
    #[error("nothing to undo/redo")]
    NothingToUndo,
    #[error("command queue is full")]
    QueueFull,
}

impl CommandError {
    pub fn not_found(what: impl std::fmt::Display) -> Self {
        CommandError::NotFound {
            what: what.to_string(),
        }
    }
    pub fn invalid(reason: impl Into<String>) -> Self {
        CommandError::InvalidValue {
            reason: reason.into(),
        }
    }
}

/// Non-fatal notice attached to an accepted command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CommandOutcome {
    pub warnings: Vec<String>,
    /// Show version that includes this command. Clients wait until they have
    /// seen it before acting on the show again (no stale ids or addresses).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub show_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CommandResult {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: u64,
    pub source: CommandSource,
    pub result: Result<CommandOutcome, CommandError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_beats_automation() {
        let op = CommandSource::LocalUi;
        let auto = CommandSource::Automation {
            name: "sync".into(),
        };
        assert!(op.priority() > auto.priority());
        assert!(op.is_operator());
        assert!(!auto.is_operator());
    }

    #[test]
    fn command_json_roundtrip() {
        let c = Command::SetGrandMaster { level: 0.5 };
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(s, r#"{"type":"set_grand_master","level":0.5}"#);
        assert_eq!(serde_json::from_str::<Command>(&s).unwrap(), c);
    }
}
