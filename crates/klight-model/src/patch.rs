//! Universes, patched fixtures, orientation and groups.

use serde::{Deserialize, Serialize};

use crate::ids::{DefinitionKey, FixtureId, GroupId, UniverseId};

/// Number of channels in a DMX universe.
pub const UNIVERSE_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum OutputKind {
    EnttecPro,
    OpenDmx,
    /// In-memory output (tests, simulation, dry runs).
    Fake,
}

/// Which physical interface drives a universe. Devices are matched by USB
/// serial number, not by port, so hot-plugging on another port works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OutputAssignment {
    pub kind: OutputKind,
    pub serial_number: Option<String>,
    /// Optional port path used only when no serial number is known.
    pub port_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Universe {
    pub id: UniverseId,
    pub name: String,
    pub output: Option<OutputAssignment>,
}

/// Mounting orientation set at patch time. After it, "up", "left" and degrees
/// mean the same for every head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Orientation {
    pub invert_pan: bool,
    pub invert_tilt: bool,
    pub swap_pan_tilt: bool,
    /// Hanging upside down: mirrors both pan and tilt.
    pub upside_down: bool,
}

impl Orientation {
    pub fn effective_invert_pan(&self) -> bool {
        self.invert_pan ^ self.upside_down
    }
    pub fn effective_invert_tilt(&self) -> bool {
        self.invert_tilt ^ self.upside_down
    }
}

/// Optional per-fixture color correction: a 3x3 matrix applied to the
/// desired linear RGB color (row-major).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ColorCalibration {
    pub matrix: [[f32; 3]; 3],
}

impl Default for ColorCalibration {
    fn default() -> Self {
        Self {
            matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PatchedFixture {
    pub id: FixtureId,
    pub name: String,
    pub definition: DefinitionKey,
    pub mode: String,
    pub universe: UniverseId,
    /// First DMX address, 1-based (1..=512).
    pub address: u16,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub calibration: Option<ColorCalibration>,
    /// Where the lights may point: whatever plays, the pan and tilt are held
    /// inside (degrees, as in the scenes: after the orientation).
    #[serde(default)]
    pub position_limits: Option<PositionLimits>,
}

/// Minimum and maximum pan and tilt of a fixture, in degrees from the center
/// of its range, as scenes and effects give them (the orientation of the
/// fixture is applied after).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PositionLimits {
    pub pan_min: f32,
    pub pan_max: f32,
    pub tilt_min: f32,
    pub tilt_max: f32,
}

impl PositionLimits {
    /// Both bounds of each axis are numbers and the minimum is not above the
    /// maximum.
    pub fn is_valid(&self) -> bool {
        [self.pan_min, self.pan_max, self.tilt_min, self.tilt_max]
            .iter()
            .all(|v| v.is_finite())
            && self.pan_min <= self.pan_max
            && self.tilt_min <= self.tilt_max
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GridPos {
    pub x: u16,
    pub y: u16,
}

/// A head of a fixture inside a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GroupMember {
    pub fixture: FixtureId,
    #[serde(default)]
    pub head: u16,
    #[serde(default)]
    pub grid: Option<GridPos>,
}

/// Position of every member of a group in its grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GroupLayout {
    pub width: u16,
    pub height: u16,
    /// Same order as `FixtureGroup::members`.
    pub positions: Vec<GridPos>,
}

/// Ordered group of heads with an optional 2D grid (matrices, fans, effect phase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixtureGroup {
    pub id: GroupId,
    pub name: String,
    /// Members in group order.
    pub members: Vec<GroupMember>,
    #[serde(default)]
    pub grid_width: u16,
    #[serde(default)]
    pub grid_height: u16,
}

impl FixtureGroup {
    /// Grid used by matrix effects: the configured one, or a single row in
    /// group order when no grid is set (or a member has no position).
    pub fn layout(&self) -> GroupLayout {
        let n = u16::try_from(self.members.len()).unwrap_or(u16::MAX);
        let configured = self.grid_width > 0;
        let positions = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| match (configured, m.grid) {
                (true, Some(p)) => p,
                _ => GridPos {
                    x: u16::try_from(i).unwrap_or(u16::MAX),
                    y: 0,
                },
            })
            .collect();
        if configured {
            GroupLayout {
                width: self.grid_width,
                height: self.grid_height.max(1),
                positions,
            }
        } else {
            GroupLayout {
                width: n.max(1),
                height: 1,
                positions,
            }
        }
    }
}
