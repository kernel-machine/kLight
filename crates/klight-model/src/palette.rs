//! Palettes: named intentions stored per fixture, referenced (not copied)
//! by scenes, cues and effects.

use serde::{Deserialize, Serialize};

use crate::attributes::{AttributeKind, AttributeValue};
use crate::ids::{FixtureId, PaletteId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PaletteKind {
    Color,
    Position,
    Beam,
    Intensity,
}

impl PaletteKind {
    pub fn accepts(self, kind: AttributeKind) -> bool {
        matches!(
            (self, kind),
            (PaletteKind::Color, AttributeKind::Color)
                | (PaletteKind::Position, AttributeKind::Position)
                | (PaletteKind::Beam, AttributeKind::Beam)
                | (PaletteKind::Intensity, AttributeKind::Intensity)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixturePaletteValues {
    pub fixture: FixtureId,
    pub values: Vec<AttributeValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Palette {
    pub id: PaletteId,
    pub name: String,
    pub kind: PaletteKind,
    /// Fallback values used for fixtures without a specific entry.
    #[serde(default)]
    pub generic: Vec<AttributeValue>,
    #[serde(default)]
    pub per_fixture: Vec<FixturePaletteValues>,
}

impl Palette {
    /// Values for a fixture: its own entry, otherwise the generic one.
    pub fn values_for(&self, fixture: FixtureId) -> &[AttributeValue] {
        self.per_fixture
            .iter()
            .find(|e| e.fixture == fixture)
            .map(|e| e.values.as_slice())
            .unwrap_or(self.generic.as_slice())
    }
}
