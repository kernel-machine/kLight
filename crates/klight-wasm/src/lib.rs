//! The core's pure functions for the browser.
//!
//! The UI must never re-implement core logic: movement and matrix previews,
//! group layouts, patch checks and color conversions come from here, so a
//! preview is identical to what the engine outputs by construction.
//!
//! Each function has a plain Rust version (tested natively) and a thin
//! `wasm_bindgen` wrapper. Structured data crosses the boundary as JSON;
//! numeric series as typed arrays.

use klight_engine_core::compile::CompiledShow;
use klight_engine_core::effects::{head_phase, matrix_cell_color, matrix_phase, movement_point};
use klight_engine_core::preview::scene_channels;
use klight_engine_core::scene_edit;
use klight_fixtures::color_sequence;
use klight_model::inspect::{self, SceneFixtureChannels};
use klight_model::vc::{widget_light, WidgetKind, WidgetLight};
use klight_model::{
    ActiveFunction, ChannelRef, Color, FixtureId, FunctionCategory, FunctionId, GroupId,
    GroupLayout, MatrixEffect, MovementEffect, Rgb8, SceneValue, Show, TimeValue, UniverseId,
};
use wasm_bindgen::prelude::*;

fn parse<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("invalid {what}: {e}"))
}

fn to_json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

// ---------------------------------------------------------------- show

/// Read-only view of a show (create one per show version).
#[wasm_bindgen]
pub struct ShowView {
    show: Show,
    /// Compiled once, on first use (same compilation the engine runs).
    compiled: std::cell::OnceCell<CompiledShow>,
}

impl ShowView {
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(Self {
            show: parse(json, "show")?,
            compiled: std::cell::OnceCell::new(),
        })
    }

    fn compiled(&self) -> &CompiledShow {
        self.compiled
            .get_or_init(|| CompiledShow::build(&self.show))
    }

    pub fn scene_channels_of(&self, id: u32) -> Option<Vec<SceneFixtureChannels>> {
        scene_channels(&self.show, self.compiled(), FunctionId(id))
    }

    pub fn add_scene_values_of(&self, values: &str, incoming: &str) -> Result<String, String> {
        let values: Vec<SceneValue> = parse(values, "scene values")?;
        let incoming: Vec<SceneValue> = parse(incoming, "scene values")?;
        Ok(to_json(&scene_edit::add_scene_values(
            &self.show,
            self.compiled(),
            &values,
            &incoming,
        )))
    }

    pub fn function_color_of(&self, id: u32) -> Option<Color> {
        inspect::function_color(&self.show, FunctionId(id))
    }

    pub fn scene_value_color_of(&self, id: u32, index: usize) -> Option<Color> {
        let f = self.show.function(FunctionId(id))?;
        let klight_model::FunctionKind::Scene(scene) = &f.kind else {
            return None;
        };
        inspect::scene_value_color(&self.show, scene.values.get(index)?)
    }

    pub fn group_layout_of(&self, group: u32) -> Option<GroupLayout> {
        self.show.group(GroupId(group)).map(|g| g.layout())
    }
}

#[wasm_bindgen]
impl ShowView {
    #[wasm_bindgen(constructor)]
    pub fn new(show_json: &str) -> Result<ShowView, JsError> {
        Self::from_json(show_json).map_err(|e| JsError::new(&e))
    }

    /// First free address (1-based) for `size` channels, or undefined.
    pub fn next_free_address(&self, universe: u32, size: u16, from: u16) -> Option<u16> {
        inspect::next_free_address(&self.show, UniverseId(universe), size, from)
    }

    /// Ids of the fixtures a span would overlap.
    pub fn overlapping(
        &self,
        universe: u32,
        address: u16,
        size: u16,
        exclude: Option<u32>,
    ) -> Vec<u32> {
        inspect::overlapping(
            &self.show,
            UniverseId(universe),
            address,
            size,
            exclude.map(FixtureId),
        )
        .into_iter()
        .map(|f| f.0)
        .collect()
    }

    /// JSON array of 512 `ChannelInfo | null`.
    pub fn channel_map(&self, universe: u32) -> String {
        to_json(&inspect::channel_map(&self.show, UniverseId(universe)))
    }

    /// JSON `FixtureInfo`, or undefined for an unknown fixture.
    pub fn fixture_info(&self, id: u32) -> Option<String> {
        let f = self.show.fixture(FixtureId(id))?;
        Some(to_json(&inspect::fixture_info(&self.show, f)))
    }

