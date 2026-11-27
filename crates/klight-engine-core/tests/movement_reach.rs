//! Where the heads of a movement effect can go: the limits the editor draws
//! and the warning when the shape goes beyond them, from the same heads and
//! ranges the engine plays.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::CompiledShow;
use klight_engine_core::preview::movement_reach;
use klight_model::function::{Direction, MovementShape};
use klight_model::inspect::MovementReach;
use klight_model::*;

/// The test spot moves 540° in pan and 270° in tilt.
const SPOT_PAN: f32 = 270.0;
const SPOT_TILT: f32 = 135.0;

fn circle(group: GroupId, center_tilt: f32, height: f32) -> MovementEffect {
    MovementEffect {
        group,
        shape: MovementShape::Circle,
        center_pan: 0.0,
        center_tilt,
        width: 90.0,
        height,
        period: TimeValue::Ms(1000),
        phase_spread: 0.0,
        direction: Direction::Forward,
        rotation: 0.0,
        mirror: MovementMirror::None,
        run_mode: klight_model::function::RunMode::Loop,
        relative: false,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    }
}

fn reach(show: &Show, m: &MovementEffect) -> MovementReach {
    movement_reach(show, &CompiledShow::build(show), m)
}

#[test]
fn spots_give_their_range_and_their_names_in_group_order() {
    let show = test_show();
    let r = reach(&show, &circle(G_SPOTS, 0.0, 90.0));
    assert_eq!(r.pan_limit, Some(SPOT_PAN));
    assert_eq!(r.tilt_limit, Some(SPOT_TILT));
    assert!(!r.clipped);
    let names: Vec<_> = r.heads.iter().map(|h| (h.name.as_str(), h.moves)).collect();
    assert_eq!(names, [("Spot 1", true), ("Spot 2", true)]);
}

#[test]
fn a_shape_beyond_the_tilt_range_is_clipped_and_the_engine_stops_at_the_limit() {
    let show = test_show();
    // Up to 150° of tilt, past the 135° the spots reach.
    let m = circle(G_SPOTS, 120.0, 60.0);
    assert!(reach(&show, &m).clipped);

    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(30),
                folder: None,
                name: "high".into(),
                kind: FunctionKind::MovementEffect(m),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(30) });
    // A quarter of the cycle: the top of the circle.
    e.tick(250_000);
    assert_eq!(dmx(&e, 42), 255, "tilt held at its limit");
}

#[test]
fn a_shape_reaching_exactly_the_limit_is_not_clipped() {
    let show = test_show();
    assert!(!reach(&show, &circle(G_SPOTS, 105.0, 60.0)).clipped);
}

#[test]
fn swapped_axes_take_the_range_of_the_other_axis() {
    let mut show = test_show();
    show.fixtures
        .iter_mut()
        .find(|f| f.id == SPOT1)
        .unwrap()
        .orientation
        .swap_pan_tilt = true;
    // Spot 1's effect pan goes to its tilt: 135° at most, for both spots.
    let mut m = circle(G_SPOTS, 0.0, 90.0);
    m.width = 300.0;
    let r = reach(&show, &m);
    assert_eq!(r.pan_limit, Some(SPOT_TILT));
    assert_eq!(r.tilt_limit, Some(SPOT_TILT));
    assert!(r.clipped);
}

#[test]
fn heads_without_pan_or_tilt_are_listed_but_give_no_limit() {
    let mut show = test_show();
    show.groups.push(FixtureGroup {
        id: GroupId(9),
        name: "Mixed".into(),
        members: [DIM1, SPOT1]
            .into_iter()
            .map(|fixture| GroupMember {
                fixture,
                head: 0,
                grid: None,
            })
            .collect(),
        grid_width: 0,
        grid_height: 0,
    });
    let r = reach(&show, &circle(GroupId(9), 0.0, 90.0));
    let moves: Vec<_> = r.heads.iter().map(|h| (h.name.as_str(), h.moves)).collect();
    assert_eq!(moves, [("Dim 1", false), ("Spot 1", true)]);
    assert_eq!(r.pan_limit, Some(SPOT_PAN));

    let dimmers = reach(&show, &circle(G_DIMMERS, 0.0, 400.0));
    assert_eq!(dimmers.pan_limit, None);
    assert_eq!(dimmers.tilt_limit, None);
    assert!(!dimmers.clipped, "nothing to clip without pan or tilt");
}

#[test]
fn multi_head_fixtures_number_their_heads() {
    let show = test_show();
    let r = reach(&show, &circle(G_PIXELS, 0.0, 90.0));
    let names: Vec<_> = r.heads.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Bar · 1", "Bar · 2", "Bar · 3", "Bar · 4"]);
}

#[test]
fn a_rotated_shape_is_measured_as_drawn() {
    let show = test_show();
    // 300° wide fits the pan; turned upright it goes past the 135° of tilt.
    let mut m = circle(G_SPOTS, 0.0, 10.0);
    m.width = 300.0;
    assert!(!reach(&show, &m).clipped);
    m.rotation = 90.0;
    assert!(reach(&show, &m).clipped);
}

#[test]
fn mirrored_heads_are_marked() {
    let show = test_show();
    let mut m = circle(G_SPOTS, 0.0, 10.0);
    m.mirror = MovementMirror::Alternate;
    let r = reach(&show, &m);
    let mirrored: Vec<_> = r.heads.iter().map(|h| h.mirrored).collect();
    assert_eq!(mirrored, [false, true]);
    // Near the edge, the mirrored copy is measured too: it stays inside.
    m.center_pan = -200.0;
    m.width = 100.0;
    m.rotation = 30.0;
    assert!(!reach(&show, &m).clipped);
    m.width = 170.0;
    assert!(reach(&show, &m).clipped);
}
