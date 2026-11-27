//! Matrix effects: axis, fill and twinkle, run modes and fades of the level.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::effects::{matrix_effect_color, matrix_phase};
use klight_engine_core::Engine;
use klight_model::function::{Direction, RunMode};
use klight_model::*;

const FX: FunctionId = FunctionId(40);
const MS: u64 = 1000;
const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
};

fn effect(algorithm: MatrixAlgorithm) -> MatrixEffect {
    MatrixEffect {
        group: G_PIXELS,
        algorithm,
        period: TimeValue::Ms(1000),
        direction: Direction::Forward,
        axis: MatrixAxis::Horizontal,
        controls: MatrixControls::Color,
        run_mode: RunMode::Loop,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    }
}

fn chase() -> MatrixAlgorithm {
    MatrixAlgorithm::Chase {
        color: RED,
        background: Color::BLACK,
    }
}

/// Which cells of a `w` x `h` grid are red at `phase`, row by row.
fn lit(e: &MatrixEffect, w: u16, h: u16, phase: f32) -> Vec<Vec<bool>> {
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| matrix_effect_color(e, w, h, x, y, phase).r > 0.5)
                .collect()
        })
        .collect()
}

#[test]
fn a_vertical_chase_moves_across_the_rows() {
    let mut e = effect(chase());
    assert_eq!(lit(&e, 2, 2, 0.6), [[false, true], [false, true]]);
    e.axis = MatrixAxis::Vertical;
    assert_eq!(lit(&e, 2, 2, 0.6), [[false, false], [true, true]]);
}

#[test]
fn text_ignores_the_axis() {
    let mut e = effect(MatrixAlgorithm::Text {
        text: "A".into(),
        color: RED,
    });
    let across = lit(&e, 4, 5, 0.3);
    e.axis = MatrixAxis::Vertical;
    assert_eq!(lit(&e, 4, 5, 0.3), across);
}

#[test]
fn fill_lights_one_more_column_at_a_time() {
    let e = effect(MatrixAlgorithm::Fill {
        color: RED,
        background: Color::BLACK,
    });
    let count = |phase| lit(&e, 4, 1, phase)[0].iter().filter(|&&l| l).count();
    assert_eq!(count(0.0), 0);
    assert_eq!(count(0.25), 1);
    assert_eq!(count(0.5), 2);
    assert_eq!(count(0.99), 4);
    assert_eq!(lit(&e, 4, 1, 0.5)[0], [true, true, false, false]);
}

#[test]
fn twinkle_lights_its_share_of_cells_and_changes_with_each_draw() {
    let twinkle = |density| {
        effect(MatrixAlgorithm::Twinkle {
            color: RED,
            background: Color::BLACK,
            density,
        })
    };
    let share = |e: &MatrixEffect, phase| {
        let cells = lit(e, 20, 20, phase);
        cells.iter().flatten().filter(|&&l| l).count() as f32 / 400.0
    };
    let e = twinkle(0.3);
    assert!((share(&e, 0.0) - 0.3).abs() < 0.08, "{}", share(&e, 0.0));
    assert_eq!(share(&twinkle(0.0), 0.5), 0.0);
    assert_eq!(share(&twinkle(1.0), 0.5), 1.0);
    // Within a draw the same cells; the next draw (1/8 cycle on) others.
    assert_eq!(lit(&e, 20, 20, 0.01), lit(&e, 20, 20, 0.12));
    assert_ne!(lit(&e, 20, 20, 0.01), lit(&e, 20, 20, 0.13));
    // And the same every time.
    assert_eq!(lit(&e, 20, 20, 0.5), lit(&e, 20, 20, 0.5));
}

#[test]
fn run_modes_of_the_phase() {
    let p = |turns, mode| matrix_phase(turns, Direction::Forward, mode);
    assert_eq!(p(1.25, RunMode::Loop), 0.25);
    // A single shot stays on its last frame.
    assert!(p(3.0, RunMode::SingleShot) > 0.999);
    assert_eq!(p(0.25, RunMode::SingleShot), 0.25);
    // Ping-pong comes back.
    assert_eq!(p(1.75, RunMode::PingPong), 0.25);
    assert!(
        p(1.0, RunMode::PingPong) > 0.999,
        "turns at the end, not the start"
    );
    // Backwards, a single shot ends near the start.
    assert!(matrix_phase(3.0, Direction::Backward, RunMode::SingleShot) < 0.001);
}

fn started(m: MatrixEffect) -> Engine {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FX,
                folder: None,
                name: "matrix".into(),
                kind: FunctionKind::MatrixEffect(m),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e
}

/// Red of the first head of the pixel bar (its virtual dimmer applied).
fn red(e: &Engine) -> u8 {
    dmx(e, 100)
}

fn solid() -> MatrixAlgorithm {
    MatrixAlgorithm::Solid { color: RED }
}

#[test]
fn the_level_rises_with_the_fade_in_and_falls_with_the_fade_out() {
    let mut m = effect(solid());
    m.fade_in = TimeValue::Ms(1000);
    m.fade_out = TimeValue::Ms(1000);
    let mut e = started(m);
    e.tick(0);
    assert_eq!(red(&e), 0);
    e.tick(500 * MS);
    assert!((126..=129).contains(&red(&e)), "{}", red(&e));
    e.tick(1500 * MS);
    assert_eq!(red(&e), 255);
    ok(&mut e, 2000 * MS, Command::StopFunction { id: FX });
    e.tick(2250 * MS);
    assert!((189..=192).contains(&red(&e)), "{}", red(&e));
    assert!(
        e.snapshot().state.active.is_empty(),
        "stopping: not listed as playing"
    );
    e.tick(3000 * MS);
    e.tick(3001 * MS);
    assert_eq!(red(&e), 0);
}

