//! Virtual console: a tree of widgets. Behaviours (solo frames, cue
//! advance, ...) are core rules, not UI logic.

use serde::{Deserialize, Serialize};

use crate::fixture_def::Rgb8;
use crate::ids::{FunctionId, WidgetId};
use crate::runtime::ActiveFunction;

/// Whether a widget is lit, on screen and on controller LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WidgetLight {
    #[default]
    Off,
    On,
    /// What lights the widget is paused.
    Paused,
}

/// Light of a widget bound to `function`. A blackout button follows the
/// blackout. Other widgets are lit while their function runs; a button that
/// starts it ([`ButtonAction::Toggle`] or [`ButtonAction::Flash`]) is also lit
/// while a step of a running chaser plays it. The function's own state wins
/// over the chasers', and a running chaser over a paused one.
pub fn widget_light(
    kind: &WidgetKind,
    function: Option<FunctionId>,
    blackout: bool,
    active: &[ActiveFunction],
) -> WidgetLight {
    if matches!(
        kind,
        WidgetKind::Button {
            action: ButtonAction::Blackout
        }
    ) {
        return if blackout {
            WidgetLight::On
        } else {
            WidgetLight::Off
        };
    }
    let Some(f) = function else {
        return WidgetLight::Off;
    };
    let light = |paused| {
        if paused {
            WidgetLight::Paused
        } else {
            WidgetLight::On
        }
    };
    if let Some(own) = active.iter().find(|a| a.id == f) {
        return light(own.paused);
    }
    if !matches!(
        kind,
        WidgetKind::Button {
            action: ButtonAction::Toggle | ButtonAction::Flash
        }
    ) {
        return WidgetLight::Off;
    }
    // `false < true`: a running chaser wins over a paused one.
    active
        .iter()
        .filter(|a| a.step_function == Some(f))
        .map(|a| a.paused)
        .min()
        .map_or(WidgetLight::Off, light)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ButtonAction {
    /// Press starts, press again stops.
    #[default]
    Toggle,
    /// Active while held.
    Flash,
    Blackout,
    StopAll,
    TapTempo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SliderMode {
    /// Controls the level of the attached function.
    #[default]
    Playback,
    /// Scales the intensity of the attached function.
    Submaster,
    /// Controls the grand master.
    GrandMaster,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetKind {
    Frame {
        pages: u16,
    },
    /// Only one child function active at a time.
    SoloFrame,
    Button {
        action: ButtonAction,
    },
    Slider {
        mode: SliderMode,
    },
    Knob,
    XyPad,
    SpeedDial,
    CueList,
    Animation,
    Label,
    Clock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WidgetProps {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub label: String,
    pub color: Option<Rgb8>,
    /// Page of the parent frame the widget belongs to.
    #[serde(default)]
    pub page: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Widget {
    pub id: WidgetId,
    pub kind: WidgetKind,
    pub props: WidgetProps,
    #[serde(default)]
    pub function: Option<FunctionId>,
    #[serde(default)]
    pub children: Vec<Widget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VirtualConsole {
    pub widgets: Vec<Widget>,
}

impl VirtualConsole {
    /// Depth-first search for a widget.
    pub fn find(&self, id: WidgetId) -> Option<&Widget> {
        fn walk(ws: &[Widget], id: WidgetId) -> Option<&Widget> {
            for w in ws {
                if w.id == id {
                    return Some(w);
                }
                if let Some(found) = walk(&w.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.widgets, id)
    }

    /// Parent of a widget (None for top-level widgets or unknown ids).
    pub fn parent_of(&self, id: WidgetId) -> Option<&Widget> {
        fn walk(ws: &[Widget], id: WidgetId) -> Option<&Widget> {
            for w in ws {
                if w.children.iter().any(|c| c.id == id) {
                    return Some(w);
                }
                if let Some(found) = walk(&w.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.widgets, id)
    }

    pub fn for_each<'a>(&'a self, f: &mut impl FnMut(&'a Widget)) {
        fn walk<'a>(ws: &'a [Widget], f: &mut impl FnMut(&'a Widget)) {
            for w in ws {
                f(w);
                walk(&w.children, f);
            }
        }
        walk(&self.widgets, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: FunctionId = FunctionId(1);
    const CHASER: FunctionId = FunctionId(10);

    fn button(action: ButtonAction) -> WidgetKind {
        WidgetKind::Button { action }
    }

    fn running(id: FunctionId, step_function: Option<FunctionId>, paused: bool) -> ActiveFunction {
        ActiveFunction {
            id,
            level: 1.0,
            position: None,
            paused,
            step_function,
        }
    }

    fn light(kind: WidgetKind, active: &[ActiveFunction]) -> WidgetLight {
        widget_light(&kind, Some(SCENE), false, active)
    }

    #[test]
    fn buttons_light_while_a_chaser_plays_their_function() {
        let playing = [running(CHASER, Some(SCENE), false)];
        for action in [ButtonAction::Toggle, ButtonAction::Flash] {
            assert_eq!(light(button(action), &playing), WidgetLight::On);
        }
        let other = [running(CHASER, Some(FunctionId(2)), false)];
        assert_eq!(
            light(button(ButtonAction::Toggle), &other),
            WidgetLight::Off
        );
        assert_eq!(light(button(ButtonAction::Toggle), &[]), WidgetLight::Off);
    }

    #[test]
    fn only_start_buttons_follow_chaser_steps() {
        let playing = [running(CHASER, Some(SCENE), false)];
        for kind in [
            WidgetKind::Slider {
                mode: SliderMode::Playback,
            },
            WidgetKind::Knob,
            button(ButtonAction::StopAll),
        ] {
            assert_eq!(light(kind, &playing), WidgetLight::Off);
        }
    }

    #[test]
    fn own_state_wins_over_chasers_and_running_over_paused() {
        let toggle = || button(ButtonAction::Toggle);
        let paused = running(CHASER, Some(SCENE), true);
        let other = running(FunctionId(11), Some(SCENE), false);
        assert_eq!(
            light(toggle(), std::slice::from_ref(&paused)),
            WidgetLight::Paused
        );
        assert_eq!(light(toggle(), &[paused.clone(), other]), WidgetLight::On);
        let own = running(SCENE, None, false);
        assert_eq!(light(toggle(), &[paused, own]), WidgetLight::On);
    }

    #[test]
    fn blackout_buttons_follow_the_blackout_only() {
        let kind = button(ButtonAction::Blackout);
        let own = [running(SCENE, None, false)];
        assert_eq!(
            widget_light(&kind, Some(SCENE), false, &own),
            WidgetLight::Off
        );
        assert_eq!(widget_light(&kind, None, true, &[]), WidgetLight::On);
    }
}
