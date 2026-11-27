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
    assert_eq!(r.pan_limit, Some((-SPOT_PAN, SPOT_PAN)));
    assert_eq!(r.tilt_limit, Some((-SPOT_TILT, SPOT_TILT)));
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
    assert_eq!(r.pan_limit, Some((-SPOT_TILT, SPOT_TILT)));
    assert_eq!(r.tilt_limit, Some((-SPOT_TILT, SPOT_TILT)));
    assert!(r.clipped);
}

const LIMITS: PositionLimits = PositionLimits {
    pan_min: -90.0,
    pan_max: 60.0,
    tilt_min: -20.0,
    tilt_max: 30.0,
};

fn limited(limits: Option<PositionLimits>, fixtures: &[FixtureId]) -> Show {
    let mut show = test_show();
    for f in &mut show.fixtures {
        if fixtures.contains(&f.id) {
            f.position_limits = limits;
        }
    }
    show
}

#[test]
fn the_position_limits_of_the_lights_are_what_they_reach() {
    let show = limited(Some(LIMITS), &[SPOT1, SPOT2]);
    let r = reach(&show, &circle(G_SPOTS, 0.0, 30.0));
    assert_eq!(r.pan_limit, Some((-90.0, 60.0)));
    assert_eq!(r.tilt_limit, Some((-20.0, 30.0)));
    assert!(!r.clipped, "a circle of 45 by 15 degrees fits");
}

#[test]
fn a_shape_beyond_a_position_limit_is_clipped_on_that_side_only() {
    let show = limited(Some(LIMITS), &[SPOT1, SPOT2]);
    // Tilt from -25 to +25: past the -20 below.
    let mut m = circle(G_SPOTS, 0.0, 50.0);
    assert!(reach(&show, &m).clipped);
    // Shifted up, from -15 to +35: past the +30 above instead.
    m.center_tilt = 10.0;
    assert!(reach(&show, &m).clipped);
    // From -7 to +17: inside both.
    m.height = 24.0;
    m.center_tilt = 5.0;
    assert!(!reach(&show, &m).clipped);
    // On the pan: 60° at most to one side, 90° to the other.
    let mut wide = circle(G_SPOTS, 0.0, 10.0);
    wide.width = 140.0;
    assert!(reach(&show, &wide).clipped, "+70 is past +60");
    wide.center_pan = -15.0;
    assert!(!reach(&show, &wide).clipped, "-85 to +55 fits");
}

#[test]
fn the_narrowest_limits_of_the_heads_are_the_reach_of_the_group() {
    // Only Spot 1 is limited: the group reaches what both do.
    let show = limited(Some(LIMITS), &[SPOT1]);
    let r = reach(&show, &circle(G_SPOTS, 0.0, 30.0));
    assert_eq!(r.pan_limit, Some((-90.0, 60.0)));
    assert_eq!(r.tilt_limit, Some((-20.0, 30.0)));
}

#[test]
fn the_limits_are_those_of_the_show_even_when_the_axes_are_swapped() {
    let mut show = limited(Some(LIMITS), &[SPOT1]);
    show.fixtures
        .iter_mut()
        .find(|f| f.id == SPOT1)
        .unwrap()
        .orientation
        .swap_pan_tilt = true;
    // The pan of the show goes to the tilt channel (135° each way), cut by
    // the pan limits of the show.
    let r = reach(&show, &circle(G_SPOTS, 0.0, 30.0));
    assert_eq!(r.pan_limit, Some((-90.0, 60.0)));
    assert_eq!(r.tilt_limit, Some((-20.0, 30.0)));
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
    assert_eq!(r.pan_limit, Some((-SPOT_PAN, SPOT_PAN)));

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
