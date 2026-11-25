//! kLight engine core: compilation of the show, layers, HTP/LTP merge,
//! fades, chasers, cue lists, effects and the musical clock.
//!
//! No I/O. Time is injected through [`clock::Clock`]; given the same show,
//! the same commands and the same clock, the output is bit-for-bit identical.

pub mod clock;
pub mod compile;
pub mod effects;
pub mod engine;
pub mod frame;
pub mod music;
pub mod playback;
pub mod preview;
pub mod runner;
pub mod scene_edit;
pub mod shared;
pub mod stats;

pub use clock::{Clock, MonotonicClock, VirtualClock};
pub use engine::{Engine, EngineConfig};
pub use frame::{DmxFrame, FrameSlot};
pub use runner::{LoopHooks, TickLoop};
pub use shared::{EngineShared, TickStatsSnapshot, UniverseOutput};
