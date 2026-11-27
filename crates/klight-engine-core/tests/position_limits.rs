//! The position limits of a fixture hold the lights inside them, whatever
//! plays and from whatever layer: scenes, effects, the programmer and the
//! raw channels of the Simple Desk.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::Engine;
use klight_fixtures::convert::{degrees_to_dmx16, limits_to_dmx};
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;
use proptest::prelude::*;

const FX: FunctionId = FunctionId(80);
const MS: u64 = 1000;
/// Spot 1: pan at 40 (fine 41), tilt at 42 (fine 43), 540° and 270° of range.
const PAN1: usize = 40;
const TILT1: usize = 42;
/// Spot 2 has its pan inverted.
const PAN2: usize = 60;
const TILT2: usize = 62;

const LIMITS: PositionLimits = PositionLimits {
    pan_min: -90.0,
    pan_max: 60.0,
    tilt_min: -20.0,
    tilt_max: 30.0,
};

fn show(limits: Option<PositionLimits>) -> Show {
    let mut s = test_show();
    for f in &mut s.fixtures {
        if f.id == SPOT1 || f.id == SPOT2 {
            f.position_limits = limits;
        }
    }
    s
}

fn word(e: &Engine, address: usize) -> u16 {
    u16::from(dmx(e, address)) << 8 | u16::from(dmx(e, address + 1))
}

fn near(a: u16, b: u16) -> bool {
    a.abs_diff(b) <= 1
}

fn position(pan: f32, tilt: f32) -> AttributeValue {
    AttributeValue::Position { pan, tilt }
}

fn with_scene(limits: Option<PositionLimits>, pan: f32, tilt: f32) -> Engine {
    let mut e = engine(show(limits));
    let f = scene(FX.0, vec![explicit(group(G_SPOTS), position(pan, tilt))], 0);
    ok(&mut e, 0, Command::SetFunction { function: f });
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e.tick(0);
    e
}

#[test]
fn a_scene_cannot_point_a_light_beyond_its_limits() {
    let e = with_scene(Some(LIMITS), 170.0, 80.0);
    assert!(near(word(&e, PAN1), degrees_to_dmx16(60.0, 540.0)));
    assert!(near(word(&e, TILT1), degrees_to_dmx16(30.0, 270.0)));
    let e = with_scene(Some(LIMITS), -170.0, -80.0);
    assert!(near(word(&e, PAN1), degrees_to_dmx16(-90.0, 540.0)));
    assert!(near(word(&e, TILT1), degrees_to_dmx16(-20.0, 270.0)));
    // Inside the limits, as asked.
    let e = with_scene(Some(LIMITS), 10.0, 5.0);
    assert!(near(word(&e, PAN1), degrees_to_dmx16(10.0, 540.0)));
    assert!(near(word(&e, TILT1), degrees_to_dmx16(5.0, 270.0)));
}

#[test]
fn without_limits_nothing_changes() {
    let e = with_scene(None, 170.0, 80.0);
    assert!(near(word(&e, PAN1), degrees_to_dmx16(170.0, 540.0)));
    assert!(near(word(&e, TILT1), degrees_to_dmx16(80.0, 270.0)));
}

#[test]
fn the_limits_are_in_the_frame_of_the_show_for_a_light_with_the_pan_inverted() {
    // Spot 2 has its pan inverted: the show's +60° is -60° on the channel.
    let e = with_scene(Some(LIMITS), 170.0, 0.0);
    assert!(near(word(&e, PAN2), degrees_to_dmx16(-60.0, 540.0)));
    let e = with_scene(Some(LIMITS), -170.0, 0.0);
    assert!(near(word(&e, PAN2), degrees_to_dmx16(90.0, 540.0)));
    assert!(near(word(&e, TILT2), degrees_to_dmx16(0.0, 270.0)));
}

#[test]
fn a_movement_stays_inside_the_limits_through_the_whole_cycle() {
    let mut e = engine(show(Some(LIMITS)));
    let m = MovementEffect {
        group: G_SPOTS,
        shape: MovementShape::Circle,
        center_pan: 0.0,
        center_tilt: 0.0,
        width: 300.0,
        height: 160.0,
        period: TimeValue::Ms(1000),
        phase_spread: 0.0,
        direction: Direction::Forward,
        rotation: 0.0,
        mirror: MovementMirror::None,
        run_mode: RunMode::Loop,
        relative: false,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    };
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FX,
                folder: None,
                name: "wide".into(),
                kind: FunctionKind::MovementEffect(m),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FX });
    let (lo_pan, hi_pan) = (
        degrees_to_dmx16(-90.0, 540.0),
        degrees_to_dmx16(60.0, 540.0),
    );
    let (lo_tilt, hi_tilt) = (
        degrees_to_dmx16(-20.0, 270.0),
        degrees_to_dmx16(30.0, 270.0),
    );
    let (mut reached_pan, mut reached_tilt) = (false, false);
    for step in 0..40 {
        e.tick(step * 25 * MS);
        let (p, t) = (word(&e, PAN1), word(&e, TILT1));
        assert!(
            (lo_pan.saturating_sub(1)..=hi_pan.saturating_add(1)).contains(&p),
            "pan {p} at {step}"
        );
        assert!(
            (lo_tilt.saturating_sub(1)..=hi_tilt.saturating_add(1)).contains(&t),
            "tilt {t} at {step}"
        );
        reached_pan |= near(p, hi_pan) || near(p, lo_pan);
        reached_tilt |= near(t, hi_tilt) || near(t, lo_tilt);
    }
    assert!(
        reached_pan && reached_tilt,
        "the wide circle goes to the limits"
    );
}

