//! Controller feedback: widget state and color rendered as exact bytes for
//! a controller profile, sent as differences with a per-device rate limit.

use std::collections::{BTreeMap, BTreeSet};

use klight_fixtures::qxi::InputProfileDef;
use klight_model::input_map::{InputMessage, InputTarget, LedBehavior};
use klight_model::vc::WidgetKind;
use klight_model::{Rgb8, RuntimeSnapshot, Show};
use serde::Deserialize;

use crate::midi::{control_change, note_on};

impl From<bool> for WidgetState {
    /// `true` = on.
    fn from(on: bool) -> Self {
        if on {
            WidgetState::Active
        } else {
            WidgetState::Inactive
        }
    }
}

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
    /// Parsed once when the profile is loaded (`#rrggbb` in files).
    #[serde(deserialize_with = "hex_color")]
    pub color: Rgb8,
}

fn hex_color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Rgb8, D::Error> {
    let s = String::deserialize(d)?;
    Rgb8::parse_hex(&s)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid color {s:?}, expected #rrggbb")))
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

/// An LED behaviour the controller selects through the MIDI channel of the
/// feedback message (e.g. "On 100% brightness", "Pulsing 1/4").
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Behavior {
    pub channel: u8,
    pub label: String,
}

/// A control of the device that has an LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct LedControl {
    /// `true` for a note, `false` for a controller number.
    pub note: bool,
    pub index: u8,
}

impl LedControl {
    /// The control behind a QLC+ channel number (controllers `0..=127`,
    /// notes from 128).
    fn from_qlcplus_number(n: u16) -> Option<Self> {
        match n {
            0..=127 => Some(Self {
                note: false,
                index: u8::try_from(n).ok()?,
            }),
            128..=255 => Some(Self {
                note: true,
                index: u8::try_from(n - 128).ok()?,
            }),
            _ => None,
        }
    }

    fn message(self) -> InputMessage {
        if self.note {
            InputMessage::MidiNote {
                channel: 0,
                note: self.index,
            }
        } else {
            InputMessage::MidiCc {
                channel: 0,
                controller: self.index,
            }
        }
    }
}