    /// JSON `ValueSupport`: which kinds of scene values some fixtures can
    /// act on (intensity, color, position, gobo, prism, zoom, strobe).
    pub fn value_support(&self, fixtures: Vec<u32>) -> String {
        let ids: Vec<FixtureId> = fixtures.into_iter().map(FixtureId).collect();
        to_json(&inspect::value_support(&self.show, &ids))
    }

    /// JSON `ColorChoices` for some fixtures: wheel colors to pick from and
    /// whether any color makes sense.
    pub fn color_choices(&self, fixtures: Vec<u32>) -> String {
        let ids: Vec<FixtureId> = fixtures.into_iter().map(FixtureId).collect();
        to_json(&inspect::color_choices(&self.show, &ids))
    }

    /// JSON `ChannelPreset[]`: the named ranges of the channel at `address`
    /// (color wheel slots, gobos, shutter modes...), empty if it has none.
    pub fn channel_presets(&self, universe: u32, address: u16) -> String {
        to_json(&inspect::channel_presets(
            &self.show,
            UniverseId(universe),
            address,
        ))
    }

    /// JSON `SceneFixtureChannels[]`: all the channels of the fixtures a
    /// scene drives, with their values; undefined if the function is not a
    /// scene.
    pub fn scene_channels(&self, id: u32) -> Option<String> {
        self.scene_channels_of(id).map(|p| to_json(&p))
    }

    /// JSON `SceneValue[]`: scene `values` after adding `incoming` (both JSON
    /// `SceneValue[]`).
    pub fn add_scene_values(&self, values: &str, incoming: &str) -> Result<String, JsError> {
        self.add_scene_values_of(values, incoming)
            .map_err(|e| JsError::new(&e))
    }

    /// Linear RGB `[r, g, b]` of the color a function shows (first color its
    /// scene sets), or undefined.
    pub fn function_color(&self, id: u32) -> Option<Vec<f32>> {
        self.function_color_of(id).map(|c| vec![c.r, c.g, c.b])
    }

    /// Linear RGB of the color the `index`-th value of a scene sets, or
    /// undefined.
    pub fn scene_value_color(&self, id: u32, index: u32) -> Option<Vec<f32>> {
        self.scene_value_color_of(id, index as usize)
            .map(|c| vec![c.r, c.g, c.b])
    }

    /// JSON `GroupLayout` (the grid matrix effects use), or undefined.
    pub fn group_layout(&self, group: u32) -> Option<String> {
        self.group_layout_of(group).map(|l| to_json(&l))
    }
}

// ---------------------------------------------------------------- scene edits

fn set_scene_channel_json(
    values: &str,
    fixture: u32,
    channel: &str,
    value: Option<u8>,
) -> Result<String, String> {
    let values: Vec<SceneValue> = parse(values, "scene values")?;
    let channel: ChannelRef = parse(channel, "channel")?;
    Ok(to_json(&scene_edit::set_scene_channel(
        &values,
        FixtureId(fixture),
        channel,
        value,
    )))
}

/// JSON `SceneValue[]`: scene `values` with one channel of a fixture set by
/// hand to `value`, or released when `value` is undefined.
#[wasm_bindgen]
pub fn set_scene_channel(
    values: &str,
    fixture: u32,
    channel: &str,
    value: Option<u8>,
) -> Result<String, JsError> {
    set_scene_channel_json(values, fixture, channel, value).map_err(|e| JsError::new(&e))
}

// ---------------------------------------------------------------- movement

#[wasm_bindgen]
pub struct MovementView {
    effect: MovementEffect,
}

impl MovementView {
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(Self {
            effect: parse(json, "movement effect")?,
        })
    }

    /// `samples + 1` points of the whole shape: [pan0, tilt0, pan1, tilt1, ...].
    pub fn path_points(&self, samples: u32) -> Vec<f32> {
        let n = samples.max(1);
        (0..=n)
            .flat_map(|k| {
                let (p, t) = movement_point(&self.effect, f64::from(k) / f64::from(n), 0.0);
                [p, t]
            })
            .collect()
    }

    /// Position of each of `heads` heads at cycle position `turns`.
    pub fn head_points(&self, turns: f64, heads: u32) -> Vec<f32> {
        let n = heads.max(1) as usize;
        (0..n)
            .flat_map(|i| {
                let (p, t) = movement_point(
                    &self.effect,
                    turns,
                    head_phase(self.effect.phase_spread, i, n),
                );
                [p, t]
            })
            .collect()
    }
}

