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
use klight_engine_core::preview::scene_preview;
use klight_model::inspect::{self, FixturePreview};
use klight_model::{
    Color, FixtureId, FunctionId, GroupId, GroupLayout, MatrixEffect, MovementEffect, Rgb8, Show,
    UniverseId,
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

    pub fn scene_preview_of(&self, id: u32) -> Option<Vec<FixturePreview>> {
        scene_preview(&self.show, self.compiled(), FunctionId(id))
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

    /// JSON `FixturePreview[]`: what a scene sets on each fixture and
    /// channel; undefined if the function is not a scene.
    pub fn scene_preview(&self, id: u32) -> Option<String> {
        self.scene_preview_of(id).map(|p| to_json(&p))
    }

    /// JSON `GroupLayout` (the grid matrix effects use), or undefined.
    pub fn group_layout(&self, group: u32) -> Option<String> {
        self.group_layout_of(group).map(|l| to_json(&l))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn bad_json_is_an_error_not_a_panic() {
        assert!(ShowView::from_json("{").is_err());
        assert!(MovementView::from_json("[]").is_err());
        assert!(MatrixView::from_json("{}", "{}").is_err());
    }
}
