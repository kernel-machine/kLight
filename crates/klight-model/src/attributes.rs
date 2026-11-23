//! Abstract attributes ("intentions"): what the user wants, independent of
//! the DMX layout of a fixture.

use serde::{Deserialize, Serialize};

use crate::fixture_def::{Attribute, Rgb8};

/// Desired color in linear RGB, each component in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
    pub const RED: Color = Color::new(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::new(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::new(0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0);

    /// Clamps each component to `0..=1`, mapping NaN to 0.
    pub fn clamped(self) -> Self {
        Self {
            r: clamp01(self.r),
            g: clamp01(self.g),
            b: clamp01(self.b),
        }
    }

    /// Linear RGB → 8-bit sRGB (for displays and color pickers).
    pub fn to_srgb8(self) -> Rgb8 {
        let e = |v: f32| (linear_to_srgb(v) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgb8::new(e(self.r), e(self.g), e(self.b))
    }

    /// Converts an 8-bit sRGB color (as in definitions) to linear RGB.
    pub fn from_srgb8(c: Rgb8) -> Self {
        Self {
            r: srgb_to_linear(f32::from(c.r) / 255.0),
            g: srgb_to_linear(f32::from(c.g) / 255.0),
            b: srgb_to_linear(f32::from(c.b) / 255.0),
        }
    }
}

/// Clamp to `0..=1`; NaN becomes 0.
pub fn clamp01(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = clamp01(c);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", content = "hz")]
pub enum StrobeValue {
    Open,
    Closed,
    Hz(f32),
}

/// A single intention applied to a fixture head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttributeValue {
    /// Intensity `0.0..=1.0`.
    Intensity {
        level: f32,
    },
    Color {
        color: Color,
    },
    /// Pan/tilt in degrees, 0 = center of the range.
    Position {
        pan: f32,
        tilt: f32,
    },
    /// Fan over the target group, in group order: head `i` of `n` gets
    /// `pan + offset(i) * pan_spread` (and the same for tilt). Resolved by
    /// the core, so adding a head to the group re-spreads the fan.
    PositionFan {
        pan: f32,
        tilt: f32,
        pan_spread: f32,
        #[serde(default)]
        tilt_spread: f32,
        #[serde(default)]
        mode: FanMode,
    },
    /// Gobo by capability name (`"Open"` for no gobo).
    Gobo {
        name: String,
    },
    /// Prism by capability name (`"Open"` for no prism).
    Prism {
        name: String,
    },
    /// Beam angle in degrees.
    Zoom {
        degrees: f32,
    },
    Strobe {
        strobe: StrobeValue,
    },
    /// Raw DMX value for an attribute, used when the definition has no
    /// physical information.
    Raw {
        attribute: Attribute,
        value: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FanMode {
    /// From -spread/2 (first head) to +spread/2 (last head).
    #[default]
    Linear,
    /// Symmetric: 0 in the middle, spread/2 at both ends.
    FromCenter,
}

/// Offset factor (-0.5..=0.5) of head `order` of `len` in a fan.
pub fn fan_factor(order: u16, len: u16, mode: FanMode) -> f32 {
    if len <= 1 {
        return 0.0;
    }
    let t = f32::from(order) / f32::from(len - 1) - 0.5;
    match mode {
        FanMode::Linear => t,
        FanMode::FromCenter => t.abs(),
    }
}

/// Category of an intention; used for palettes and per-attribute timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum AttributeKind {
    Intensity,
    Color,
    Position,
    Beam,
}

impl AttributeValue {
    pub fn kind(&self) -> AttributeKind {
        match self {
            AttributeValue::Intensity { .. } => AttributeKind::Intensity,
            AttributeValue::Color { .. } => AttributeKind::Color,
            AttributeValue::Position { .. } | AttributeValue::PositionFan { .. } => {
                AttributeKind::Position
            }
            AttributeValue::Gobo { .. }
            | AttributeValue::Prism { .. }
            | AttributeValue::Zoom { .. }
            | AttributeValue::Strobe { .. } => AttributeKind::Beam,
            AttributeValue::Raw { attribute, .. } => kind_of_attribute(*attribute),
        }
    }

    /// The concrete value for head `order` of `len` in the target (fans are
    /// spread here; every other value is the same for all heads).
    pub fn for_member(&self, order: u16, len: u16) -> std::borrow::Cow<'_, AttributeValue> {
        match *self {
            AttributeValue::PositionFan {
                pan,
                tilt,
                pan_spread,
                tilt_spread,
                mode,
            } => {
                let k = fan_factor(order, len, mode);
                std::borrow::Cow::Owned(AttributeValue::Position {
                    pan: pan + k * pan_spread,
                    tilt: tilt + k * tilt_spread,
                })
            }
            _ => std::borrow::Cow::Borrowed(self),
        }
    }

    /// Two values that set the same thing (a newer one replaces the older).
    pub fn same_slot(&self, other: &AttributeValue) -> bool {
        let position = |v: &AttributeValue| {
            matches!(
                v,
                AttributeValue::Position { .. } | AttributeValue::PositionFan { .. }
            )
        };
        match (self, other) {
            (a, b) if position(a) && position(b) => true,
            (
                AttributeValue::Raw { attribute: a, .. },
                AttributeValue::Raw { attribute: b, .. },
            ) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// Raw values on maintenance channels are never allowed in scenes, cues or palettes.
    pub fn targets_service_channel(&self) -> bool {
        matches!(
            self,
            AttributeValue::Raw {
                attribute: Attribute::Maintenance,
                ..
            }
        )
    }
}

pub fn kind_of_attribute(a: Attribute) -> AttributeKind {
    match a {
        Attribute::Dimmer => AttributeKind::Intensity,
        Attribute::Pan | Attribute::Tilt => AttributeKind::Position,
        Attribute::Red
        | Attribute::Green
        | Attribute::Blue
        | Attribute::White
        | Attribute::Amber
        | Attribute::Uv
        | Attribute::Lime
        | Attribute::Indigo
        | Attribute::Cyan
        | Attribute::Magenta
        | Attribute::Yellow
        | Attribute::ColorTemperature
        | Attribute::ColorWheel => AttributeKind::Color,
        _ => AttributeKind::Beam,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_handles_nan() {
        assert_eq!(clamp01(f32::NAN), 0.0);
        assert_eq!(clamp01(2.0), 1.0);
        assert_eq!(clamp01(-1.0), 0.0);
    }

    #[test]
    fn attribute_value_json_shape() {
        let v = AttributeValue::Position {
            pan: 10.0,
            tilt: -5.0,
        };
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, r#"{"type":"position","pan":10.0,"tilt":-5.0}"#);
        let back: AttributeValue = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