#[wasm_bindgen]
impl MovementView {
    #[wasm_bindgen(constructor)]
    pub fn new(effect_json: &str) -> Result<MovementView, JsError> {
        Self::from_json(effect_json).map_err(|e| JsError::new(&e))
    }

    pub fn path(&self, samples: u32) -> Vec<f32> {
        self.path_points(samples)
    }

    pub fn heads(&self, turns: f64, heads: u32) -> Vec<f32> {
        self.head_points(turns, heads)
    }
}

// ---------------------------------------------------------------- matrix

#[wasm_bindgen]
pub struct MatrixView {
    effect: MatrixEffect,
    layout: GroupLayout,
}

impl MatrixView {
    pub fn from_json(effect_json: &str, layout_json: &str) -> Result<Self, String> {
        Ok(Self {
            effect: parse(effect_json, "matrix effect")?,
            layout: parse(layout_json, "group layout")?,
        })
    }

    /// Linear RGB of each cell (layout order): [r0, g0, b0, r1, ...].
    pub fn cell_colors(&self, turns: f64) -> Vec<f32> {
        let phase = matrix_phase(turns, self.effect.direction);
        self.layout
            .positions
            .iter()
            .flat_map(|p| {
                let c = matrix_cell_color(
                    &self.effect.algorithm,
                    self.layout.width,
                    self.layout.height,
                    p.x,
                    p.y,
                    phase,
                );
                [c.r, c.g, c.b]
            })
            .collect()
    }
}

#[wasm_bindgen]
impl MatrixView {
    #[wasm_bindgen(constructor)]
    pub fn new(effect_json: &str, layout_json: &str) -> Result<MatrixView, JsError> {
        Self::from_json(effect_json, layout_json).map_err(|e| JsError::new(&e))
    }

    pub fn colors(&self, turns: f64) -> Vec<f32> {
        self.cell_colors(turns)
    }
}

// ---------------------------------------------------------------- colors

