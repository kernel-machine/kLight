//! Translation of input events into commands, using the mappings stored in
//! the show. Includes soft takeover for faders and knobs.

use std::collections::BTreeMap;

use klight_model::input_map::{InputMessage, InputTarget};
use klight_model::vc::{SliderMode, WidgetKind};
use klight_model::{Command, RuntimeSnapshot, Show, WidgetId};

use crate::midi::MidiEvent;

/// A device-independent input event.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Midi(MidiEvent),
    /// OSC message with its first numeric argument (if any), normalised by the sender.
    Osc {
        path: String,
        value: Option<f32>,
    },
}

impl InputEvent {
    /// The mapping key and normalised value (0..=1) of the event.
    pub fn key_value(&self) -> Option<(InputMessage, f32)> {
        match self {
            InputEvent::Midi(MidiEvent::NoteOn {
                channel,
                note,
                velocity,
            }) => Some((
                InputMessage::MidiNote {
                    channel: *channel,
                    note: *note,
                },
                f32::from(*velocity) / 127.0,
            )),
            InputEvent::Midi(MidiEvent::NoteOff { channel, note, .. }) => Some((
                InputMessage::MidiNote {
                    channel: *channel,
                    note: *note,
                },
                0.0,
            )),
            InputEvent::Midi(MidiEvent::ControlChange {
                channel,
                controller,
                value,
            }) => Some((
                InputMessage::MidiCc {
                    channel: *channel,
                    controller: *controller,
                },
                f32::from(*value) / 127.0,
            )),
            InputEvent::Osc { path, value } => Some((
                InputMessage::Osc { path: path.clone() },
                value
                    .filter(|v| v.is_finite())
                    .map_or(1.0, |v| v.clamp(0.0, 1.0)),
            )),
            _ => None,
        }
    }
}

/// Soft takeover: a physical control only takes effect once it has "caught"
/// the current value, so a fader in the wrong position does not jump.
#[derive(Debug, Default, Clone)]
pub struct SoftTakeover {
    engaged: bool,
    last_input: Option<f32>,
    last_sent: Option<f32>,
}

/// Values closer than this count as "caught".
pub const TAKEOVER_TOLERANCE: f32 = 2.0 / 127.0;

impl SoftTakeover {
    /// Returns the value to apply, or None while not engaged.
    pub fn feed(&mut self, input: f32, current: f32) -> Option<f32> {
        // Someone else moved the target: disengage.
        if let Some(sent) = self.last_sent {
            if (sent - current).abs() > TAKEOVER_TOLERANCE {
                self.engaged = false;
                self.last_sent = None;
            }
        }
        if !self.engaged {
            let close = (input - current).abs() <= TAKEOVER_TOLERANCE;
            let crossed = self
                .last_input
                .is_some_and(|prev| (prev - current).signum() != (input - current).signum());
            if close || crossed {
                self.engaged = true;
            }
        }
        self.last_input = Some(input);
        if self.engaged {
            self.last_sent = Some(input);
            Some(input)
        } else {
            None
        }
    }
}

/// Stateful translator (soft takeover and button edges per control).
#[derive(Debug, Default)]
pub struct Mapper {
    takeover: BTreeMap<(String, InputMessage), SoftTakeover>,
    pressed: BTreeMap<(String, InputMessage), bool>,
}

fn widget_value(show: &Show, rt: &RuntimeSnapshot, id: WidgetId) -> f32 {
    show.virtual_console
        .find(id)
        .map(|w| match (&w.kind, w.function) {
            (
                WidgetKind::Slider {
                    mode: SliderMode::GrandMaster,
                },
                _,
            ) => rt.state.grand_master,
            (_, Some(f)) => rt
                .state
                .active
                .iter()
                .find(|a| a.id == f)
                .map_or(0.0, |a| a.level),
            _ => 0.0,
        })
        .unwrap_or(0.0)
}

impl Mapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget takeover state (e.g. after a device reconnects).
    pub fn reset_device(&mut self, device: &str) {
        self.takeover.retain(|(d, _), _| d != device);
        self.pressed.retain(|(d, _), _| d != device);
    }

    pub fn translate(
        &mut self,
        show: &Show,
        rt: &RuntimeSnapshot,
        device: &str,
        event: &InputEvent,
    ) -> Vec<Command> {
        let Some((key, value)) = event.key_value() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for m in show
            .input_mappings
            .iter()
            .filter(|m| m.device == device && m.message == key)
        {
            let k = (device.to_owned(), key.clone());
            let is_button_like = matches!(key, InputMessage::MidiNote { .. })
                || matches!(&m.target, InputTarget::TapTempo | InputTarget::Blackout);
            let pressed_now = value >= 0.5;
            let edge = |pressed: &mut BTreeMap<(String, InputMessage), bool>| {
                let was = pressed.insert(k.clone(), pressed_now).unwrap_or(false);
                (pressed_now && !was, !pressed_now && was)
            };
            match &m.target {
                InputTarget::TapTempo => {
                    if edge(&mut self.pressed).0 {
                        out.push(Command::TapTempo);
                    }
                }
                InputTarget::Blackout => {
                    if edge(&mut self.pressed).0 {
                        out.push(Command::SetBlackout {
                            on: !rt.state.blackout,
                        });
                    }
                }
                InputTarget::GrandMaster => {
                    let st = self.takeover.entry(k.clone()).or_default();
                    if let Some(v) = st.feed(value, rt.state.grand_master) {
                        out.push(Command::SetGrandMaster { level: v });
                    }
                }
                InputTarget::Widget { id } => {
                    let Some(w) = show.virtual_console.find(*id) else {
                        continue;
                    };
                    match &w.kind {
                        WidgetKind::Slider { .. } | WidgetKind::Knob if !is_button_like => {
                            let current = widget_value(show, rt, *id);
                            let st = self.takeover.entry(k.clone()).or_default();
                            if let Some(v) = st.feed(value, current) {
                                out.push(Command::WidgetSetValue { id: *id, value: v });
                            }
                        }
                        WidgetKind::SpeedDial if !is_button_like => {
                            // CC maps 0..127 to 40..240 BPM.
                            out.push(Command::WidgetSetValue {
                                id: *id,
                                value: 40.0 + value * 200.0,
                            });
                        }
                        _ => {
                            let (down, up) = edge(&mut self.pressed);
                            if down {
                                out.push(Command::WidgetPress { id: *id });
                            } else if up {
                                out.push(Command::WidgetRelease { id: *id });
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_takeover_waits_for_catch() {
        let mut st = SoftTakeover::default();
        // Current value 0.5; fader at 0.0 moving up.
        assert_eq!(st.feed(0.0, 0.5), None);
        assert_eq!(st.feed(0.3, 0.5), None);
        assert_eq!(st.feed(0.6, 0.5), Some(0.6)); // crossed
        assert_eq!(st.feed(0.7, 0.6), Some(0.7));
        // Value changed elsewhere (UI): disengage.
        assert_eq!(st.feed(0.72, 0.1), None);
        assert_eq!(st.feed(0.11, 0.1), Some(0.11));
    }
}
