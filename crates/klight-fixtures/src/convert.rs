//! Physical units / intentions → DMX values for a specific fixture head.

use klight_model::patch::ColorCalibration;
use klight_model::{
    attributes::clamp01, Attribute, AttributeValue, CapabilityKind, Color, FixtureDefinition,
    Orientation, StrobeValue,
};

use crate::profile::{to16, FixtureProfile, Slot};

/// One resolved channel value (16-bit scale: 8-bit channels use the high byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Slot {
        slot: u16,
        value: u16,
    },
    /// The head has no intensity channel: the engine scales its color
    /// channels by this virtual dimmer value.
    VirtualDimmer {
        value: u16,
    },
}

/// Why an intention had no effect on a fixture. Not an error: the rest of
/// the command still applies.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Ignored {
    #[error("fixture has no {0} channel")]
    NoChannel(&'static str),
    #[error("fixture has no gobo/prism named '{0}'")]
    UnknownName(String),
    #[error("definition has no physical range for {0}")]
    NoPhysicalRange(&'static str),
    #[error("service channels cannot be stored")]
    ServiceChannel,
    #[error("head {0} does not exist")]
    NoHead(u16),
}

/// Fraction `0..=1` → 16-bit value, rounded.
pub fn unit_to16(t: f32) -> u16 {
    let t = clamp01(t);
    let v = (f64::from(t) * 65535.0).round();
    if v >= 65535.0 {
        u16::MAX
    } else if v <= 0.0 {
        0
    } else {
        v as u16
    }
}

/// Degrees (0 = center of `range`) → 16-bit DMX value.
pub fn degrees_to_dmx16(deg: f32, range: f32) -> u16 {
    if !range.is_finite() || range <= 0.0 || !deg.is_finite() {
        return 32768;
    }
    unit_to16((deg + range / 2.0) / range)
}

/// 16-bit DMX value → degrees (0 = center of `range`).
pub fn dmx16_to_degrees(v: u16, range: f32) -> f32 {
    f32::from(v) / 65535.0 * range - range / 2.0
}

// ---- Perceptual color (OKLab) ------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

pub fn linear_to_oklab(c: Color) -> Oklab {
    let c = c.clamped();
    let l = 0.412_221_46 * c.r + 0.536_332_55 * c.g + 0.051_445_995 * c.b;
    let m = 0.211_903_5 * c.r + 0.680_699_5 * c.g + 0.107_396_96 * c.b;
    let s = 0.088_302_46 * c.r + 0.281_718_85 * c.g + 0.629_978_7 * c.b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    Oklab {
        l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    }
}

pub fn oklab_distance2(x: Oklab, y: Oklab) -> f32 {
    let dl = x.l - y.l;
    let da = x.a - y.a;
    let db = x.b - y.b;
    dl * dl + da * da + db * db
}

/// Index of the closest color in a perceptual space.
pub fn nearest_color<I: IntoIterator<Item = Color>>(target: Color, candidates: I) -> Option<usize> {
    let t = linear_to_oklab(target);
    let mut best: Option<(usize, f32)> = None;
    for (i, c) in candidates.into_iter().enumerate() {
        let d = oklab_distance2(t, linear_to_oklab(c));
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

fn apply_calibration(c: Color, cal: Option<&ColorCalibration>) -> Color {
    let c = c.clamped();
    let Some(cal) = cal else { return c };
    let m = &cal.matrix;
    Color {
        r: m[0][0] * c.r + m[0][1] * c.g + m[0][2] * c.b,
        g: m[1][0] * c.r + m[1][1] * c.g + m[1][2] * c.b,
        b: m[2][0] * c.r + m[2][1] * c.g + m[2][2] * c.b,
    }
    .clamped()
}

/// Additive mix for whatever emitters the head has.
///
/// Rules: white is extracted as `min(r, g, b)` (so it is zero for saturated
/// colors); amber, modeled as (1, 0.5, 0), is extracted from what remains of
/// red and green; UV, lime and indigo are not reachable from RGB and stay 0.
pub fn additive_mix(c: Color, has_white: bool, has_amber: bool) -> AdditiveMix {
    let (mut r, mut g, mut b) = (c.r, c.g, c.b);
    let mut w = 0.0;
    let mut a = 0.0;
    if has_white {
        w = r.min(g).min(b);
        r -= w;
        g -= w;
        b -= w;
    }
    if has_amber {
        a = r.min(g * 2.0).max(0.0);
        r -= a;
        g -= a * 0.5;
    }
    AdditiveMix {
        r: clamp01(r),
        g: clamp01(g),
        b: clamp01(b),
        w: clamp01(w),
        a: clamp01(a),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdditiveMix {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub w: f32,
    pub a: f32,
}

/// Context needed to convert intentions for one patched fixture.
pub struct Converter<'a> {
    pub def: &'a FixtureDefinition,
    pub profile: &'a FixtureProfile,
    pub orientation: Orientation,
    pub calibration: Option<&'a ColorCalibration>,
}

impl Converter<'_> {
    fn channel_of(&self, slot: &Slot) -> Option<&klight_model::ChannelDef> {
        self.def.channels.get(usize::from(slot.channel))
    }

    /// Converts one intention for `head`, pushing the resulting values to `out`.
    pub fn resolve(
        &self,
        head: u16,
        value: &AttributeValue,
        out: &mut Vec<Resolved>,
    ) -> Result<(), Ignored> {
        if self.profile.heads.get(usize::from(head)).is_none() {
            return Err(Ignored::NoHead(head));
        }
        match value {
            AttributeValue::Intensity { level } => {
                let v = unit_to16(*level);
                match self.profile.find(head, Attribute::Dimmer) {
                    Some((slot, _)) => out.push(Resolved::Slot { slot, value: v }),
                    None => out.push(Resolved::VirtualDimmer { value: v }),
                }
                Ok(())
            }
            AttributeValue::Color { color } => self.resolve_color(head, *color, out),
            // A fan reaching a single head directly: its center.
            AttributeValue::Position { pan, tilt }
            | AttributeValue::PositionFan { pan, tilt, .. } => {
                self.resolve_position(head, *pan, *tilt, out)
            }
            AttributeValue::Gobo { name } => self.resolve_named(head, Attribute::Gobo, name, out),
            AttributeValue::Prism { name } => self.resolve_named(head, Attribute::Prism, name, out),
            AttributeValue::Zoom { degrees } => {
                let (slot, _) = self
                    .profile
                    .find(head, Attribute::Zoom)
                    .ok_or(Ignored::NoChannel("zoom"))?;
                let p = self.profile.physical;
                let span = p.zoom_max_deg - p.zoom_min_deg;
                if !span.is_finite() || span <= 0.0 {
                    return Err(Ignored::NoPhysicalRange("zoom"));
                }
                out.push(Resolved::Slot {
                    slot,
                    value: unit_to16((degrees - p.zoom_min_deg) / span),
                });
                Ok(())
            }
            AttributeValue::Strobe { strobe } => self.resolve_strobe(head, *strobe, out),
            AttributeValue::Raw { attribute, value } => {
                if *attribute == Attribute::Maintenance {
                    return Err(Ignored::ServiceChannel);
                }
                let (slot, _) = self
                    .profile
                    .find(head, *attribute)
                    .ok_or(Ignored::NoChannel("matching"))?;
                out.push(Resolved::Slot {
                    slot,
                    value: to16(*value),
                });
                Ok(())
            }
        }
    }

    fn resolve_position(
        &self,
        head: u16,
        pan: f32,
        tilt: f32,
        out: &mut Vec<Resolved>,
    ) -> Result<(), Ignored> {
        let o = self.orientation;
        let pan = if o.effective_invert_pan() { -pan } else { pan };
        let tilt = if o.effective_invert_tilt() {
            -tilt
        } else {
            tilt
        };
        let (pan_attr, tilt_attr) = if o.swap_pan_tilt {
            (Attribute::Tilt, Attribute::Pan)
        } else {
            (Attribute::Pan, Attribute::Tilt)
        };
        let range_of = |a: Attribute| match a {
            Attribute::Pan => self.profile.physical.pan_max_deg,
            _ => self.profile.physical.tilt_max_deg,
        };
        let pan_slot = self.profile.find(head, pan_attr);
        let tilt_slot = self.profile.find(head, tilt_attr);
        if pan_slot.is_none() && tilt_slot.is_none() {
            return Err(Ignored::NoChannel("pan/tilt"));
        }
        let mut any = false;
        if let Some((slot, _)) = pan_slot {
            let r = range_of(pan_attr);
            if r > 0.0 {
                out.push(Resolved::Slot {
                    slot,
                    value: degrees_to_dmx16(pan, r),
                });
                any = true;
            }
        }
        if let Some((slot, _)) = tilt_slot {
            let r = range_of(tilt_attr);
            if r > 0.0 {
                out.push(Resolved::Slot {
                    slot,
                    value: degrees_to_dmx16(tilt, r),
                });
                any = true;
            }
        }
        if any {
            Ok(())
        } else {
            Err(Ignored::NoPhysicalRange("pan/tilt"))
        }
    }

    fn resolve_color(
        &self,
        head: u16,
        color: Color,
        out: &mut Vec<Resolved>,
    ) -> Result<(), Ignored> {
        let c = apply_calibration(color, self.calibration);
        let p = self.profile;
        let has = |a| p.has(head, a);

        if has(Attribute::Red) || has(Attribute::Green) || has(Attribute::Blue) {
            let mix = additive_mix(c, has(Attribute::White), has(Attribute::Amber));
            let pairs = [
                (Attribute::Red, mix.r),
                (Attribute::Green, mix.g),
                (Attribute::Blue, mix.b),
                (Attribute::White, mix.w),
                (Attribute::Amber, mix.a),
                (Attribute::Uv, 0.0),
                (Attribute::Lime, 0.0),
                (Attribute::Indigo, 0.0),
            ];
            for (attr, v) in pairs {
                if let Some((slot, _)) = p.find(head, attr) {
                    out.push(Resolved::Slot {
                        slot,
                        value: unit_to16(v),
                    });
                }
            }
            return Ok(());
        }
        if has(Attribute::Cyan) || has(Attribute::Magenta) || has(Attribute::Yellow) {
            for (attr, v) in [
                (Attribute::Cyan, 1.0 - c.r),
                (Attribute::Magenta, 1.0 - c.g),
                (Attribute::Yellow, 1.0 - c.b),
            ] {
                if let Some((slot, _)) = p.find(head, attr) {
                    out.push(Resolved::Slot {
                        slot,
                        value: unit_to16(v),
                    });
                }
            }
            return Ok(());
        }
        if let Some((slot, s)) = p.find(head, Attribute::ColorWheel) {
            let ch = self.channel_of(s).ok_or(Ignored::NoChannel("color"))?;
            let options: Vec<(Color, u8)> = ch
                .capabilities
                .iter()
                .filter_map(|cap| match cap.kind {
                    CapabilityKind::Color {
                        color,
                        color2: None,
                    } => Some((Color::from_srgb8(color), cap.midpoint())),
                    CapabilityKind::Open => Some((Color::WHITE, cap.midpoint())),
                    _ => None,
                })
                .collect();
            let best = nearest_color(c, options.iter().map(|(c, _)| *c))
                .and_then(|i| options.get(i))
                .ok_or(Ignored::NoChannel("color"))?;
            out.push(Resolved::Slot {
                slot,
                value: to16(best.1),
            });
            return Ok(());
        }
        Err(Ignored::NoChannel("color"))
    }

    fn resolve_named(
        &self,
        head: u16,
        attribute: Attribute,
        name: &str,
        out: &mut Vec<Resolved>,
    ) -> Result<(), Ignored> {
        let label = if attribute == Attribute::Gobo {
            "gobo"
        } else {
            "prism"
        };
        let (slot, s) = self
            .profile
            .find(head, attribute)
            .ok_or(Ignored::NoChannel(label))?;
        let ch = self.channel_of(s).ok_or(Ignored::NoChannel(label))?;
        let wanted = name.trim().to_lowercase();
        let wants_open = wanted == "open" || wanted == "off" || wanted == "none";
        let cap = ch
            .capabilities
            .iter()
            .find(|c| c.name.trim().to_lowercase() == wanted)
            .or_else(|| {
                wants_open
                    .then(|| {
                        ch.capabilities
                            .iter()
                            .find(|c| c.kind == CapabilityKind::Open)
                    })
                    .flatten()
            })
            .ok_or_else(|| Ignored::UnknownName(name.to_owned()))?;
        out.push(Resolved::Slot {
            slot,
            value: to16(cap.midpoint()),
        });
        Ok(())
    }

    fn resolve_strobe(
        &self,
        head: u16,
        strobe: StrobeValue,
        out: &mut Vec<Resolved>,
    ) -> Result<(), Ignored> {
        let (slot, s) = self
            .profile
            .find(head, Attribute::Shutter)
            .ok_or(Ignored::NoChannel("shutter"))?;
        let ch = self.channel_of(s).ok_or(Ignored::NoChannel("shutter"))?;
        let value = match strobe {
            StrobeValue::Open => ch
                .capabilities
                .iter()
                .find(|c| c.kind == CapabilityKind::ShutterOpen)
                .map(|c| c.midpoint()),
            StrobeValue::Closed => ch
                .capabilities
                .iter()
                .find(|c| c.kind == CapabilityKind::ShutterClosed)
                .map(|c| c.midpoint()),
            StrobeValue::Hz(hz) => strobe_dmx(ch, hz),
        }
        .ok_or(Ignored::NoPhysicalRange("strobe"))?;
        out.push(Resolved::Slot {
            slot,
            value: to16(value),
        });
        Ok(())
    }
}

/// DMX value for a strobe frequency, from capabilities with a Hz range.
pub fn strobe_dmx(ch: &klight_model::ChannelDef, hz: f32) -> Option<u8> {
    if !hz.is_finite() {
        return None;
    }
    let mut best: Option<(f32, u8)> = None;
    for cap in &ch.capabilities {
        if let CapabilityKind::Strobe {
            hz_min: Some(lo),
            hz_max: Some(hi),
        } = cap.kind
        {
            let (dmin, dmax) = (f32::from(cap.min), f32::from(cap.max));
            let (value, err) = if (hi - lo).abs() < f32::EPSILON {
                (cap.midpoint(), (hz - lo).abs())
            } else {
                let t = ((hz - lo) / (hi - lo)).clamp(0.0, 1.0);
                let v = (dmin + t * (dmax - dmin)).round().clamp(0.0, 255.0) as u8;
                let clamped = hz.clamp(lo.min(hi), lo.max(hi));
                (v, (hz - clamped).abs())
            };
            if best.is_none_or(|(e, _)| err < e) {
                best = Some((err, value));
            }
        }
    }
    best.map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrees_center_and_edges() {
        assert_eq!(degrees_to_dmx16(0.0, 540.0), 32768);
        assert_eq!(degrees_to_dmx16(-270.0, 540.0), 0);
        assert_eq!(degrees_to_dmx16(270.0, 540.0), 65535);
        assert_eq!(degrees_to_dmx16(1000.0, 540.0), 65535);
        assert_eq!(degrees_to_dmx16(f32::NAN, 540.0), 32768);
    }

    #[test]
    fn white_extraction_zero_for_saturated() {
        let m = additive_mix(Color::RED, true, false);
        assert_eq!((m.r, m.g, m.b, m.w), (1.0, 0.0, 0.0, 0.0));
        let m = additive_mix(Color::WHITE, true, false);
        assert_eq!((m.r, m.g, m.b, m.w), (0.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn nearest_color_picks_red() {
        let opts = [Color::WHITE, Color::new(0.8, 0.05, 0.0), Color::BLUE];
        assert_eq!(nearest_color(Color::RED, opts), Some(1));
    }
}
