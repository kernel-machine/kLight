//! Virtual console: a tree of widgets. Behaviours (solo frames, cue
//! advance, ...) are core rules, not UI logic.

use serde::{Deserialize, Serialize};

use crate::fixture_def::Rgb8;
use crate::ids::{FunctionId, WidgetId};

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
