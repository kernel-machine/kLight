//! A matrix in color mode gives each light the color a scene would: white and
//! amber channels, cyan/magenta/yellow, the color wheel, the calibration.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::Engine;
use klight_model::function::{Direction, RunMode};
use klight_model::*;

const CMY: &str = include_str!("../../../fixtures/Test-CMY-Par.qxf");
const WHEEL: &str = include_str!("../../../fixtures/Test-Color-Wheel.qxf");
const MATRIX: GroupId = GroupId(9);
const FX: FunctionId = FunctionId(70);
const FIRST: u16 = 200;
/// RGB, RGBW, RGBW with calibration, CMY, color wheel.
const CELLS: u16 = 5;

fn patch(id: u32, name: &str, d: &FixtureDefinition, mode: &str, address: u16) -> PatchedFixture {
    PatchedFixture {
        id: FixtureId(id),
        name: name.into(),
        definition: d.key(),
        mode: mode.into(),
        universe: U1,
        address,
        orientation: Orientation::default(),
        calibration: None,
        position_limits: None,
    }
}

fn show() -> Show {
    let mut s = test_show();
    let (cmy, wheel) = (def(CMY), def(WHEEL));
    let rgb_key = s.fixtures.iter().find(|f| f.id == RGB).unwrap().clone();
    let rgbw_key = s.fixtures.iter().find(|f| f.id == RGBW).unwrap().clone();
    let mut fixtures = vec![
        PatchedFixture {
            id: FixtureId(20),
            address: FIRST,
            ..rgb_key
        },
        PatchedFixture {
            id: FixtureId(21),
            address: FIRST + 10,
            ..rgbw_key.clone()
        },
        PatchedFixture {
            id: FixtureId(22),
            address: FIRST + 20,
            // Takes a part of green away.
            calibration: Some(patch::ColorCalibration {
                matrix: [[1.0, 0.0, 0.0], [0.1, 0.8, 0.0], [0.0, 0.0, 0.9]],
            }),
            ..rgbw_key
        },
        patch(23, "CMY", &cmy, "4 Channel", FIRST + 30),
        patch(24, "Wheel", &wheel, "2 Channel", FIRST + 40),
    ];
    s.fixtures.append(&mut fixtures);
    s.definitions.push(cmy);
    s.definitions.push(wheel);
    s.groups.push(FixtureGroup {
        id: MATRIX,
        name: "Mixed".into(),
        members: (0..CELLS)
            .map(|i| GroupMember {
                fixture: FixtureId(20 + u32::from(i)),
                head: 0,
                grid: Some(patch::GridPos { x: i, y: 0 }),
            })
            .collect(),
        grid_width: CELLS,
        grid_height: 1,
    });
    s
}

fn matrix(color: Color) -> Function {
    Function {
        id: FX,
        folder: None,
        name: "matrix".into(),
        kind: FunctionKind::MatrixEffect(MatrixEffect {
            group: MATRIX,
            algorithm: MatrixAlgorithm::Solid { color },
            period: TimeValue::Ms(1000),
            direction: Direction::Forward,
            axis: MatrixAxis::Horizontal,
            controls: MatrixControls::Color,
            run_mode: RunMode::Loop,
            fade_in: TimeValue::ZERO,
            fade_out: TimeValue::ZERO,
        }),
    }
}

/// The lights of the group as a matrix drives them.
fn by_matrix(color: Color) -> Vec<u8> {
    let mut e = engine(show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: matrix(color),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e.tick(0);
    frame(&e)
}

/// The same lights as a scene sets them: the oracle.
fn by_scene(color: Color) -> Vec<u8> {
    let mut e = engine(show());
    let f = scene(
        FX.0,
        vec![
            explicit(group(MATRIX), AttributeValue::Color { color }),
            explicit(group(MATRIX), intensity(1.0)),
        ],
        0,
    );
    ok(&mut e, 0, Command::SetFunction { function: f });
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e.tick(0);
    frame(&e)
}

fn frame(e: &Engine) -> Vec<u8> {
    let f = e.frame(U1).unwrap();
    f[usize::from(FIRST) - 1..usize::from(FIRST) - 1 + 50].to_vec()
}

fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b }
}

#[test]
fn every_kind_of_light_gets_what_a_scene_gives_it() {
    for color in [
        rgb(1.0, 0.0, 0.0),
        rgb(0.0, 1.0, 0.0),
        rgb(0.0, 0.0, 1.0),
        rgb(1.0, 1.0, 1.0),
        rgb(0.9, 0.5, 0.2),
        rgb(0.2, 0.6, 0.8),
        rgb(0.0, 0.0, 0.0),
    ] {
        assert_eq!(by_matrix(color), by_scene(color), "{color:?}");
    }
}

#[test]
fn a_white_matrix_lights_the_white_emitter_of_an_rgbw_light() {
    let out = by_matrix(rgb(1.0, 1.0, 1.0));
    // RGBW par at +10: dimmer, R, G, B, W.
    let (r, g, b, w) = (out[11], out[12], out[13], out[14]);
    assert_eq!((r, g, b, w), (0, 0, 0, 255));
}

#[test]
fn the_cmy_light_gets_the_complement_and_the_wheel_its_nearest_color() {
    let out = by_matrix(rgb(1.0, 0.0, 0.0));
    // CMY at +30: dimmer, C, M, Y.
    assert_eq!((out[31], out[32], out[33]), (0, 255, 255));
    // Wheel at +40: dimmer, slot. Red is the slot at 10..19.
    assert!((10..=19).contains(&out[41]), "{}", out[41]);
}

#[test]
fn the_calibration_of_a_light_is_applied() {
    let out = by_matrix(rgb(1.0, 1.0, 0.0));
    // The green of the plain RGBW light (+10) is full; the calibrated one
    // (+20) mixes 0.1 of red and 0.8 of green into it: 0.9.
    assert_eq!(out[12], 255);
    assert!((229..=231).contains(&out[22]), "{}", out[22]);
}