/// A control whose LED has no color, only on/off (the round buttons around a
/// pad grid): it takes plain values instead of the profile's color model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SimpleLed {
    /// `true` for a note, `false` for a controller number.
    pub note: bool,
    pub index: u8,
    /// Value that lights the LED (0 turns it off).
    pub on: u8,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ControllerProfile {
    pub name: String,
    #[serde(flatten)]
    pub kind: ProfileKind,
    /// LED behaviours the user can pick per widget state (profiles imported
    /// from QLC+ input profiles). Empty: the profile has a single look.
    #[serde(default)]
    pub behaviors: Vec<Behavior>,
    /// Feedback channel used for the "on" state until the user picks another.
    #[serde(default)]
    pub default_active: Option<u8>,
    /// Controls with a plain on/off LED in a profile that otherwise has colors.
    #[serde(default)]
    pub simple_leds: Vec<SimpleLed>,
    /// Every control of the device with an LED (from the QLC+ profile), so
    /// stale LED states can be cleared when feedback starts.
    #[serde(default)]
    pub led_controls: Vec<LedControl>,
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
            if colors.is_empty() {
                return Err(ProfileError::Invalid("palette needs colors".into()));
            }
        }
        if let ProfileKind::RgbSysex { template, .. } = &p.kind {
            parse_template(template, 0, 0, 0, 0)
                .ok_or_else(|| ProfileError::Invalid("bad sysex template".into()))?;
        }
        Ok(p)
    }

    /// Builds a profile from a QLC+ input profile. Profiles that can show
    /// colors become a palette; the ones with only a feedback range, on/off.
    /// Returns `None` for controllers without any feedback.
    pub fn from_qxi(def: &InputProfileDef) -> Option<Self> {
        if !def.has_feedback() {
            return None;
        }
        let kind = if def.colors.is_empty() {
            let upper = def
                .channels
                .iter()
                .find_map(|c| c.feedback.map(|f| f.upper))
                .unwrap_or(127);
            ProfileKind::OnOff { on: upper, off: 0 }
        } else {
            ProfileKind::Palette {
                colors: def
                    .colors
                    .iter()
                    // Flashing/blinking entries are behaviours, not colors.
                    .filter(|c| {
                        let l = c.label.to_lowercase();
                        !l.contains("flash") && !l.contains("blink")
                    })
                    // QLC+ stores LED values on its 0-255 scale; MIDI is 0-127.
                    .map(|c| PaletteEntry {
                        value: c.value / 2,
                        color: c.rgb,
                    })
                    .collect(),
            }
        };
        let behaviors: Vec<Behavior> = def
            .behaviors
            .iter()
            .map(|b| Behavior {
                channel: b.midi_channel,
                label: b.label.clone(),
            })
            .collect();
        // The channel the controller's own pads ask for is its "on" look.
        let default_active = def
            .feedback_midi_channel()
            .filter(|c| behaviors.iter().any(|b| b.channel == *c));
        // Controls whose feedback has no MIDI channel of its own in a profile
        // where pads have one: round buttons with a single-color LED.
        let rich = !def.colors.is_empty() || !behaviors.is_empty();
        let simple_leds = if rich {
            def.channels
                .iter()
                .filter_map(|c| {
                    let f = c.feedback.filter(|f| f.midi_channel.is_none())?;
                    let control = LedControl::from_qlcplus_number(c.number)?;
                    Some(SimpleLed {
                        note: control.note,
                        index: control.index,
                        on: if f.lower > 0 { f.lower } else { f.upper.max(1) },
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let led_controls = def
            .channels
            .iter()
            .filter(|c| c.feedback.is_some())
            .filter_map(|c| LedControl::from_qlcplus_number(c.number))
            .collect();
        Some(Self {
            name: def.name(),
            kind,
            behaviors,
            default_active,
            simple_leds,
            led_controls,
            inactive_brightness: default_dim(),
            max_messages_per_second: default_rate(),
        })
    }

    /// Bytes to show `color` in `state` on the control `message`, with the
    /// profile's default look. `blink_on` alternates for the armed state.
    pub fn render(
        &self,
        message: &InputMessage,
        state: WidgetState,
        color: Rgb8,
        blink_on: bool,
    ) -> Option<Vec<u8>> {
        self.render_with(message, state, color, blink_on, &Look::default())
    }

    /// Bytes that turn the LED of `message` fully off, whatever it showed
    /// before (a control that is no longer mapped).
    pub fn render_dark(&self, message: &InputMessage) -> Option<Vec<u8>> {
        let look = Look {
            active: None,
            inactive: Some(LedBehavior::Off),
        };
        self.render_with(
            message,
            WidgetState::Inactive,
            Rgb8::new(0, 0, 0),
            false,
            &look,
        )
    }

    /// Like [`render`](Self::render), with the LED behaviours the user chose.
    pub fn render_with(
        &self,
        message: &InputMessage,
        state: WidgetState,
        color: Rgb8,
        blink_on: bool,
        look: &Look,
    ) -> Option<Vec<u8>> {
        let (channel, index, is_note) = match message {
            InputMessage::MidiNote { channel, note } => (*channel, *note, true),
            InputMessage::MidiCc {
                channel,
                controller,
            } => (*channel, *controller, false),
            InputMessage::Osc { .. } => return None,
        };
        if let Some(bytes) = self.render_simple(message, state, blink_on) {
            return Some(bytes);
        }
        if !self.behaviors.is_empty() {
            return self.render_behavior(message, state, color, blink_on, look);
        }
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
            ProfileKind::Palette { colors } => Some(msg(nearest_value(colors, scaled)?)),
            ProfileKind::RgbSysex { template, max } => {
                let s = |c: u8| u8::try_from(u16::from(c) * u16::from(*max) / 255).unwrap_or(*max);
                parse_template(template, index, s(scaled.r), s(scaled.g), s(scaled.b))
            }
        }
    }
}

/// LED behaviours chosen by the user for the two widget states; `None`
/// means the profile default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub active: Option<LedBehavior>,
    pub inactive: Option<LedBehavior>,
}

impl ControllerProfile {
    /// Single-color LEDs: on while the widget is on, off otherwise (an armed
    /// widget blinks). `None` if the control is not one of them.
    fn render_simple(
        &self,
        message: &InputMessage,
        state: WidgetState,
        blink_on: bool,
    ) -> Option<Vec<u8>> {
        let (channel, index, note) = match message {
            InputMessage::MidiNote { channel, note } => (*channel, *note, true),
            InputMessage::MidiCc {
                channel,
                controller,
            } => (*channel, *controller, false),
            InputMessage::Osc { .. } => return None,
        };
        let led = self
            .simple_leds
            .iter()
            .find(|l| l.note == note && l.index == index)?;
        let lit = match state {
            WidgetState::Active => true,
            WidgetState::Inactive => false,
            WidgetState::Armed => blink_on,
        };
        let value = if lit { led.on } else { 0 };
        Some(if note {
            note_on(channel, index, value).to_vec()
        } else {
            control_change(channel, index, value).to_vec()
        })
    }

    /// Profiles with behaviours: the color is the palette value nearest to
    /// the widget color, the behaviour is the MIDI channel of the message.
    fn render_behavior(
        &self,
        message: &InputMessage,
        state: WidgetState,
        color: Rgb8,
        blink_on: bool,
        look: &Look,
    ) -> Option<Vec<u8>> {
        let (channel, index, is_note) = match message {
            InputMessage::MidiNote { channel, note } => (*channel, *note, true),
            InputMessage::MidiCc {
                channel,
                controller,
            } => (*channel, *controller, false),
            InputMessage::Osc { .. } => return None,
        };
        let default_on = self
            .default_active
            .or_else(|| self.behaviors.first().map(|b| b.channel))
            .map(|channel| LedBehavior::Channel { channel })
            .unwrap_or(LedBehavior::Off);
        let on = look.active.unwrap_or(default_on);
        let off = look.inactive.unwrap_or(LedBehavior::Off);
        let behavior = match state {
            WidgetState::Active => on,
            WidgetState::Inactive => off,
            // No look of its own yet: blink between the other two.
            WidgetState::Armed => {
                if blink_on {
                    on
                } else {
                    off
                }
            }
        };
        let msg = |channel: u8, v: u8| -> Vec<u8> {
            if is_note {
                note_on(channel, index, v).to_vec()
            } else {
                control_change(channel, index, v).to_vec()
            }
        };
        match behavior {
            LedBehavior::Off => Some(msg(channel, 0)),
            LedBehavior::Channel { channel: c } => {
                let value = match &self.kind {
                    ProfileKind::Palette { colors } => nearest_value(colors, color)?,
                    ProfileKind::OnOff { on, .. } => *on,
                    _ => return None,
                };
                Some(msg(c & 0x0F, value))
            }
        }
    }
}

/// Palette value whose color is perceptually closest to `target`.
fn nearest_value(colors: &[PaletteEntry], target: Rgb8) -> Option<u8> {
    colors
        .iter()
        .min_by(|a, b| {
            perceptual_distance(a.color, target).total_cmp(&perceptual_distance(b.color, target))
        })
        .map(|c| c.value)
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
pub fn widget_state(
    show: &Show,
    rt: &RuntimeSnapshot,
    id: klight_model::WidgetId,
    beat: f64,
) -> WidgetState {
    let Some(w) = show.virtual_console.find(id) else {
        return WidgetState::Inactive;
    };
    // Tempo widgets blink on the beat, like their on-screen lamp.
    if matches!(
        &w.kind,
        WidgetKind::SpeedDial
            | WidgetKind::Button {
                action: klight_model::vc::ButtonAction::TapTempo
            }
    ) {
        let t = &rt.state.tempo;
        return if klight_model::beat_flash(beat, t.bpm, t.beats_per_bar).on {
            WidgetState::Active
        } else {
            WidgetState::Inactive
        };
    }
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

/// Time-dependent inputs of the feedback: the blink phase of armed widgets
/// and the musical beat position (tempo LEDs).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FeedbackTime {
    pub blink_on: bool,
    pub beat: f64,
}

/// Computes feedback for all mappings of a device and returns only what
/// changed since the last call (differences only).
#[derive(Debug, Default)]
pub struct FeedbackTracker {
    last: BTreeMap<InputMessage, Vec<u8>>,
    /// LEDs of unmapped controls still to be darkened after (re)start; `None`
    /// until the first update.
    clear_queue: Option<Vec<InputMessage>>,
}

impl FeedbackTracker {
    /// Forget what was sent: the next update re-sends everything (device
    /// reconnected, engine restarted). The controls that had an LED state are
    /// still remembered, so the ones no longer mapped are switched off.
    pub fn resync(&mut self) {
        // Bytes are never empty, so every mapped control looks changed.
        self.last.values_mut().for_each(Vec::clear);
        self.clear_queue = None;
    }

    pub fn update(
        &mut self,
        show: &Show,
        rt: &RuntimeSnapshot,
        device: &str,
        profile: &ControllerProfile,
        time: FeedbackTime,
        limit: usize,
    ) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let look = show
            .input_devices
            .iter()
            .find(|d| d.name == device)
            .map(|d| Look {
                active: d.feedback_active,
                inactive: d.feedback_inactive,
            })
            .unwrap_or_default();
        // A control mapped more than once shows its first mapping only;
        // rendering both would flip the LED (and send twice) on every update.
        let mut rendered: BTreeSet<&InputMessage> = BTreeSet::new();
        for m in show.input_mappings.iter().filter(|m| m.device == device) {
            if matches!(m.target, InputTarget::GrandMaster) || !rendered.insert(&m.message) {
                continue;
            }
            let (state, color) = match m.target {
                InputTarget::Widget { id } => (
                    widget_state(show, rt, id, time.beat),
                    show.virtual_console
                        .find(id)
                        .and_then(|w| w.props.color)
                        .unwrap_or(Rgb8::new(255, 255, 255)),
                ),
                // Global controls mapped straight to a button: same LEDs as
                // their widgets (tap blinks on the beat, blackout when on).
                InputTarget::TapTempo => {
                    let t = &rt.state.tempo;
                    let lit = klight_model::beat_flash(time.beat, t.bpm, t.beats_per_bar).on;
                    (WidgetState::from(lit), Rgb8::new(255, 255, 255))
                }
                InputTarget::Blackout => (
                    WidgetState::from(rt.state.blackout),
                    Rgb8::new(255, 255, 255),
                ),
                InputTarget::GrandMaster => continue,
            };
            let Some(bytes) = profile.render_with(&m.message, state, color, time.blink_on, &look)
            else {
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
        // First update after a (re)start: the hardware may still show what an
        // earlier session left (a blinking pad of a mapping that is gone).
        // Darken every LED of the device that is not mapped now.
        let mapped_now: Vec<&InputMessage> = show
            .input_mappings
            .iter()
            .filter(|m| m.device == device)
            .map(|m| &m.message)
            .collect();
        let queue = self.clear_queue.get_or_insert_with(|| {
            profile
                .led_controls
                .iter()
                .map(|c| c.message())
                .filter(|m| !mapped_now.contains(&m))
                .collect()
        });
        while out.len() < limit {
            let Some(message) = queue.pop() else { break };
            if mapped_now.contains(&&message) {
                continue;
            }
            if let Some(bytes) = profile.render_dark(&message) {
                out.push(bytes);
            }
        }
        // Controls that lost their mapping keep showing their last state on
        // the hardware: switch those LEDs off once.
        let mapped: Vec<&InputMessage> = show
            .input_mappings
            .iter()
            .filter(|m| m.device == device && !matches!(m.target, InputTarget::GrandMaster))
            .map(|m| &m.message)
            .collect();
        let stale: Vec<InputMessage> = self
            .last
            .keys()
            .filter(|k| !mapped.contains(k))
            .cloned()
            .collect();
        for message in stale {
            if out.len() >= limit {
                break;
            }
            self.last.remove(&message);
            if let Some(bytes) = profile.render_dark(&message) {
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

    /// Akai APC mini mk2 pads: color = velocity, LED behaviour = MIDI channel.
    fn mk2() -> ControllerProfile {
        let def = klight_fixtures::parse_qxi(
            r##"<InputProfile>
 <Manufacturer>Akai</Manufacturer><Model>APC Mini mk2</Model>
 <Channel Number="128"><Name>Pad</Name><Type>Button</Type><Feedback LowerValue="1" UpperValue="26" MidiChannel="6"/></Channel>
 <ColorTable>
  <Color Value="0" Label="Black" RGB="#000000"/>
  <Color Value="10" Label="Red" RGB="#ff0000"/>
  <Color Value="42" Label="Green" RGB="#00ff00"/>
  <Color Value="44" Label="Green Blinking" RGB="#00ff00"/>
  <Color Value="90" Label="Blue" RGB="#0000ff"/>
 </ColorTable>
 <MidiChannelTable>
  <Channel Value="0" Label="On 10% brightness"/>
  <Channel Value="6" Label="On 100% brightness"/>
  <Channel Value="9" Label="Pulsing 1/4"/>
 </MidiChannelTable>
</InputProfile>"##,
        )
        .unwrap();
        ControllerProfile::from_qxi(&def).unwrap()
    }

    #[test]
    fn qxi_profile_defaults_full_brightness_when_on_and_off_otherwise() {
        let p = mk2();
        assert_eq!(p.default_active, Some(6));
        // Green pad, widget on: note-on on MIDI channel 6 (0x96), velocity 21
        // (the table says 42: QLC+ doubles LED values).
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![0x96, 36, 21])
        );
        // Widget off: note-on velocity 0 on the pad's own channel.
        assert_eq!(
            p.render(&NOTE, WidgetState::Inactive, GREEN, true),
            Some(vec![0x90, 36, 0])
        );
    }

    #[test]
    fn qxi_profile_follows_the_behaviours_the_user_picked() {
        let p = mk2();
        let look = Look {
            active: Some(LedBehavior::Channel { channel: 9 }),
            inactive: Some(LedBehavior::Channel { channel: 0 }),
        };
        let red = Rgb8::new(255, 0, 0);
        assert_eq!(
            p.render_with(&NOTE, WidgetState::Active, red, true, &look),
            Some(vec![0x99, 36, 5])
        );
        // Off state shows the same color dimly (10% brightness channel).
        assert_eq!(
            p.render_with(&NOTE, WidgetState::Inactive, red, true, &look),
            Some(vec![0x90, 36, 5])
        );
    }

    #[test]
    fn tempo_widgets_blink_on_the_beat() {
        use klight_model::vc::{ButtonAction, Widget, WidgetProps};
        let widget = |id, kind| Widget {
            id: klight_model::WidgetId(id),
            kind,
            props: WidgetProps {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                label: String::new(),
                color: None,
                page: 0,
            },
            function: None,
            children: Vec::new(),
        };
        let mut show = Show::new("t");
        show.virtual_console.widgets = vec![
            widget(1, WidgetKind::SpeedDial),
            widget(
                2,
                WidgetKind::Button {
                    action: ButtonAction::TapTempo,
                },
            ),
            widget(
                3,
                WidgetKind::Button {
                    action: ButtonAction::Toggle,
                },
            ),
        ];
        let mut rt = RuntimeSnapshot::default();
        rt.state.tempo.bpm = 120.0;
        rt.state.tempo.beats_per_bar = 4;
        let id = |n| klight_model::WidgetId(n);
        // Start of a beat: lit; mid-beat: dark.
        assert_eq!(widget_state(&show, &rt, id(1), 8.0), WidgetState::Active);
        assert_eq!(widget_state(&show, &rt, id(2), 8.0), WidgetState::Active);
        assert_eq!(widget_state(&show, &rt, id(1), 8.5), WidgetState::Inactive);
        assert_eq!(widget_state(&show, &rt, id(2), 8.5), WidgetState::Inactive);
        // A normal button does not follow the beat.
        assert_eq!(widget_state(&show, &rt, id(3), 8.0), WidgetState::Inactive);
    }

    #[test]
    fn tap_tempo_and_blackout_mappings_light_their_leds() {
        use klight_model::input_map::{InputMapping, InputTarget};
        use klight_model::MappingId;
        let on_off = ControllerProfile::from_toml(
            "name = \"x\"
kind = \"on_off\"
",
        )
        .unwrap();
        let note = |n| InputMessage::MidiNote {
            channel: 0,
            note: n,
        };
        let mut show = Show::new("t");
        show.input_mappings = vec![
            InputMapping {
                id: MappingId(1),
                device: "pad".into(),
                message: note(122),
                target: InputTarget::TapTempo,
            },
            InputMapping {
                id: MappingId(2),
                device: "pad".into(),
                message: note(123),
                target: InputTarget::Blackout,
            },
            InputMapping {
                id: MappingId(3),
                device: "pad".into(),
                message: note(124),
                target: InputTarget::GrandMaster,
            },
        ];
        let mut rt = RuntimeSnapshot::default();
        rt.state.tempo.bpm = 120.0;
        rt.state.tempo.beats_per_bar = 4;
        let mut tracker = FeedbackTracker::default();
        let at = |beat| FeedbackTime {
            blink_on: false,
            beat,
        };
        // Beat start: tap lit; blackout off; grand master has no LED.
        let sent = tracker.update(&show, &rt, "pad", &on_off, at(8.0), 100);
        assert_eq!(sent, vec![vec![0x90, 122, 127], vec![0x90, 123, 0]]);
        // Mid-beat: tap dark again; nothing else changes.
        let sent = tracker.update(&show, &rt, "pad", &on_off, at(8.5), 100);
        assert_eq!(sent, vec![vec![0x90, 122, 0]]);
        // Blackout on lights its LED.
        rt.state.blackout = true;
        let sent = tracker.update(&show, &rt, "pad", &on_off, at(8.5), 100);
        assert_eq!(sent, vec![vec![0x90, 123, 127]]);
    }

    #[test]
    fn leds_left_by_an_earlier_session_are_cleared_at_start() {
        use klight_model::input_map::{InputMapping, InputTarget};
        use klight_model::MappingId;
        let def = klight_fixtures::parse_qxi(
            r##"<InputProfile>
 <Channel Number="135"><Name>Pad</Name><Type>Button</Type><Feedback LowerValue="1" UpperValue="26" MidiChannel="6"/></Channel>
 <Channel Number="136"><Name>Pad</Name><Type>Button</Type><Feedback LowerValue="1" UpperValue="26" MidiChannel="6"/></Channel>
 <Channel Number="48"><Name>Slider</Name><Type>Slider</Type></Channel>
 <ColorTable><Color Value="10" Label="Red" RGB="#ff0000"/></ColorTable>
 <MidiChannelTable><Channel Value="6" Label="On 100% brightness"/></MidiChannelTable>
</InputProfile>"##,
        )
        .unwrap();
        let p = ControllerProfile::from_qxi(&def).unwrap();
        // Note 8 is mapped; note 7 is not (its mapping was removed earlier).
        let mut show = Show::new("t");
        show.input_mappings = vec![InputMapping {
            id: MappingId(1),
            device: "pad".into(),
            message: InputMessage::MidiNote {
                channel: 0,
                note: 8,
            },
            target: InputTarget::Blackout,
        }];
        let rt = RuntimeSnapshot::default();
        let at = FeedbackTime::default();
        let mut tracker = FeedbackTracker::default();
        let sent = tracker.update(&show, &rt, "pad", &p, at, 100);
        // The unmapped pad is darkened; the mapped one gets its own state.
        assert!(sent.contains(&vec![0x90, 7, 0]), "{sent:?}");
        assert_eq!(sent.iter().filter(|b| b.get(1) == Some(&8)).count(), 1);
        // Only once.
        assert!(tracker.update(&show, &rt, "pad", &p, at, 100).is_empty());
        // After a resync (device reconnected) it is done again.
        tracker.resync();
        assert!(tracker
            .update(&show, &rt, "pad", &p, at, 100)
            .contains(&vec![0x90, 7, 0]));
    }

    #[test]
    fn unmapped_controls_are_switched_off() {
        use klight_model::input_map::{InputMapping, InputTarget};
        use klight_model::MappingId;
        let mk2 = mk2();
        let pad = InputMessage::MidiNote {
            channel: 0,
            note: 11,
        };
        let mut show = Show::new("t");
        show.input_mappings = vec![InputMapping {
            id: MappingId(1),
            device: "pad".into(),
            message: pad.clone(),
            target: InputTarget::TapTempo,
        }];
        let mut rt = RuntimeSnapshot::default();
        rt.state.tempo.bpm = 120.0;
        rt.state.tempo.beats_per_bar = 4;
        let mut tracker = FeedbackTracker::default();
        let at = FeedbackTime {
            blink_on: false,
            beat: 8.0,
        };
        // Lit while mapped (note-on on channel 6).
        let lit = tracker.update(&show, &rt, "pad", &mk2, at, 100);
        // (The mk2 test profile's other pad, note 0, is darkened at start.)
        let for_pad = |sent: &[Vec<u8>]| {
            sent.iter()
                .filter(|b| b.get(1) == Some(&11))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(for_pad(&lit).len(), 1);
        assert_eq!(
            for_pad(&lit).first().map(|b| &b[..2]),
            Some(&[0x96, 11][..])
        );
        // Mapping removed: one message that darkens the pad, then silence.
        show.input_mappings.clear();
        assert_eq!(
            tracker.update(&show, &rt, "pad", &mk2, at, 100),
            vec![vec![0x90, 11, 0]]
        );
        assert!(tracker.update(&show, &rt, "pad", &mk2, at, 100).is_empty());
    }

    fn on_off() -> ControllerProfile {
        ControllerProfile::from_toml(
            "name = \"x\"
kind = \"on_off\"
",
        )
        .unwrap()
    }

    fn map(
        id: u32,
        message: InputMessage,
        target: InputTarget,
    ) -> klight_model::input_map::InputMapping {
        klight_model::input_map::InputMapping {
            id: klight_model::MappingId(id),
            device: "pad".into(),
            message,
            target,
        }
    }

    /// Regression: the input loop resyncs on every show change, and removing
    /// a mapping is a show change. The resync forgot which LEDs were lit, so
    /// a profile without a list of LED controls never darkened the pad.
    #[test]
    fn unmapped_control_is_switched_off_across_a_resync() {
        let p = on_off();
        let mut show = Show::new("t");
        show.input_mappings = vec![map(1, NOTE, InputTarget::Blackout)];
        let mut rt = RuntimeSnapshot::default();
        rt.state.blackout = true;
        let at = FeedbackTime::default();
        let mut tracker = FeedbackTracker::default();
        assert_eq!(
            tracker.update(&show, &rt, "pad", &p, at, 100),
            vec![vec![0x90, 36, 127]]
        );
        show.input_mappings.clear();
        tracker.resync();
        assert_eq!(
            tracker.update(&show, &rt, "pad", &p, at, 100),
            vec![vec![0x90, 36, 0]]
        );
        assert!(tracker.update(&show, &rt, "pad", &p, at, 100).is_empty());
    }

    #[test]
    fn resync_resends_mapped_leds() {
        let p = on_off();
        let mut show = Show::new("t");
        show.input_mappings = vec![map(1, NOTE, InputTarget::Blackout)];
        let rt = RuntimeSnapshot::default();
        let at = FeedbackTime::default();
        let mut tracker = FeedbackTracker::default();
        assert_eq!(tracker.update(&show, &rt, "pad", &p, at, 100).len(), 1);
        assert!(tracker.update(&show, &rt, "pad", &p, at, 100).is_empty());
        tracker.resync();
        assert_eq!(
            tracker.update(&show, &rt, "pad", &p, at, 100),
            vec![vec![0x90, 36, 0]]
        );
    }

    /// Regression: one control mapped to two targets in different states made
    /// the LED flip between them and sent two messages on every update.
    #[test]
    fn control_mapped_twice_shows_its_first_mapping_only() {
        let p = on_off();
        let mut show = Show::new("t");
        show.input_mappings = vec![
            map(1, NOTE, InputTarget::Blackout),
            map(2, NOTE, InputTarget::TapTempo),
        ];
        let mut rt = RuntimeSnapshot::default();
        rt.state.blackout = true;
        rt.state.tempo.bpm = 120.0;
        rt.state.tempo.beats_per_bar = 4;
        // Mid-beat: tap tempo would be dark, blackout is lit.
        let at = FeedbackTime {
            blink_on: false,
            beat: 8.5,
        };
        let mut tracker = FeedbackTracker::default();
        assert_eq!(
            tracker.update(&show, &rt, "pad", &p, at, 100),
            vec![vec![0x90, 36, 127]]
        );
        assert!(tracker.update(&show, &rt, "pad", &p, at, 100).is_empty());
    }

    #[test]
    fn palette_colors_are_validated_when_loaded() {
        let bad = "name = \"p\"
kind = \"palette\"
colors = [ { value = 1, color = \"red\" } ]
";
        assert!(ControllerProfile::from_toml(bad).is_err());
        let empty = "name = \"p\"
kind = \"palette\"
colors = []
";
        assert!(ControllerProfile::from_toml(empty).is_err());
    }

    #[test]
    fn qxi_round_buttons_are_plain_on_off_leds() {
        let def = klight_fixtures::parse_qxi(
            r##"<InputProfile>
 <Channel Number="128"><Name>Pad</Name><Type>Button</Type><Feedback LowerValue="1" UpperValue="26" MidiChannel="6"/></Channel>
 <Channel Number="245"><Name>Drum Button</Name><Type>Button</Type><Feedback LowerValue="1" UpperValue="3"/></Channel>
 <ColorTable><Color Value="10" Label="Red" RGB="#ff0000"/></ColorTable>
 <MidiChannelTable><Channel Value="6" Label="On 100% brightness"/></MidiChannelTable>
</InputProfile>"##,
        )
        .unwrap();
        let p = ControllerProfile::from_qxi(&def).unwrap();
        assert_eq!(
            p.simple_leds,
            vec![SimpleLed {
                note: true,
                index: 117,
                on: 1
            }]
        );
        let drum = InputMessage::MidiNote {
            channel: 0,
            note: 117,
        };
        // Colors do not apply: on is value 1 on the button's own channel.
        assert_eq!(
            p.render(&drum, WidgetState::Active, GREEN, true),
            Some(vec![0x90, 117, 1])
        );
        assert_eq!(
            p.render(&drum, WidgetState::Inactive, GREEN, true),
            Some(vec![0x90, 117, 0])
        );
        // Pads keep the color model.
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![0x96, 36, 5])
        );
    }

    #[test]
    fn qxi_profile_without_colors_is_on_off_and_without_feedback_is_none() {
        let def = klight_fixtures::parse_qxi(
            r#"<InputProfile><Channel Number="1"><Name>a</Name><Feedback LowerValue="0" UpperValue="100"/></Channel></InputProfile>"#,
        )
        .unwrap();
        let p = ControllerProfile::from_qxi(&def).unwrap();
        assert_eq!(
            p.render(&NOTE, WidgetState::Active, GREEN, true),
            Some(vec![0x90, 36, 100])
        );
        let none =
            klight_fixtures::parse_qxi("<InputProfile><Model>x</Model></InputProfile>").unwrap();
        assert!(ControllerProfile::from_qxi(&none).is_none());
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
