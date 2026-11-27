//! Pure data types for kLight. No I/O, no global state.
//!
//! Everything the engine runs is described by a [`show::Show`]; everything
//! that changes it is a [`command::Command`].

pub mod attributes;
pub mod command;
pub mod fixture_def;
pub mod function;
pub mod ids;
pub mod input_map;
pub mod inspect;
pub mod palette;
pub mod patch;
pub mod runtime;
pub mod show;
pub mod validate;
pub mod vc;

pub use attributes::{AttributeKind, AttributeValue, Color, FanMode, StrobeValue};
pub use command::{
    Command, CommandEnvelope, CommandError, CommandOutcome, CommandResult, CommandSource,
    ControlKey, OperatingMode,
};
pub use fixture_def::{
    Attribute, Capability, CapabilityKind, ChannelByte, ChannelDef, FixtureDefinition, FixtureKind,
    HeadDef, ModeDef, Physical, Rgb8,
};
pub use function::{
    ChannelRef, Chaser, ChaserStep, Collection, Cue, CueFollow, CueList, Function,
    FunctionCategory, FunctionFolder, FunctionKind, MatrixAlgorithm, MatrixAxis, MatrixEffect,
    MovementEffect, MovementMirror, Scene, SceneValue, Spread, Target, TimeValue, Timing,
    ValueSource,
};
pub use ids::*;
pub use palette::{Palette, PaletteKind};
pub use patch::{
    FixtureGroup, GroupLayout, GroupMember, Orientation, OutputAssignment, OutputKind,
    PatchedFixture, Universe, UNIVERSE_SIZE,
};
pub use runtime::{
    beat_flash, ActiveFunction, BeatFlash, ChannelOverride, ProgrammerEntry, RuntimeSnapshot,
    RuntimeState, TempoSource, TempoStatus,
};
pub use show::{Show, ShowSettings, SCHEMA_VERSION};
