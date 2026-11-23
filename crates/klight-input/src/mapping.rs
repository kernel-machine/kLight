//! Translation of input events into commands, using the mappings stored in
//! the show. Includes soft takeover for faders and knobs.

use std::collections::{BTreeMap, VecDeque};

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
    /// Target value seen at the previous event.
    last_seen: Option<f32>,
    /// Values sent that no snapshot has shown yet (plus the last one shown).
    /// The target value reaches us through a snapshot published after the
    /// command is applied, so it lags behind what was sent while the control
    /// moves fast. Snapshots show the sent values in order, so everything
    /// before the value a snapshot shows can never show up again and is
    /// dropped: an old value set again from elsewhere is not mistaken for
    /// ours.
    recent_sent: VecDeque<f32>,
}

/// Values closer than this count as "caught".
pub const TAKEOVER_TOLERANCE: f32 = 2.0 / 127.0;

/// Values we sent come back through the engine's 16-bit levels, so matching
/// them needs no tolerance beyond that quantisation (< 1e-5).
const SENT_MATCH: f32 = 1e-4;

/// How many sent values are remembered to recognise a lagging snapshot.
const RECENT_SENT: usize = 256;

impl SoftTakeover {
    /// Returns the value to apply, or None while not engaged.
    pub fn feed(&mut self, input: f32, current: f32) -> Option<f32> {
        // Someone else moved the target: disengage. A change to a value we
        // sent ourselves is only the snapshot catching up.
        if self.engaged {
            let changed = self
                .last_seen
                .is_some_and(|seen| (seen - current).abs() > SENT_MATCH);
            if changed {
                match self
                    .recent_sent
                    .iter()
                    .position(|sent| (sent - current).abs() <= SENT_MATCH)
                {
                    Some(shown) => {
                        self.recent_sent.drain(..shown);
                    }
                    None => {
                        self.engaged = false;
                        self.recent_sent.clear();
                    }
                }
            }
        }
        if !self.engaged {
            let close = (input - current).abs() <= TAKEOVER_TOLERANCE;
            let crossed = self
                .last_input
                .is_some_and(|prev| (prev - current).signum() != (input - current).signum());
            if close || crossed {
                self.engaged = true;
                self.recent_sent.clear();
            }
        }
        self.last_input = Some(input);
        self.last_seen = Some(current);
        if self.engaged {
            if self.recent_sent.len() == RECENT_SENT {
                self.recent_sent.pop_front();
            }
            self.recent_sent.push_back(input);
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

    /// The engine value reaches the mapper through a snapshot published a
    /// little later than the command, so while a fader moves fast the
    /// observed value lags behind what was sent. That is not "someone else
    /// moved it": the control must stay engaged.
    #[test]
    fn soft_takeover_survives_lagging_snapshot() {
        let mut st = SoftTakeover::default();
        assert_eq!(st.feed(1.0, 1.0), Some(1.0));
        let inputs: Vec<f32> = (1..=100).map(|i| 1.0 - i as f32 / 127.0).collect();
        for lag in [1_usize, 4, 10] {
            for (i, &input) in inputs.iter().enumerate() {
                let seen = if i >= lag {
                    inputs.get(i - lag).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                assert_eq!(st.feed(input, seen), Some(input), "lag {lag}, step {i}");
            }
            // Back to the top, still lagging.
            assert_eq!(st.feed(1.0, 1.0), Some(1.0));
        }
    }

    /// Regression: after a full sweep every common value had been sent, so a
    /// value set from the UI looked like ours and the fader stayed engaged,
    /// making the target jump on the next move.
    #[test]
    fn soft_takeover_lets_go_after_a_sweep_when_set_elsewhere() {
        let mut st = SoftTakeover::default();
        let mut seen = 0.0;
        let sweep = (0..=127).chain((51..127).rev()).map(|i| i as f32 / 127.0);
        for input in sweep {
            assert_eq!(st.feed(input, seen), Some(input));
            seen = input;
        }
        // Fader parked at 51/127; the operator sets full from the UI.
        assert_eq!(st.feed(50.0 / 127.0, 1.0), None);
    }

    /// A fader reversing while snapshots lag sends the same value twice; the
    /// older one being shown must not discard the values sent after it.
    #[test]
    fn soft_takeover_survives_reversal_with_lag() {
        let mut st = SoftTakeover::default();
        assert_eq!(st.feed(0.5, 0.5), Some(0.5));
        let sent = [0.51_f32, 0.52, 0.51, 0.5, 0.51];
        let shown = [0.5_f32, 0.5, 0.51, 0.52, 0.51];
        for (input, current) in sent.into_iter().zip(shown) {
            assert_eq!(st.feed(input, current), Some(input), "{input} {current}");
        }
    }

    /// The engine stores levels in 16 bits; what it reports back must still
    /// match every value a 7-bit control sends.
    #[test]
    fn engine_level_quantisation_matches_sent_values() {
        for i in 0..=127_u8 {
            let sent = f32::from(i) / 127.0;
            let back = f32::from(klight_fixtures::convert::unit_to16(sent)) / 65535.0;
            assert!((back - sent).abs() <= SENT_MATCH, "{i}");
        }
    }

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
