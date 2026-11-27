//! Movement effects: rotation, mirrored heads, run modes, relative mode and
//! the fades of their size.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::Engine;
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;

const SPOT1_ONLY: GroupId = GroupId(9);
const FX: FunctionId = FunctionId(30);
const MS: u64 = 1000;

fn show() -> Show {
    let mut s = test_show();
    s.groups.push(FixtureGroup {
        id: SPOT1_ONLY,
        name: "Spot 1".into(),
        members: vec![GroupMember {
            fixture: SPOT1,
            head: 0,
            grid: None,
        }],
        grid_width: 0,
        grid_height: 0,
    });
    s
}

/// A circle of 60° x 20° around the center, one cycle a second.
fn circle(group: GroupId) -> MovementEffect {
    MovementEffect {
        group,
        shape: MovementShape::Circle,
        center_pan: 0.0,
        center_tilt: 0.0,
        width: 60.0,
        height: 20.0,
        period: TimeValue::Ms(1000),
        phase_spread: 0.0,
        direction: Direction::Forward,
        rotation: 0.0,
        mirror: MovementMirror::None,
        run_mode: RunMode::Loop,
        relative: false,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    }
}

fn set(e: &mut Engine, now: u64, id: FunctionId, kind: FunctionKind) {
    ok(
        e,
        now,
        Command::SetFunction {
            function: Function {
                id,
                folder: None,
                name: format!("f{}", id.0),
                kind,
            },
        },
    );
}

fn started(m: MovementEffect) -> Engine {
    let mut e = engine(show());
    set(&mut e, 0, FX, FunctionKind::MovementEffect(m));
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e
}

/// 16-bit pan and tilt of Spot 1 (pan at 40, tilt at 42, each with its fine).
fn spot1(e: &Engine) -> (u16, u16) {
    let at = |a| u16::from(dmx(e, a)) << 8 | u16::from(dmx(e, a + 1));
    (at(40), at(42))
}

/// What Spot 1 shows at `pan`, `tilt` degrees (540° x 270°).
fn degrees(pan: f32, tilt: f32) -> (u16, u16) {
    use klight_fixtures::convert::degrees_to_dmx16;
    (degrees_to_dmx16(pan, 540.0), degrees_to_dmx16(tilt, 270.0))
}

fn near(a: (u16, u16), b: (u16, u16)) -> bool {
    a.0.abs_diff(b.0) <= 2 && a.1.abs_diff(b.1) <= 2
}

#[test]
fn rotation_turns_the_shape_around_its_center() {
    let mut m = circle(SPOT1_ONLY);
    m.rotation = 90.0;
    let mut e = started(m);
    // The start of the circle, 30° right, turned a quarter: 30° up.
    e.tick(0);
    assert!(near(spot1(&e), degrees(0.0, 30.0)), "{:?}", spot1(&e));
    // A quarter on, 10° up, turned: 10° left.
    e.tick(250 * MS);
    assert!(near(spot1(&e), degrees(-10.0, 0.0)), "{:?}", spot1(&e));
}

#[test]
fn mirrored_heads_draw_the_shape_left_to_right() {
    // Spot 2 is patched with its pan inverted: without mirror the two spots
    // point to opposite sides, mirrored they point to the same one.
    let pans = |mirror| {
        let mut m = circle(G_SPOTS);
        m.mirror = mirror;
        let mut e = started(m);
        e.tick(0);
        (dmx(&e, 40), dmx(&e, 60))
    };
    let (a, b) = pans(MovementMirror::None);
    assert_ne!(a, b);
    let (a, b) = pans(MovementMirror::Alternate);
    assert_eq!(a, b);
    let (a, b) = pans(MovementMirror::Halves);
    assert_eq!(a, b, "the second of two heads is the second half");
}

#[test]
fn which_heads_are_mirrored() {
    let who = |mirror: MovementMirror, n| (0..n).map(|i| mirror.mirrors(i, n)).collect::<Vec<_>>();
    assert_eq!(who(MovementMirror::None, 3), [false, false, false]);
    assert_eq!(
        who(MovementMirror::Alternate, 4),
        [false, true, false, true]
    );
    assert_eq!(who(MovementMirror::Halves, 4), [false, false, true, true]);
    assert_eq!(
        who(MovementMirror::Halves, 5),
        [false, false, false, true, true]
    );
}

#[test]
fn ping_pong_goes_back_along_the_shape() {
    let mut m = circle(SPOT1_ONLY);
    m.run_mode = RunMode::PingPong;
    let mut e = started(m);
    e.tick(250 * MS);
    let forward = spot1(&e);
    // In the second cycle it comes back: at 1.75 it is where it was at 0.25.
    e.tick(1750 * MS);
    assert!(near(spot1(&e), forward));
    assert!(near(forward, degrees(0.0, 10.0)));
}

