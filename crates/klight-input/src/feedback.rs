//! Controller feedback: widget state and color rendered as exact bytes for
//! a controller profile, sent as differences with a per-device rate limit.

use std::collections::BTreeMap;

use klight_model::input_map::{InputMessage, InputTarget};
use klight_model::vc::WidgetKind;
use klight_model::{Rgb8, RuntimeSnapshot, Show};
use serde::Deserialize;

use crate::midi::{control_change, note_on};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WidgetState {
    Inactive,
    Active,
    /// Next / armed: rendered blinking.
    Armed,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PaletteEntry {
    pub value: u8,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileKind {
    None,
    OnOff {
        #[serde(default = "default_on")]
        on: u8,
        #[serde(default)]
        off: u8,
    },
    /// Color picked by velocity/value: nearest palette color (perceptual).
    Palette {
        colors: Vec<PaletteEntry>,
    },
    /// Exact RGB through a SysEx template. Placeholders: `{index}` (note or
    /// controller number), `{r}`, `{g}`, `{b}` (scaled to `max`).
    RgbSysex {
        template: String,
        #[serde(default = "default_max")]
        max: u8,
    },
}

fn default_on() -> u8 {
    127
}
fn default_max() -> u8 {
    127
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ControllerProfile {
    pub name: String,
    #[serde(flatten)]
    pub kind: ProfileKind,
    /// Brightness factor of the inactive state (0..1).
    #[serde(default = "default_dim")]
    pub inactive_brightness: f32,
    /// Max feedback messages per second for the device.
    #[serde(default = "default_rate")]
    pub max_messages_per_second: u32,
}

fn default_dim() -> f32 {
    0.2
}
fn default_rate() -> u32 {
    500
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("invalid profile: {0}")]
    Invalid(String),
}

impl ControllerProfile {
    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        let p: ControllerProfile =
            toml::from_str(text).map_err(|e| ProfileError::Invalid(e.to_string()))?;
        if let ProfileKind::Palette { colors } = &p.kind {
            if colors.is_empty() || colors.iter().any(|c| Rgb8::parse_hex(&c.color).is_none()) {
                return Err(ProfileError::Invalid(
                    "palette needs valid #rrggbb colors".into(),
                ));
            }
        }
        if let ProfileKind::RgbSysex { template, .. } = &p.kind {
            parse_template(template, 0, 0, 0, 0)
                .ok_or_else(|| ProfileError::Invalid("bad sysex template".into()))?;
        }
        Ok(p)
    }

    /// Bytes to show `color` in `state` on the control `message`.
    /// `blink_on` alternates for the armed state.
    pub fn render(
        &self,
        message: &InputMessage,
        state: WidgetState,
        color: Rgb8,
        blink_on: bool,
    ) -> Option<Vec<u8>> {
        let (channel, index, is_note) = match message {
            InputMessage::MidiNote { channel, note } => (*channel, *note, true),
            InputMessage::MidiCc {
                channel,
                controller,
            } => (*channel, *controller, false),
            InputMessage::Osc { .. } => return None,
        };
        let lit = match state {
            WidgetState::Inactive => None,
            WidgetState::Active => Some(1.0),
            WidgetState::Armed => blink_on.then_some(1.0),
        };
        let factor = lit.unwrap_or(self.inactive_brightness.clamp(0.0, 1.0));
        let scaled = scale(color, factor);
        let msg = |v: u8| -> Vec<u8> {
            if is_note {
                note_on(channel, index, v).to_vec()
            } else {
                control_change(channel, index, v).to_vec()
            }
        };
        match &self.kind {
            ProfileKind::None => None,
            ProfileKind::OnOff { on, off } => Some(msg(if lit.is_some() { *on } else { *off })),
            ProfileKind::Palette { colors } => {
                let target = scaled;
                let best = colors
                    .iter()
                    .filter_map(|c| Some((c.value, Rgb8::parse_hex(&c.color)?)))
                    .min_by(|a, b| {
                        perceptual_distance(a.1, target)
                            .total_cmp(&perceptual_distance(b.1, target))
                    })?;
                Some(msg(best.0))
            }
            ProfileKind::RgbSysex { template, max } => {
                let s = |c: u8| u8::try_from(u16::from(c) * u16::from(*max) / 255).unwrap_or(*max);
                parse_template(template, index, s(scaled.r), s(scaled.g), s(scaled.b))
            }
        }
    }
}

fn scale(c: Rgb8, f: f32) -> Rgb8 {
    let s = |v: u8| (f32::from(v) * f).round().clamp(0.0, 255.0) as u8;
    Rgb8::new(s(c.r), s(c.g), s(c.b))
}

/// Distance in OKLab between two 8-bit sRGB colors (same perceptual space
/// as color wheel matching in `klight-fixtures`).
pub fn perceptual_distance(a: Rgb8, b: Rgb8) -> f32 {
    use klight_fixtures::convert::{linear_to_oklab, oklab_distance2};
    use klight_model::Color;
    oklab_distance2(
        linear_to_oklab(Color::from_srgb8(a)),
        linear_to_oklab(Color::from_srgb8(b)),
    )
}

fn parse_template(t: &str, index: u8, r: u8, g: u8, b: u8) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for tok in t.split_whitespace() {
        let v = match tok {
            "{index}" => index,
            "{r}" => r,
            "{g}" => g,
            "{b}" => b,
            hex => u8::from_str_radix(hex, 16).ok()?,
        };
        out.push(v);
    }
    (out.first() == Some(&0xF0) && out.last() == Some(&0xF7)).then_some(out)
}

