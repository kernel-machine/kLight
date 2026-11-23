//! Typed JSON messages between clients and the engine.
//!
//! After `welcome`, every server message carries a per-connection `seq`.
//! Messages may be dropped for slow clients; a gap in `seq` means the client
//! must send `request_snapshot`.

use serde::{Deserialize, Serialize};

use klight_model::input_map::InputMessage;
use klight_model::{
    ClientId, Command, CommandError, FixtureDefinition, RuntimeSnapshot, Show, UniverseId,
};

pub const PROTOCOL_VERSION: u32 = 1;

/// Fixture library available for patching (sorted by manufacturer, then model).
pub type Library = Vec<FixtureDefinition>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LibraryMode {
    pub name: String,
    pub channels: u16,
    pub heads: u16,
}

/// Lightweight library entry (`GET /api/library`); the full definition is
/// fetched with `GET /api/library/definition?key=<manufacturer/model>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LibraryEntry {
    pub key: String,
    pub manufacturer: String,
    pub model: String,
    pub kind: klight_model::FixtureKind,
    pub modes: Vec<LibraryMode>,
}

impl LibraryEntry {
    pub fn of(d: &FixtureDefinition) -> Self {
        Self {
            key: d.key().0,
            manufacturer: d.manufacturer.clone(),
            model: d.model.clone(),
            kind: d.kind,
            modes: d
                .modes
                .iter()
                .map(|m| LibraryMode {
                    name: m.name.clone(),
                    channels: m.footprint(),
                    heads: u16::try_from(m.heads.len().max(1)).unwrap_or(u16::MAX),
                })
                .collect(),
        }
    }
}

/// Sorts definitions by manufacturer, then model (case-insensitive).
pub fn sort_library(lib: &mut Library) {
    lib.sort_by_cached_key(|d| (d.manufacturer.to_lowercase(), d.model.to_lowercase()));
}
/// Largest accepted client message.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Everything, including structural edits.
    Full,
    /// Playback commands only.
    PlaybackOnly,
    ReadOnly,
}