/// Linear RGB → `#rrggbb` (sRGB).
#[wasm_bindgen]
pub fn linear_to_hex(r: f32, g: f32, b: f32) -> String {
    let c = Color::new(r, g, b).to_srgb8();
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// `#rrggbb` (sRGB) → linear RGB `[r, g, b]` (black if malformed).
#[wasm_bindgen]
pub fn hex_to_linear(hex: &str) -> Vec<f32> {
    let c = Color::from_srgb8(Rgb8::parse_hex(hex).unwrap_or_default());
    vec![c.r, c.g, c.b]
}

/// Beat lamp at musical position `beat`: bit 0 = lit, bit 1 = first beat of a
/// bar. The engine uses the same function for controller LEDs.
#[wasm_bindgen]
pub fn beat_flash_bits(beat: f64, bpm: f64, beats_per_bar: u8) -> u8 {
    let f = klight_model::beat_flash(beat, bpm, beats_per_bar);
    u8::from(f.on) | (u8::from(f.down) << 1)
}

fn widget_light_json(
    kind: &str,
    function: Option<u32>,
    blackout: bool,
    active: &str,
) -> Result<WidgetLight, String> {
    let kind: WidgetKind = parse(kind, "widget kind")?;
    let active: Vec<ActiveFunction> = parse(active, "active functions")?;
    Ok(widget_light(
        &kind,
        function.map(FunctionId),
        blackout,
        &active,
    ))
}

/// Whether a widget bound to `function` is lit: 0 = off, 1 = on, 2 = paused.
/// `kind` is a JSON `WidgetKind`, `active` a JSON `ActiveFunction[]`. The
/// engine uses the same function for controller LEDs.
#[wasm_bindgen]
pub fn widget_light_code(
    kind: &str,
    function: Option<u32>,
    blackout: bool,
    active: &str,
) -> Result<u8, JsError> {
    let light =
        widget_light_json(kind, function, blackout, active).map_err(|e| JsError::new(&e))?;
    Ok(match light {
        WidgetLight::Off => 0,
        WidgetLight::On => 1,
        WidgetLight::Paused => 2,
    })
}

/// JSON `FunctionCategory[]`: the types of function a chaser step can play.
#[wasm_bindgen]
pub fn chaser_step_types() -> String {
    to_json(&FunctionCategory::CHASER_STEPS)
}

/// The beat lengths offered for chaser steps and fades and effect cycles.
#[wasm_bindgen]
pub fn beat_choices() -> Vec<f32> {
    TimeValue::BEAT_CHOICES.to_vec()
}

/// Most colors `color_sequence` can return.
#[wasm_bindgen]
pub fn max_sequence_colors() -> u32 {
    u32::try_from(color_sequence::MAX_SEQUENCE_COLORS).unwrap_or(u32::MAX)
}

/// JSON `{ name, color }[]`: `n` bright, mutually distinct, named colors
/// (linear RGB), in color-wheel order from red.
#[wasm_bindgen]
pub fn color_sequence_json(n: u32) -> String {
    #[derive(serde::Serialize)]
    struct Named {
        name: &'static str,
        color: Color,
    }
    let colors: Vec<Named> = color_sequence::color_sequence(n as usize)
        .into_iter()
        .map(|c| Named {
            name: c.name,
            color: c.color,
        })
        .collect();
    to_json(&colors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_flash_bits_match_the_core() {
        assert_eq!(beat_flash_bits(8.0, 120.0, 4), 0b11);
        assert_eq!(beat_flash_bits(9.0, 120.0, 4), 0b01);
        assert_eq!(beat_flash_bits(9.5, 120.0, 4), 0);
    }

    #[test]
    fn widget_light_json_shape() {
        let kind = r#"{"type":"button","action":"Toggle"}"#;
        let active = r#"[{"id":10,"level":1,"position":0,"paused":false,"step_function":1}]"#;
        assert_eq!(
            widget_light_json(kind, Some(1), false, active),
            Ok(WidgetLight::On)
        );
        assert_eq!(
            widget_light_json(kind, Some(2), false, active),
            Ok(WidgetLight::Off)
        );
        assert!(widget_light_json("{", None, false, "[]").is_err());
    }

    #[test]
    fn chaser_step_types_json_shape() {
        assert_eq!(
            chaser_step_types(),
            r#"["scene","movement_effect","matrix_effect"]"#
        );
    }

    #[test]
    fn color_sequence_json_shape() {
        let v: serde_json::Value = serde_json::from_str(&color_sequence_json(3)).unwrap();
        let names: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Red", "Green", "Blue"]);
        assert!(max_sequence_colors() >= 3);
    }

    #[test]
    fn colors_roundtrip() {
        for hex in ["#ff0000", "#123456", "#808080", "#ffffff", "#000000"] {
            let c = hex_to_linear(hex);
            assert_eq!(linear_to_hex(c[0], c[1], c[2]), hex);
        }
    }

    #[test]
    fn movement_matches_engine_shape() {
        let v = MovementView::from_json(
            r#"{"group":1,"shape":{"type":"circle"},"center_pan":10,"center_tilt":-5,"width":60,"height":20,"period":{"ms":1000},"phase_spread":180,"direction":"Forward"}"#,
        )
        .unwrap();
        let heads = v.head_points(0.0, 2);
        assert_eq!(&heads[..2], &[40.0, -5.0]);
        assert!((heads[2] - 10.0).abs() < 1e-4 && (heads[3] - 5.0).abs() < 1e-4);
        assert_eq!(v.path_points(4).len(), 10);
    }

    #[test]
    fn matrix_chase() {
        let m = MatrixView::from_json(
            r#"{"group":1,"algorithm":{"type":"chase","color":{"r":1,"g":0,"b":0},"background":{"r":0,"g":0,"b":0}},"period":{"ms":400},"direction":"Forward"}"#,
            r#"{"width":4,"height":1,"positions":[{"x":0,"y":0},{"x":1,"y":0},{"x":2,"y":0},{"x":3,"y":0}]}"#,
        )
        .unwrap();
        let reds: Vec<f32> = m.cell_colors(0.25).chunks(3).map(|c| c[0]).collect();
        assert_eq!(reds, vec![0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn scene_channel_edit_json_shape() {
        let channel = r#"{"type":"dmx","attribute":"Pan","index":0,"fine":false}"#;
        let set = set_scene_channel_json("[]", 3, channel, Some(7)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&set).unwrap();
        assert_eq!(v[0]["target"]["id"], 3);
        assert_eq!(v[0]["value"]["value"], 7);
        assert_eq!(
            set_scene_channel_json(&set, 3, channel, None).unwrap(),
            "[]"
        );
        assert!(set_scene_channel_json("{", 3, channel, None).is_err());
    }

    #[test]
    fn bad_json_is_an_error_not_a_panic() {
        assert!(ShowView::from_json("{").is_err());
        assert!(MovementView::from_json("[]").is_err());
        assert!(MatrixView::from_json("{}", "{}").is_err());
    }
}