/// State of a widget for feedback, derived from the runtime snapshot.
pub fn widget_state(show: &Show, rt: &RuntimeSnapshot, id: klight_model::WidgetId) -> WidgetState {
    let Some(w) = show.virtual_console.find(id) else {
        return WidgetState::Inactive;
    };
    let active = |f| rt.state.active.iter().find(|a| a.id == f);
    match (&w.kind, w.function) {
        (
            WidgetKind::Button {
                action: klight_model::vc::ButtonAction::Blackout,
            },
            _,
        ) => {
            if rt.state.blackout {
                WidgetState::Active
            } else {
                WidgetState::Inactive
            }
        }
        (_, Some(f)) => match active(f) {
            Some(a) if a.paused => WidgetState::Armed,
            Some(_) => WidgetState::Active,
            None => WidgetState::Inactive,
        },
        _ => WidgetState::Inactive,
    }
}

/// Computes feedback for all mappings of a device and returns only what
/// changed since the last call (differences only).
#[derive(Debug, Default)]
pub struct FeedbackTracker {
    last: BTreeMap<InputMessage, Vec<u8>>,
}

impl FeedbackTracker {
    /// Forget what was sent: the next update re-sends everything (device
    /// reconnected, engine restarted).
    pub fn resync(&mut self) {
        self.last.clear();
    }

    pub fn update(
        &mut self,
        show: &Show,
        rt: &RuntimeSnapshot,
        device: &str,
        profile: &ControllerProfile,
        blink_on: bool,
        limit: usize,
    ) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for m in show.input_mappings.iter().filter(|m| m.device == device) {
            let InputTarget::Widget { id } = m.target else {
                continue;
            };
            let color = show
                .virtual_console
                .find(id)
                .and_then(|w| w.props.color)
                .unwrap_or(Rgb8::new(255, 255, 255));
            let state = widget_state(show, rt, id);
            let Some(bytes) = profile.render(&m.message, state, color, blink_on) else {
                continue;
            };
            if self.last.get(&m.message) != Some(&bytes) {
                if out.len() >= limit {
                    break; // rest goes out on the next update
                }
                self.last.insert(m.message.clone(), bytes.clone());
                out.push(bytes);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN: Rgb8 = Rgb8::new(0, 255, 0);
    const NOTE: InputMessage = InputMessage::MidiNote {
        channel: 0,
        note: 36,
    };

    #[test]
    fn on_off_profile_bytes() {
        let p = ControllerProfile::from_toml("name = \"x\"\nkind = \"on_off\"\n").unwrap();
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![0x90, 36, 127])
        );
        assert_eq!(
            p.render(&NOTE, WidgetState::Inactive, GREEN, true),
            Some(vec![0x90, 36, 0])
        );
        assert_eq!(
            p.render(&NOTE, WidgetState::Armed, GREEN, false),
            Some(vec![0x90, 36, 0])
        );
    }

    #[test]
    fn palette_profile_picks_nearest() {
        let p = ControllerProfile::from_toml(
            r##"name = "pal"
kind = "palette"
inactive_brightness = 0.0
colors = [ { value = 0, color = "#000000" }, { value = 5, color = "#ff0000" }, { value = 21, color = "#00ff00" }, { value = 45, color = "#0000ff" } ]
"##,
        )
        .unwrap();
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![0x90, 36, 21])
        );
        assert_eq!(
            p.render(&NOTE, WidgetState::Inactive, GREEN, true),
            Some(vec![0x90, 36, 0])
        );
    }

    #[test]
    fn sysex_profile_exact_rgb() {
        let p = ControllerProfile::from_toml(
            "name = \"rgb\"\nkind = \"rgb_sysex\"\ntemplate = \"F0 00 20 29 02 0D 03 03 {index} {r} {g} {b} F7\"\n",
        )
        .unwrap();
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![
                0xF0, 0x00, 0x20, 0x29, 0x02, 0x0D, 0x03, 0x03, 36, 0, 127, 0, 0xF7
            ])
        );
    }
}
