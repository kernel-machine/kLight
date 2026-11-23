//! Internal fixture definition model (not the QLC+ format: `klight-fixtures`
//! converts `.qxf` files into these types).

use serde::{Deserialize, Serialize};

use crate::ids::DefinitionKey;

/// 8-bit sRGB-ish color as found in definitions (`#rrggbb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#rrggbb` (or `rrggbb`). Returns `None` on malformed input.
    pub fn parse_hex(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        let r = u8::from_str_radix(s.get(0..2)?, 16).ok()?;
        let g = u8::from_str_radix(s.get(2..4)?, 16).ok()?;
        let b = u8::from_str_radix(s.get(4..6)?, 16).ok()?;
        Some(Self { r, g, b })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum FixtureKind {
    Dimmer,
    ColorChanger,
    MovingHead,
    Scanner,
    Flower,
    Effect,
    Strobe,
    Smoke,
    Hazer,
    Fan,
    Laser,
    LedBarBeams,
    LedBarPixels,
    #[default]
    Other,
}

/// What a channel controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Attribute {
    /// Master / generic intensity.
    Dimmer,
    Red,
    Green,
    Blue,
    White,
    Amber,
    Uv,
    Lime,
    Indigo,
    Cyan,
    Magenta,
    Yellow,
    /// Color temperature mixer (CTO/CTB).
    ColorTemperature,
    Pan,
    Tilt,
    ColorWheel,
    Gobo,
    GoboRotation,
    Prism,
    PrismRotation,
    /// Shutter and strobe.
    Shutter,
    Zoom,
    Focus,
    Iris,
    Frost,
    Speed,
    Effect,
    /// Lamp on/off, reset and similar: never stored in scenes, cues or palettes.
    Maintenance,
    NoFunction,
    Other,
}

impl Attribute {
    /// Additive color emitters (light-producing color channels).
    pub fn is_additive_color(self) -> bool {
        matches!(
            self,
            Attribute::Red
                | Attribute::Green
                | Attribute::Blue
                | Attribute::White
                | Attribute::Amber
                | Attribute::Uv
                | Attribute::Lime
                | Attribute::Indigo
        )
    }

    pub fn is_subtractive_color(self) -> bool {
        matches!(
            self,
            Attribute::Cyan | Attribute::Magenta | Attribute::Yellow
        )
    }

    /// Whether intermediate values are meaningful (continuous channels fade,
    /// stepped ones like wheels snap).
    pub fn is_fadeable(self) -> bool {
        !matches!(
            self,
            Attribute::ColorWheel
                | Attribute::Gobo
                | Attribute::Prism
                | Attribute::Shutter
                | Attribute::Effect
                | Attribute::Maintenance
        )
    }
}

/// Byte significance of a channel within a 16-bit pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ChannelByte {
    #[default]
    Coarse,
    Fine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ServiceAction {
    LampOn,
    LampOff,
    Reset,
    Other,
}

/// Meaning of a DMX range within a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type")]
pub enum CapabilityKind {
    #[default]
    Generic,
    /// A color wheel slot (two colors for split slots).
    Color {
        color: Rgb8,
        color2: Option<Rgb8>,
    },
    Gobo,
    ShutterOpen,
    ShutterClosed,
    /// Strobe with an optional frequency range in Hz (from `min` to `max` DMX value).
    Strobe {
        hz_min: Option<f32>,
        hz_max: Option<f32>,
    },
    PrismOn,
    Open,
    Service {
        action: ServiceAction,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Capability {
    pub min: u8,
    pub max: u8,
    pub name: String,
    pub kind: CapabilityKind,
}

impl Capability {
    pub fn midpoint(&self) -> u8 {
        let lo = u16::from(self.min.min(self.max));
        let hi = u16::from(self.min.max(self.max));
        // Result is <= 255 because both inputs are.
        u8::try_from((lo + hi) / 2).unwrap_or(u8::MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChannelDef {
    pub name: String,
    pub attribute: Attribute,
    pub byte: ChannelByte,
    pub default: u8,
    pub capabilities: Vec<Capability>,
}

impl ChannelDef {
    pub fn is_service(&self) -> bool {
        self.attribute == Attribute::Maintenance
    }
}

/// Physical properties (ranges used for unit conversions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Physical {
    /// Total pan range in degrees (e.g. 540).
    pub pan_max_deg: f32,
    /// Total tilt range in degrees (e.g. 270).
    pub tilt_max_deg: f32,
    /// Beam angle range of the zoom, in degrees.
    pub zoom_min_deg: f32,
    pub zoom_max_deg: f32,
}

/// A head: a group of channels of a multi-cell fixture (indices into `ModeDef::channels`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HeadDef {
    pub channels: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ModeDef {
    pub name: String,
    /// Channel order of the mode: indices into `FixtureDefinition::channels`.
    pub channels: Vec<u16>,
    pub heads: Vec<HeadDef>,
    pub physical: Option<Physical>,
}

impl ModeDef {
    /// Number of DMX channels used by the mode.
    pub fn footprint(&self) -> u16 {
        u16::try_from(self.channels.len()).unwrap_or(u16::MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixtureDefinition {
    pub manufacturer: String,
    pub model: String,
    pub kind: FixtureKind,
    pub channels: Vec<ChannelDef>,
    pub modes: Vec<ModeDef>,
    pub physical: Physical,
}

impl FixtureDefinition {
    pub fn key(&self) -> DefinitionKey {
        DefinitionKey::new(&self.manufacturer, &self.model)
    }

    pub fn mode(&self, name: &str) -> Option<&ModeDef> {
        self.modes.iter().find(|m| m.name == name)
    }

    /// Physical data of a mode, falling back to the global one.
    pub fn physical_for(&self, mode: &ModeDef) -> Physical {
        mode.physical.unwrap_or(self.physical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_colors() {
        assert_eq!(Rgb8::parse_hex("#ff8000"), Some(Rgb8::new(255, 128, 0)));
        assert_eq!(Rgb8::parse_hex("00ff00"), Some(Rgb8::new(0, 255, 0)));
        assert_eq!(Rgb8::parse_hex("#fff"), None);
        assert_eq!(Rgb8::parse_hex("#gg0000"), None);
        assert_eq!(Rgb8::parse_hex("#ff00è"), None);
    }

    #[test]
    fn capability_midpoint() {
        let c = Capability {
            min: 10,
            max: 20,
            name: String::new(),
            kind: CapabilityKind::Generic,
        };
        assert_eq!(c.midpoint(), 15);
    }
}
