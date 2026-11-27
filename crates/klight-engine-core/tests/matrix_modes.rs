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
    chase_of(1)
}

fn chase_of(width: u8) -> MatrixAlgorithm {
    MatrixAlgorithm::Chase {
        color: RED,
        background: Color::BLACK,
        width,
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

#[test]
fn a_chase_lights_as_many_cells_as_its_width_and_wraps_around() {
    let row = |width, phase| lit(&effect(chase_of(width)), 8, 1, phase)[0].clone();
    let t = true;
    let f = false;
    assert_eq!(row(1, 0.0), [t, f, f, f, f, f, f, f]);
    assert_eq!(row(3, 0.0), [t, t, t, f, f, f, f, f]);
    assert_eq!(row(3, 0.25), [f, f, t, t, t, f, f, f]);
    // On the last column the band goes on from the first.
    assert_eq!(row(3, 0.875), [t, t, f, f, f, f, f, t]);
    // Wider than the grid: everything, and no more than 16 are asked for.
    assert_eq!(row(16, 0.4), [t; 8]);
    // A width of nothing is one cell.
    assert_eq!(row(0, 0.0), [t, f, f, f, f, f, f, f]);
}

#[test]
fn a_chase_without_a_width_in_an_old_show_is_one_cell() {
    let a: MatrixAlgorithm = serde_json::from_str(
        r#"{"type":"chase","color":{"r":1.0,"g":0.0,"b":0.0},"background":{"r":0.0,"g":0.0,"b":0.0}}"#,
    )
    .unwrap();
    assert!(matches!(a, MatrixAlgorithm::Chase { width: 1, .. }));
}

fn radial(algorithm: MatrixAlgorithm) -> MatrixEffect {
    let mut e = effect(algorithm);
    e.axis = MatrixAxis::Radial;
    e
}

#[test]
fn a_radial_chase_goes_from_the_center_to_the_corners() {
    let e = radial(chase());
    let lit_cells = |phase| {
        lit(&e, 5, 5, phase)
            .into_iter()
            .flatten()
            .filter(|&l| l)
            .count()
    };
    // The rings of the 5x5 grid go from the center to the corners.
    let at = |phase, x: usize, y: usize| lit(&e, 5, 5, phase)[y][x];
    assert!(at(0.0, 2, 2) && lit_cells(0.0) == 1, "the center first");
    assert!(!at(0.0, 0, 0));
    assert!(at(0.999, 0, 0) && at(0.999, 4, 4) && at(0.999, 0, 4) && at(0.999, 4, 0));
    assert!(!at(0.999, 2, 2));
    // Cells at the same distance from the center share their color.
    for phase in [0.1, 0.4, 0.7] {
        let g = lit(&e, 5, 5, phase);
        assert_eq!(g[0][2], g[4][2]);
        assert_eq!(g[2][0], g[2][4]);
        assert_eq!(g[0][2], g[2][0]);
    }
}

#[test]
fn a_radial_fill_grows_from_the_center() {
    let fill = || MatrixAlgorithm::Fill {
        color: RED,
        background: Color::BLACK,
    };
    let e = radial(fill());
    let count = |phase| {
        lit(&e, 5, 5, phase)
            .into_iter()
            .flatten()
            .filter(|&l| l)
            .count()
    };
    assert_eq!(count(0.0), 0);
    assert!(count(0.3) > 0 && count(0.3) < count(0.6));
    assert_eq!(count(0.99), 25);
    assert!(lit(&e, 5, 5, 0.3)[2][2], "the center is the first to fill");
}

#[test]
fn the_other_axes_are_what_they_were() {
    // Horizontal and vertical give what the cells gave before the radial axis.
    let mut e = effect(chase());
    assert_eq!(lit(&e, 4, 2, 0.6)[0], [false, false, true, false]);
    e.axis = MatrixAxis::Vertical;
    assert_eq!(lit(&e, 4, 2, 0.6), [[false; 4], [true; 4]]);
}

fn rainbow(spread: f32) -> MatrixEffect {
    effect(MatrixAlgorithm::Rainbow { spread })
}

fn color_at(e: &MatrixEffect, w: u16, x: u16, phase: f32) -> Color {
    matrix_effect_color(e, w, 1, x, 0, phase)
}

#[test]
fn a_rainbow_starts_red_and_goes_through_the_hues() {
    let e = rainbow(1.0);
    let first = color_at(&e, 6, 0, 0.0);
    assert!(
        first.r > 0.99 && first.g < 0.01 && first.b < 0.01,
        "{first:?}"
    );
    // Two cells of six: a third of the way round is green, two thirds blue.
    let green = color_at(&e, 6, 2, 0.0);
    assert!(
        green.g > 0.99 && green.r < 0.01 && green.b < 0.01,
        "{green:?}"
    );
    let blue = color_at(&e, 6, 4, 0.0);
    assert!(blue.b > 0.99 && blue.r < 0.01 && blue.g < 0.01, "{blue:?}");
}

#[test]
fn a_rainbow_scrolls_and_comes_back_after_a_cycle() {
    let e = rainbow(1.0);
    let at = |phase| color_at(&e, 6, 0, phase);
    assert_ne!(at(0.0), at(0.3));
    let (a, b) = (at(0.0), at(0.9999));
    assert!((a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01);
    for x in 0..6 {
        for phase in [0.0, 0.17, 0.5, 0.83] {
            let c = color_at(&e, 6, x, phase);
            assert!([c.r, c.g, c.b].iter().all(|v| (0.0..=1.0).contains(v)));
            assert!(c.r.max(c.g).max(c.b) > 0.99, "full brightness: {c:?}");
        }
    }
}

#[test]
fn the_spread_is_how_many_rainbows_fit_the_grid_and_is_kept_in_range() {
    // Two rainbows over four cells: the third cell is where the first started.
    let two = rainbow(2.0);
    let (a, c) = (color_at(&two, 4, 0, 0.0), color_at(&two, 4, 2, 0.0));
    assert!((a.r - c.r).abs() < 0.01 && (a.g - c.g).abs() < 0.01 && (a.b - c.b).abs() < 0.01);
    // Out of range values are held to 0.25..=4.
    assert_eq!(
        color_at(&rainbow(100.0), 8, 3, 0.2),
        color_at(&rainbow(4.0), 8, 3, 0.2)
    );
    assert_eq!(
        color_at(&rainbow(0.0), 8, 3, 0.2),
        color_at(&rainbow(0.25), 8, 3, 0.2)
    );
}

#[test]
fn a_rainbow_moves_along_the_axis_and_drives_the_dimmer_at_full() {
    let mut e = rainbow(1.0);
    e.axis = MatrixAxis::Vertical;
    let top = matrix_effect_color(&e, 1, 6, 0, 0, 0.0);
    let third = matrix_effect_color(&e, 1, 6, 0, 2, 0.0);
    assert!(top.r > 0.99 && third.g > 0.99);
    // On the dimmer a rainbow has no brightness to vary: full.
    assert_eq!(klight_engine_core::effects::color_level(third), 1.0);
}

#[test]
fn a_rainbow_without_a_spread_in_a_file_is_one_rainbow() {
    let a: MatrixAlgorithm = serde_json::from_str(r#"{"type":"rainbow"}"#).unwrap();
    assert!(matches!(a, MatrixAlgorithm::Rainbow { spread } if spread == 1.0));
}