#[test]
fn a_single_shot_plays_one_cycle_then_stops() {
    let mut m = circle(SPOT1_ONLY);
    m.run_mode = RunMode::SingleShot;
    let mut e = started(m);
    e.tick(500 * MS);
    assert_eq!(e.snapshot().state.active.len(), 1);
    assert!(near(spot1(&e), degrees(-30.0, 0.0)));
    e.tick(1000 * MS);
    e.tick(1010 * MS);
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn a_single_shot_with_a_fade_out_ends_at_its_center() {
    let mut m = circle(SPOT1_ONLY);
    m.run_mode = RunMode::SingleShot;
    m.center_pan = 50.0;
    m.fade_out = TimeValue::Ms(400);
    let mut e = started(m);
    e.tick(1000 * MS);
    // Stopped at the end of its cycle (the start of the circle), it shrinks.
    e.tick(1200 * MS);
    assert!(near(spot1(&e), degrees(65.0, 0.0)), "{:?}", spot1(&e));
    e.tick(1300 * MS);
    assert!(near(spot1(&e), degrees(57.5, 0.0)), "{:?}", spot1(&e));
    e.tick(1400 * MS);
    e.tick(1401 * MS);
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn the_size_grows_with_the_fade_in_and_shrinks_with_the_fade_out() {
    let mut m = circle(SPOT1_ONLY);
    m.center_pan = 20.0;
    m.fade_in = TimeValue::Ms(1000);
    m.fade_out = TimeValue::Ms(1000);
    let mut e = started(m);
    // At the start, nothing: the center.
    e.tick(0);
    assert!(near(spot1(&e), degrees(20.0, 0.0)));
    // Two cycles in (the start of the circle), fully grown: 30° right.
    e.tick(2000 * MS);
    assert!(near(spot1(&e), degrees(50.0, 0.0)));
    // Half way through the fade in (half a cycle, 30° left): half size.
    let mut e2 = started({
        let mut m = circle(SPOT1_ONLY);
        m.center_pan = 20.0;
        m.fade_in = TimeValue::Ms(1000);
        m
    });
    e2.tick(500 * MS);
    let (pan, _) = spot1(&e2);
    let (half, _) = degrees(20.0 - 15.0, 0.0);
    assert!(pan.abs_diff(half) <= 2, "{pan} {half}");

    // Stopped: still playing while it shrinks, then gone.
    ok(&mut e, 3000 * MS, Command::StopFunction { id: FX });
    e.tick(3500 * MS);
    assert!(
        near(spot1(&e), degrees(20.0 - 15.0, 0.0)),
        "{:?}",
        spot1(&e)
    );
    assert!(
        e.snapshot().state.active.is_empty(),
        "stopping: not listed as playing"
    );
    e.tick(4000 * MS);
    e.tick(4001 * MS);
    // Its values are gone: the channel is back to its default.
    let mut idle = engine(show());
    idle.tick(0);
    assert_eq!(spot1(&e), spot1(&idle));
}

#[test]
fn a_stopped_movement_without_fade_out_leaves_at_once() {
    let mut e = started(circle(SPOT1_ONLY));
    e.tick(0);
    ok(&mut e, 10, Command::StopFunction { id: FX });
    e.tick(20);
    let mut idle = engine(show());
    idle.tick(0);
    assert_eq!(spot1(&e), spot1(&idle));
}

#[test]
fn a_relative_movement_adds_to_the_position_of_the_scenes() {
    let mut e = engine(show());
    let position = AttributeValue::Position {
        pan: 90.0,
        tilt: -20.0,
    };
    set(
        &mut e,
        0,
        FunctionId(3),
        scene(3, vec![explicit(fixture(SPOT1), position)], 0).kind,
    );
    let mut m = circle(SPOT1_ONLY);
    m.relative = true;
    m.center_tilt = 5.0; // an offset from where the scene puts it
    set(&mut e, 0, FX, FunctionKind::MovementEffect(m));
    // Started before the scene: the scene still comes first.
    ok(&mut e, 0, Command::StartFunction { id: FX });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(3) });
    e.tick(0);
    assert!(near(spot1(&e), degrees(120.0, -15.0)), "{:?}", spot1(&e));
    e.tick(250 * MS);
    assert!(near(spot1(&e), degrees(90.0, -5.0)), "{:?}", spot1(&e));

    // Without a scene, around the middle of the range.
    ok(
        &mut e,
        300 * MS,
        Command::StopFunction { id: FunctionId(3) },
    );
    e.tick(1000 * MS);
    assert!(near(spot1(&e), degrees(30.0, 5.0)), "{:?}", spot1(&e));
}

#[test]
fn a_relative_movement_follows_the_orientation_of_each_head() {
    // Spot 2 has its pan inverted: the scene's 90° and the shape's 30° both
    // turn the other way, as for an absolute effect.
    let mut e = engine(show());
    let position = AttributeValue::Position {
        pan: 90.0,
        tilt: 0.0,
    };
    set(
        &mut e,
        0,
        FunctionId(3),
        scene(3, vec![explicit(group(G_SPOTS), position)], 0).kind,
    );
    let mut m = circle(G_SPOTS);
    m.relative = true;
    set(&mut e, 0, FX, FunctionKind::MovementEffect(m));
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(3) });
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e.tick(0);
    let pan2 = u16::from(dmx(&e, 60)) << 8 | u16::from(dmx(&e, 61));
    assert!(near(spot1(&e), degrees(120.0, 0.0)));
    assert!(pan2.abs_diff(degrees(-120.0, 0.0).0) <= 2, "{pan2}");
}
