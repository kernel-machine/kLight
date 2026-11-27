//! Functions: scenes, chasers, cue lists, movement and matrix effects.

use serde::{Deserialize, Serialize};

use crate::attributes::{AttributeKind, AttributeValue, Color};
use crate::fixture_def::Attribute;
use crate::ids::{FixtureId, FolderId, FunctionId, GroupId, PaletteId};

/// A duration, either absolute or locked to the musical clock.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum TimeValue {
    Ms(u32),
    Beats(f32),
}

impl Default for TimeValue {
    fn default() -> Self {
        TimeValue::Ms(0)
    }
}

impl TimeValue {
    pub const ZERO: TimeValue = TimeValue::Ms(0);

    /// Beat lengths offered for the times that follow the tempo (chaser steps
    /// and fades, effect cycles): from 1/32 to 32 beats, doubling.
    pub const BEAT_CHOICES: [f32; 11] = [
        0.03125, 0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0,
    ];

    /// Duration in microseconds at the given tempo. Invalid values (NaN,
    /// negative, non-positive BPM) yield 0.
    pub fn to_micros(self, bpm: f64) -> u64 {
        match self {
            TimeValue::Ms(ms) => u64::from(ms) * 1000,
            TimeValue::Beats(b) => {
                let b = f64::from(b);
                if !(b.is_finite() && b > 0.0 && bpm.is_finite() && bpm > 0.0) {
                    return 0;
                }
                let us = b * 60_000_000.0 / bpm;
                if us >= u64::MAX as f64 {
                    u64::MAX
                } else {
                    us.round() as u64
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Timing {
    #[serde(default)]
    pub delay: TimeValue,
    #[serde(default)]
    pub fade_in: TimeValue,
    #[serde(default)]
    pub fade_out: TimeValue,
}

/// Distribution of delays across the heads of a group, in group order
/// (e.g. heads lighting up one after the other).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Spread {
    #[default]
    None,
    /// Head `i` gets an extra delay of `i * step`.
    Linear { step: TimeValue },
    /// Delay grows from the center of the group outwards.
    FromCenter { step: TimeValue },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    Fixture {
        id: FixtureId,
    },
    Head {
        id: FixtureId,
        head: u16,
    },
    Group {
        id: GroupId,
    },
    /// Every light of the scene the value belongs to ([`Scene::members`]).
    /// Only the values of a scene can have it.
    Scene,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValueSource {
    Explicit {
        value: AttributeValue,
    },
    Palette {
        id: PaletteId,
    },
    /// One byte of one channel set by hand (the faders of the scene editor).
    /// The target must be a single fixture.
    Channel {
        channel: ChannelRef,
        value: u8,
    },
}

/// A channel of a fixture, named independently of its DMX address and of its
/// position in the mode, so that it survives a change of fixture model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelRef {
    /// The `index`-th channel (0-based, in mode order) with this attribute.
    /// `fine` selects the low byte of a 16-bit channel.
    Dmx {
        attribute: Attribute,
        index: u16,
        #[serde(default)]
        fine: bool,
    },
    /// The virtual dimmer of a head without an intensity channel.
    VirtualDimmer { head: u16 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneValue {
    pub target: Target,
    pub value: ValueSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AttributeTiming {
    pub kind: AttributeKind,
    pub timing: Timing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Scene {
    /// The groups and fixtures added to the scene, in order.
    #[serde(default)]
    pub members: Vec<Target>,
    /// Whether lights added later take the values of the whole scene.
    #[serde(default = "yes")]
    pub new_members_follow: bool,
    pub values: Vec<SceneValue>,
    #[serde(default)]
    pub timing: Timing,
    #[serde(default)]
    pub spread: Spread,
    /// Overrides of `timing` for specific attribute kinds.
    #[serde(default)]
    pub attribute_timing: Vec<AttributeTiming>,
}

fn yes() -> bool {
    true
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            members: Vec::new(),
            new_members_follow: yes(),
            values: Vec::new(),
            timing: Timing::default(),
            spread: Spread::default(),
            attribute_timing: Vec::new(),
        }
    }
}

impl Scene {
    pub fn timing_for(&self, kind: AttributeKind) -> Timing {
        self.attribute_timing
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| t.timing)
            .unwrap_or(self.timing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum RunMode {
    #[default]
    Loop,
    SingleShot,
    PingPong,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChaserStep {
    /// Function played by this step, of one of the types in
    /// [`FunctionCategory::CHASER_STEPS`]. (Named after scenes, the first of
    /// them, so that existing show files keep loading.)
    pub scene: FunctionId,
    /// Overrides of the chaser defaults.
    #[serde(default)]
    pub hold: Option<TimeValue>,
    #[serde(default)]
    pub fade: Option<TimeValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Chaser {
    pub steps: Vec<ChaserStep>,
    /// Duration of each step (from its start to the start of the next).
    pub hold: TimeValue,
    /// Crossfade between steps.
    #[serde(default)]
    pub fade: TimeValue,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub run_mode: RunMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CueFollow {
    /// Wait for Go.
    #[default]
    Manual,
    /// Go automatically after the given time from the start of the cue.
    Auto { after: TimeValue },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Cue {
    /// Cue number shown to the operator (e.g. 1, 1.5, 2).
    pub number: f32,
    pub name: String,
    /// Full state of the cue. (Tracking cue lists, see SPEC §17, will store
    /// only the differences; the field keeps that evolution possible.)
    pub values: Vec<SceneValue>,
    #[serde(default)]
    pub timing: Timing,
    #[serde(default)]
    pub follow: CueFollow,
    /// Per-cue override of the cue list setting.
    #[serde(default)]
    pub move_in_black: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CueList {
    pub cues: Vec<Cue>,
    #[serde(default)]
    pub move_in_black: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MovementShape {
    Circle,
    Eight,
    Line,
    /// A regular polygon, flat at the bottom, fitted to the width and height
    /// of the effect. Four sides are the square the shape used to be: shows
    /// saved with `square` open as this.
    #[serde(alias = "square")]
    Polygon {
        #[serde(default = "default_polygon_sides")]
        sides: u8,
    },
    Lissajous {
        x_freq: u8,
        y_freq: u8,
    },
    /// A sine wave across the width, with `periods` crests, played across and
    /// back along the same wave.
    Sine {
        periods: u8,
    },
    /// The heads of the group open from the center to opposite sides (the
    /// outer ones as far as the width) and close again, holding `hold`
    /// percent of the cycle open and closed.
    Fan {
        hold: u8,
    },
}

fn default_polygon_sides() -> u8 {
    4
}

fn default_rainbow_spread() -> f32 {
    1.0
}

fn default_chase_width() -> u8 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MovementEffect {
    pub group: GroupId,
    pub shape: MovementShape,
    /// Center position in degrees.
    pub center_pan: f32,
    pub center_tilt: f32,
    /// Size in degrees.
    pub width: f32,
    pub height: f32,
    /// Duration of one full cycle.
    pub period: TimeValue,
    /// Phase offset distributed over the group (degrees, 0..360 total).
    #[serde(default)]
    pub phase_spread: f32,
    #[serde(default)]
    pub direction: Direction,
    /// Rotation of the shape around its center, degrees counterclockwise.
    #[serde(default)]
    pub rotation: f32,
    /// Heads that draw the shape mirrored left to right around its center.
    #[serde(default)]
    pub mirror: MovementMirror,
    /// Loop, one cycle then stop, or back and forth along the shape.
    #[serde(default)]
    pub run_mode: RunMode,
    /// The shape is added to the pan and tilt the scenes below give each
    /// head (`center_*` is then an offset from there) instead of being placed
    /// at `center_*`.
    #[serde(default)]
    pub relative: bool,
    /// The size grows from nothing to full when the effect starts, and
    /// shrinks back to the center when it is stopped.
    #[serde(default)]
    pub fade_in: TimeValue,
    #[serde(default)]
    pub fade_out: TimeValue,
}

/// Which heads of a movement effect draw the shape mirrored left to right
/// (pan around the center), by their place in the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MovementMirror {
    #[default]
    None,
    /// Every second head (the 2nd, 4th, ...).
    Alternate,
    /// The second half of the group (the middle head of an odd group is
    /// not mirrored).
    Halves,
}

impl MovementMirror {
    /// Whether the head at `order` of `n` is mirrored.
    pub fn mirrors(self, order: usize, n: usize) -> bool {
        match self {
            MovementMirror::None => false,
            MovementMirror::Alternate => order % 2 == 1,
            MovementMirror::Halves => order >= n.div_ceil(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MatrixAlgorithm {
    Solid {
        color: Color,
    },
    /// A band of colored lines moving across the grid, `width` cells wide
    /// (1 to 16; one is a single line).
    Chase {
        color: Color,
        background: Color,
        #[serde(default = "default_chase_width")]
        width: u8,
    },
    /// Sine wave of intensity across the X axis.
    Wave {
        color: Color,
        wavelength: f32,
    },
    /// Horizontal gradient between two colors, scrolling with time.
    Gradient {
        from: Color,
        to: Color,
    },
    Text {
        text: String,
        color: Color,
    },
    /// The grid fills with the color, one column after the other, over the
    /// cycle.
    Fill {
        color: Color,
        background: Color,
    },
    /// Cells light up at random, a new draw eight times per cycle;
    /// `density` (0..=1) is the share of cells lit.
    /// Every hue, along the axis and scrolling with time; `spread` rainbows
    /// fit the grid (0.25 to 4).
    Rainbow {
        #[serde(default = "default_rainbow_spread")]
        spread: f32,
    },
    Twinkle {
        color: Color,
        background: Color,
        density: f32,
    },
}

/// What a matrix effect drives on the lights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MatrixControls {
    /// The color of every cell, at the effect's level of intensity.
    #[default]
    Color,
    /// Only the intensity of every cell: the brightness of the pattern (its
    /// brightest color component). The colors stay those the scenes give;
    /// intensity merges HTP, so scenes that also set it raise the floor.
    Dimmer,
}

/// Along which side of the grid a matrix effect moves (chase, wave,
/// gradient, fill). Text always scrolls left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MatrixAxis {
    /// Across the columns, left to right.
    #[default]
    Horizontal,
    /// Across the rows, top to bottom.
    Vertical,
    /// Out from the center of the grid, in rings.
    Radial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MatrixEffect {
    pub group: GroupId,
    pub algorithm: MatrixAlgorithm,
    /// Duration of one animation cycle.
    pub period: TimeValue,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub axis: MatrixAxis,
    /// What the effect drives on the lights: their color, or only their
    /// intensity.
    #[serde(default)]
    pub controls: MatrixControls,
    /// Loop, one cycle then stop, or forth and back.
    #[serde(default)]
    pub run_mode: RunMode,
    /// The level rises from nothing when the effect starts, and goes back
    /// to nothing when it is stopped.
    #[serde(default)]
    pub fade_in: TimeValue,
    #[serde(default)]
    pub fade_out: TimeValue,
}

/// Functions played all together, in parallel, of the types in
/// [`FunctionCategory::COLLECTION_MEMBERS`]. Where several scenes of a
/// collection drive the same channel, the highest value wins (HTP), whatever
/// the channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Collection {
    pub functions: Vec<FunctionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionKind {
    Scene(Scene),
    Chaser(Chaser),
    CueList(CueList),
    MovementEffect(MovementEffect),
    MatrixEffect(MatrixEffect),
    Collection(Collection),
}

/// Type of a function, used to list functions by type and to scope folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FunctionCategory {
    Scene,
    Chaser,
    CueList,
    MovementEffect,
    MatrixEffect,
    Collection,
}

impl FunctionCategory {
    /// Types of function a chaser step can play.
    pub const CHASER_STEPS: [FunctionCategory; 4] = [
        FunctionCategory::Scene,
        FunctionCategory::MovementEffect,
        FunctionCategory::MatrixEffect,
        FunctionCategory::Collection,
    ];

    /// Types of function a collection can play: the ones that contain no
    /// other function.
    pub const COLLECTION_MEMBERS: [FunctionCategory; 3] = [
        FunctionCategory::Scene,
        FunctionCategory::MovementEffect,
        FunctionCategory::MatrixEffect,
    ];
}

impl FunctionKind {
    pub fn category(&self) -> FunctionCategory {
        match self {
            FunctionKind::Scene(_) => FunctionCategory::Scene,
            FunctionKind::Chaser(_) => FunctionCategory::Chaser,
            FunctionKind::CueList(_) => FunctionCategory::CueList,
            FunctionKind::MovementEffect(_) => FunctionCategory::MovementEffect,
            FunctionKind::MatrixEffect(_) => FunctionCategory::MatrixEffect,
            FunctionKind::Collection(_) => FunctionCategory::Collection,
        }
    }
}

/// A user folder inside one function type (nestable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FunctionFolder {
    pub id: FolderId,
    pub category: FunctionCategory,
    pub name: String,
    /// Parent folder (same category); `None` = top level of the category.
    #[serde(default)]
    pub parent: Option<FolderId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub kind: FunctionKind,
    /// Folder of the function (`None` = top level of its type).
    #[serde(default)]
    pub folder: Option<FolderId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beats_to_micros() {
        assert_eq!(TimeValue::Beats(1.0).to_micros(120.0), 500_000);
        assert_eq!(TimeValue::Ms(250).to_micros(120.0), 250_000);
        assert_eq!(TimeValue::Beats(f32::NAN).to_micros(120.0), 0);
        assert_eq!(TimeValue::Beats(1.0).to_micros(0.0), 0);
    }

    #[test]
    fn time_value_json() {
        assert_eq!(
            serde_json::to_string(&TimeValue::Ms(5)).unwrap(),
            r#"{"ms":5}"#
        );
        assert_eq!(
            serde_json::to_string(&TimeValue::Beats(0.5)).unwrap(),
            r#"{"beats":0.5}"#
        );
    }
}