#[test]
fn the_programmer_is_held_inside_the_limits_too() {
    let mut e = engine(show(Some(LIMITS)));
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: fixture(SPOT1),
            value: position(0.0, 100.0),
        },
    );
    e.tick(0);
    assert!(near(word(&e, TILT1), degrees_to_dmx16(30.0, 270.0)));
}

#[test]
fn the_raw_channels_of_the_simple_desk_are_held_inside_the_limits_too() {
    let mut e = engine(show(Some(LIMITS)));
    // The tilt channel at full (the highest the desk can ask).
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: u16::try_from(TILT1).unwrap(),
            value: Some(255),
        },
    );
    e.tick(0);
    assert!(near(word(&e, TILT1), degrees_to_dmx16(30.0, 270.0)));
    // And the lights of another fixture keep what the desk gives them.
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 300,
            value: Some(255),
        },
    );
    e.tick(0);
    assert_eq!(dmx(&e, 300), 255);
}

#[test]
fn limits_that_make_no_sense_are_refused() {
    let mut e = engine(show(None));
    let mut f = e.show().fixture(SPOT1).unwrap().clone();
    for bad in [
        PositionLimits {
            pan_min: 10.0,
            pan_max: -10.0,
            ..LIMITS
        },
        PositionLimits {
            tilt_max: f32::NAN,
            ..LIMITS
        },
    ] {
        f.position_limits = Some(bad);
        let r = e.apply(
            0,
            ui(
                1,
                Command::PatchFixture {
                    fixture: f.clone(),
                    force: false,
                },
            ),
        );
        assert!(r.result.is_err(), "{bad:?}");
    }
    f.position_limits = Some(LIMITS);
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: f,
            force: false,
        },
    );
    assert_eq!(
        e.show().fixture(SPOT1).unwrap().position_limits,
        Some(LIMITS)
    );
}

#[test]
fn a_head_with_one_byte_of_pan_and_tilt_is_held_inside_too() {
    // The basic head has no fine channels and, as the file is, no range:
    // give it one of 360 and 180 degrees.
    let text = include_str!("../../../fixtures/Test-Basic-Head.qxf").replace(
        "<Mode Name=\"Basic\">",
        "<Mode Name=\"Basic\"><Physical><Focus PanMax=\"360\" TiltMax=\"180\"/></Physical>",
    );
    let basic = def(&text);
    let mut s = test_show();
    s.fixtures.push(PatchedFixture {
        id: FixtureId(90),
        name: "Basic".into(),
        definition: basic.key(),
        mode: "Basic".into(),
        universe: U1,
        address: 300,
        orientation: Orientation::default(),
        calibration: None,
        position_limits: Some(PositionLimits {
            pan_min: -45.0,
            pan_max: 45.0,
            tilt_min: -10.0,
            tilt_max: 10.0,
        }),
    });
    s.definitions.push(basic);
    let mut e = engine(s);
    let f = scene(
        FX.0,
        vec![explicit(fixture(FixtureId(90)), position(170.0, -80.0))],
        0,
    );
    ok(&mut e, 0, Command::SetFunction { function: f });
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e.tick(0);
    // 45° of 360 from the center: 0.625 of the way, 159.4 of 255, so no
    // further than 159. -10° of 180: 0.444, 113.3, no lower than 114.
    let (pan, tilt) = (dmx(&e, 300), dmx(&e, 301));
    assert!((158..=159).contains(&pan), "{pan}");
    assert!((113..=115).contains(&tilt), "{tilt}");
}

proptest! {
    /// Whatever is asked, from the scene or the desk, a limited light ends
    /// inside the bounds the limits give its channels.
    #[test]
    fn the_output_is_always_inside_the_bounds(
        (pan, tilt) in (-400.0f32..400.0, -400.0f32..400.0),
        (raw_pan, raw_tilt) in (any::<u8>(), any::<u8>()),
        use_desk in any::<bool>(),
    ) {
        let mut e = with_scene(Some(LIMITS), pan, tilt);
        if use_desk {
            for (addr, v) in [(PAN1, raw_pan), (TILT1, raw_tilt)] {
                ok(&mut e, 0, Command::SetChannel { universe: U1, address: u16::try_from(addr).unwrap(), value: Some(v) });
            }
            e.tick(0);
        }
        let spot = e.show().fixture(SPOT1).unwrap().orientation;
        let spot2 = e.show().fixture(SPOT2).unwrap().orientation;
        for (o, pan_addr, tilt_addr) in [(spot, PAN1, TILT1), (spot2, PAN2, TILT2)] {
            let ((plo, phi), (tlo, thi)) = limits_to_dmx(o, &LIMITS, 540.0, 270.0);
            let (p, t) = (word(&e, pan_addr), word(&e, tilt_addr));
            // The desk gives a high byte; its low byte can add up to 255.
            prop_assert!(p >= plo.saturating_sub(1) && p <= phi.saturating_add(1), "pan {p} not in {plo}..{phi}");
            prop_assert!(t >= tlo.saturating_sub(1) && t <= thi.saturating_add(1), "tilt {t} not in {tlo}..{thi}");
        }
    }
}