impl Permission {
    pub fn allows(self, cmd: &Command) -> bool {
        match self {
            Permission::Full => true,
            Permission::PlaybackOnly => cmd.is_playback(),
            Permission::ReadOnly => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message. Remote clients must present a token.
    Hello {
        name: String,
        #[serde(default)]
        token: Option<String>,
    },
    Command {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        id: u64,
        command: Command,
    },
    RequestSnapshot,
    /// DMX monitor: universes whose frames should be streamed.
    SubscribeDmx {
        universes: Vec<UniverseId>,
    },
    /// Remote MIDI bridge: raw bytes received by a controller on the client.
    MidiIn {
        device: String,
        bytes: Vec<u8>,
    },
    /// MIDI learn: the next input message received by the core is reported back.
    MidiLearn,
    Ping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct JitterInfo {
    pub min_us: u32,
    pub avg_us: u32,
    pub max_us: u32,
    pub p99_us: u32,
    pub work_us: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ticks: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub overruns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OutputInfo {
    pub universe: UniverseId,
    pub kind: String,
    /// "ok", "connecting", "degraded", "disconnected".
    pub state: String,
    pub port: Option<String>,
    pub serial_number: Option<String>,
    /// False for Open DMX: output stops if the engine stops.
    pub hardware_buffer: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub frames_sent: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub errors: u64,
    pub last_error: Option<String>,
}

/// A serial port that may be a DMX interface (`GET /api/ports`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SerialPortInfo {
    pub port: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
    pub is_ftdi: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InputInfo {
    pub name: String,
    pub kind: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SystemStatus {
    pub jitter: JitterInfo,
    pub outputs: Vec<OutputInfo>,
    pub inputs: Vec<InputInfo>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_autosave_ms: Option<u64>,
    pub autosave_error: Option<String>,
    pub clients: u32,
    /// Current musical beat position.
    pub beat: f64,
    pub show_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        client: ClientId,
        permission: Permission,
        protocol: u32,
        server: String,
    },
    /// Full state: sent on connection and on request.
    Snapshot {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        show_seq: u64,
        show: Box<Show>,
        runtime: Box<RuntimeSnapshot>,
        status: SystemStatus,
    },
    ShowChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        show_seq: u64,
        show: Box<Show>,
    },
    Runtime {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        runtime: RuntimeSnapshot,
    },
    Status {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        status: SystemStatus,
    },
    Dmx {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        universe: UniverseId,
        data: Vec<u8>,
    },
    Ack {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        id: u64,
        warnings: Vec<String>,
        /// Show version including the command (see `CommandOutcome::show_seq`).
        #[serde(default)]
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        show_seq: u64,
    },
    Error {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        #[cfg_attr(feature = "ts", ts(type = "number | null"))]
        id: Option<u64>,
        error: ApiError,
    },
    /// Feedback for a controller attached to this client (remote MIDI bridge).
    MidiOut {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        device: String,
        bytes: Vec<u8>,
    },
    /// Answer to `midi_learn`.
    Learned {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
        device: String,
        message: InputMessage,
    },
    Pong {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        seq: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    #[error("{error}")]
    Command { error: CommandError },
    #[error("malformed message: {reason}")]
    Malformed { reason: String },
    #[error("authentication required or invalid token")]
    Unauthorized,
    #[error("not permitted for this client")]
    Forbidden,
    #[error("engine queue full, retry")]
    Busy,
}

impl ServerMessage {
    /// Sets the per-connection sequence number.
    pub fn set_seq(&mut self, s: u64) {
        match self {
            ServerMessage::Welcome { .. } => {}
            ServerMessage::Snapshot { seq, .. }
            | ServerMessage::ShowChanged { seq, .. }
            | ServerMessage::Runtime { seq, .. }
            | ServerMessage::Status { seq, .. }
            | ServerMessage::Dmx { seq, .. }
            | ServerMessage::Ack { seq, .. }
            | ServerMessage::Error { seq, .. }
            | ServerMessage::MidiOut { seq, .. }
            | ServerMessage::Learned { seq, .. }
            | ServerMessage::Pong { seq } => *seq = s,
        }
    }

    /// Messages that may be dropped for slow clients (the client resyncs).
    pub fn droppable(&self) -> bool {
        matches!(
            self,
            ServerMessage::Runtime { .. }
                | ServerMessage::Status { .. }
                | ServerMessage::Dmx { .. }
                | ServerMessage::MidiOut { .. }
        )
    }
}

/// Parses a client message. Never panics on any input.
pub fn parse_client_message(text: &str) -> Result<ClientMessage, ApiError> {
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(ApiError::Malformed {
            reason: "message too large".into(),
        });
    }
    serde_json::from_str(text).map_err(|e| ApiError::Malformed {
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_message() {
        let m = parse_client_message(
            r#"{"type":"command","id":3,"command":{"type":"set_blackout","on":true}}"#,
        );
        assert_eq!(
            m,
            Ok(ClientMessage::Command {
                id: 3,
                command: Command::SetBlackout { on: true }
            })
        );
        assert!(parse_client_message("{").is_err());
        assert!(parse_client_message(r#"{"type":"nope"}"#).is_err());
    }

    #[test]
    fn permissions() {
        let edit = Command::SetShowName { name: "x".into() };
        let play = Command::SetBlackout { on: true };
        assert!(Permission::Full.allows(&edit));
        assert!(!Permission::PlaybackOnly.allows(&edit));
        assert!(Permission::PlaybackOnly.allows(&play));
        assert!(!Permission::ReadOnly.allows(&play));
        assert!(!Permission::PlaybackOnly.allows(&Command::SetMode {
            mode: klight_model::OperatingMode::Design,
            pin: None
        }));
    }
}