#[test]
fn a_matrix_without_fade_out_stops_at_once() {
    let mut e = started(effect(solid()));
    e.tick(0);
    assert_eq!(red(&e), 255);
    ok(&mut e, 10, Command::StopFunction { id: FX });
    e.tick(20);
    assert_eq!(red(&e), 0);
}

#[test]
fn a_single_shot_matrix_plays_one_cycle_then_stops() {
    let mut m = effect(chase());
    m.run_mode = RunMode::SingleShot;
    let mut e = started(m);
    e.tick(100 * MS);
    assert_eq!(red(&e), 255, "the chase on the first column");
    e.tick(999 * MS);
    assert_eq!(e.snapshot().state.active.len(), 1);
    e.tick(1000 * MS);
    e.tick(1001 * MS);
    assert!(e.snapshot().state.active.is_empty());
}

// ---- driving the dimmer only

/// A scene of `values`, started.
fn with_scene(e: &mut Engine, values: Vec<SceneValue>) {
    ok(
        e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(41),
                folder: None,
                name: "colors".into(),
                kind: scene(41, values, 0).kind,
            },
        },
    );
    ok(e, 0, Command::StartFunction { id: FunctionId(41) });
}

fn color(r: f32, g: f32, b: f32) -> AttributeValue {
    AttributeValue::Color {
        color: Color::new(r, g, b),
    }
}

/// Red, green and blue of head `h` of the pixel bar.
fn head_rgb(e: &Engine, h: usize) -> (u8, u8, u8) {
    let a = 100 + 3 * h;
    (dmx(e, a), dmx(e, a + 1), dmx(e, a + 2))
}

fn dimmer_only(algorithm: MatrixAlgorithm) -> MatrixEffect {
    MatrixEffect {
        controls: MatrixControls::Dimmer,
        ..effect(algorithm)
    }
}

#[test]
fn a_matrix_on_the_dimmer_keeps_the_colors_of_the_scenes() {
    // Blue from a scene, a red chase on the dimmer: the lit head is blue.
    let mut e = started(dimmer_only(chase()));
    with_scene(
        &mut e,
        vec![explicit(group(G_PIXELS), color(0.0, 0.0, 1.0))],
    );
    e.tick(100 * MS);
    assert_eq!(
        head_rgb(&e, 0),
        (0, 0, 255),
        "lit by the chase, in the scene's color"
    );
    assert_eq!(
        head_rgb(&e, 1),
        (0, 0, 0),
        "dark: its intensity is the chase's"
    );
    e.tick(300 * MS);
    assert_eq!(head_rgb(&e, 0), (0, 0, 0));
    assert_eq!(head_rgb(&e, 1), (0, 0, 255));
}

#[test]
fn the_level_of_a_dimmer_matrix_is_the_brightness_of_its_pattern() {
    // A solid gray at half: the scene's orange at half, its hue unchanged.
    let mut e = started(dimmer_only(MatrixAlgorithm::Solid {
        color: Color::new(0.5, 0.2, 0.0),
    }));
    with_scene(
        &mut e,
        vec![explicit(group(G_PIXELS), color(1.0, 0.5, 0.0))],
    );
    e.tick(0);
    let (r, g, b) = head_rgb(&e, 0);
    assert!((127..=129).contains(&r), "{r}");
    assert!((63..=65).contains(&g), "{g}");
    assert_eq!(b, 0);
}

#[test]
fn a_scene_that_also_sets_the_intensity_raises_the_floor() {
    // Intensity merges HTP: the dark head of the chase keeps the scene's 40%.
    let mut e = started(dimmer_only(chase()));
    with_scene(
        &mut e,
        vec![
            explicit(group(G_PIXELS), color(0.0, 1.0, 0.0)),
            explicit(group(G_PIXELS), intensity(0.4)),
        ],
    );
    e.tick(100 * MS);
    assert_eq!(head_rgb(&e, 0).1, 255);
    assert!(
        (101..=103).contains(&head_rgb(&e, 1).1),
        "{:?}",
        head_rgb(&e, 1)
    );
}

#[test]
fn a_dimmer_matrix_leaves_the_color_channels_alone() {
    // On its own it sets no color: nothing shows until a scene gives one.
    let mut e = started(dimmer_only(MatrixAlgorithm::Solid { color: RED }));
    e.tick(0);
    let mut idle = engine(test_show());
    idle.tick(0);
    assert_eq!(head_rgb(&e, 0), head_rgb(&idle, 0));
}

#[test]
fn the_preview_of_a_dimmer_matrix_is_gray() {
    use klight_engine_core::effects::{color_level, matrix_cell_look};
    let e = dimmer_only(MatrixAlgorithm::Solid {
        color: Color::new(0.2, 0.8, 0.4),
    });
    let look = matrix_cell_look(&e, 1, 1, 0, 0, 0.0);
    assert_eq!((look.r, look.g, look.b), (0.8, 0.8, 0.8));
    assert_eq!(color_level(Color::new(0.2, 0.8, 0.4)), 0.8);
    // On the color, the color itself.
    let e = effect(MatrixAlgorithm::Solid {
        color: Color::new(0.2, 0.8, 0.4),
    });
    assert_eq!(matrix_cell_look(&e, 1, 1, 0, 0, 0.0).r, 0.2);
}
