//! The shapes a movement effect can draw: polygons with any number of sides,
//! sine waves and fan sweeps. Checked on the unit shape the engine and the
//! editor's preview share.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use klight_engine_core::effects::{
    head_mirrored, head_spread, movement_path_point, movement_point, shape_point,
};
use klight_model::function::{Direction, MovementMirror, MovementShape, RunMode};
use klight_model::{MovementEffect, TimeValue};

const EPS: f32 = 1e-4;

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() < EPS
}

/// Samples of one cycle, the start repeated at the end to check it closes.
fn cycle(shape: MovementShape, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| shape_point(shape, i as f32 / n as f32))
        .collect()
}

#[test]
fn a_polygon_of_four_sides_is_the_square_it_replaces() {
    let square = MovementShape::Polygon { sides: 4 };
    assert!(near(shape_point(square, 0.0), (-1.0, -1.0)));
    assert!(near(shape_point(square, 0.25), (1.0, -1.0)));
    assert!(near(shape_point(square, 0.5), (1.0, 1.0)));
    assert!(near(shape_point(square, 0.75), (-1.0, 1.0)));
    // Half way along the first side.
    assert!(near(shape_point(square, 0.125), (0.0, -1.0)));
}

#[test]
fn a_polygon_reaches_each_of_its_corners_in_turn() {
    // A triangle: flat at the bottom, a corner at the top, three equal parts
    // of the cycle.
    let triangle = MovementShape::Polygon { sides: 3 };
    let (a, b, c) = (
        shape_point(triangle, 0.0),
        shape_point(triangle, 1.0 / 3.0),
        shape_point(triangle, 2.0 / 3.0),
    );
    assert!(near(a, (-1.0, -1.0)), "{a:?}");
    assert!(near(b, (1.0, -1.0)), "{b:?}");
    assert!(near(c, (0.0, 1.0)), "{c:?}");
    // Between two corners it goes straight.
    assert!(near(shape_point(triangle, 1.0 / 6.0), (0.0, -1.0)));
}

#[test]
fn every_polygon_fits_the_unit_square_and_closes() {
    for sides in 3..=12 {
        let shape = MovementShape::Polygon { sides };
        let points = cycle(shape, 240);
        for (x, y) in &points {
            assert!(
                x.abs() <= 1.0 + EPS && y.abs() <= 1.0 + EPS,
                "{sides}: {x} {y}"
            );
        }
        // The corners reach the edges on both axes: the sliders mean what they say.
        let corners: Vec<(f32, f32)> = (0..sides)
            .map(|k| shape_point(shape, f32::from(k) / f32::from(sides)))
            .collect();
        let max_x = corners.iter().fold(0.0_f32, |m, c| m.max(c.0.abs()));
        let max_y = corners.iter().fold(0.0_f32, |m, c| m.max(c.1.abs()));
        assert!((max_x - 1.0).abs() < 1e-3, "{sides} sides: width {max_x}");
        assert!((max_y - 1.0).abs() < 1e-3, "{sides} sides: height {max_y}");
        assert!(near(points[0], points[240]), "{sides} sides does not close");
    }
}

#[test]
fn the_number_of_sides_is_kept_in_a_range_that_draws_something() {
    // Fewer than three sides draw a triangle, a lot more than twelve a dodecagon:
    // never nothing and never a division by zero.
    for sides in [0_u8, 1, 2, 255] {
        for (x, y) in cycle(MovementShape::Polygon { sides }, 60) {
            assert!(x.is_finite() && y.is_finite());
            assert!(x.abs() <= 1.0 + EPS && y.abs() <= 1.0 + EPS);
        }
    }
    assert!(near(
        shape_point(MovementShape::Polygon { sides: 1 }, 0.3),
        shape_point(MovementShape::Polygon { sides: 3 }, 0.3)
    ));
    assert!(near(
        shape_point(MovementShape::Polygon { sides: 200 }, 0.3),
        shape_point(MovementShape::Polygon { sides: 12 }, 0.3)
    ));
}

#[test]
fn a_sine_goes_across_and_comes_back_along_the_same_wave() {
    let sine = MovementShape::Sine { periods: 2 };
    // Across: from the left to the right edge, half way at the middle of the
    // cycle's first half.
    assert!(near(shape_point(sine, 0.0), (-1.0, 0.0)));
    assert!(near(shape_point(sine, 0.5), (1.0, 0.0)));
    assert!(near(shape_point(sine, 1.0), (-1.0, 0.0)));
    // Two periods: crest a quarter of a period in, which is an eighth of the way.
    let (x, y) = shape_point(sine, 0.0625);
    assert!((x + 0.75).abs() < EPS && (y - 1.0).abs() < EPS, "{x} {y}");
    // The way back retraces the same points (mirrored in time).
    for i in 1..20 {
        let t = i as f32 / 40.0;
        assert!(
            near(shape_point(sine, t), shape_point(sine, 1.0 - t)),
            "t = {t}"
        );
    }
}

#[test]
fn a_sine_has_as_many_crests_as_periods() {
    for periods in 1..=8_u8 {
        let points = cycle(MovementShape::Sine { periods }, 4000);
        // Crests on the way across (the first half), where y rises above 0.99
        // after being below.
        let mut crests = 0;
        let mut above = false;
        for (_, y) in points.iter().take(2001) {
            if *y > 0.99 && !above {
                crests += 1;
            }
            above = *y > 0.99;
        }
        assert_eq!(crests, i32::from(periods), "{periods} periods");
    }
}

#[test]
fn the_line_a_fan_covers_is_swept_at_a_steady_speed_with_holds_at_the_ends() {
    let fan = MovementShape::Fan { hold: 20 };
    // 20% of the cycle at each end, 30% sweeping each way.
    assert!(near(shape_point(fan, 0.0), (-1.0, 0.0)));
    assert!(near(shape_point(fan, 0.15), (0.0, 0.0)));
    assert!(near(shape_point(fan, 0.3), (1.0, 0.0)));
    assert!(near(shape_point(fan, 0.4), (1.0, 0.0)));
    assert!(near(shape_point(fan, 0.5), (1.0, 0.0)));
    assert!(near(shape_point(fan, 0.65), (0.0, 0.0)));
    assert!(near(shape_point(fan, 0.8), (-1.0, 0.0)));
    assert!(near(shape_point(fan, 0.9), (-1.0, 0.0)));
    // It stays on its line.
    for (_, y) in cycle(fan, 100) {
        assert!(y.abs() < EPS);
    }
}

#[test]
fn the_line_of_a_fan_without_hold_never_stops_and_one_with_too_much_is_capped() {
    let none = MovementShape::Fan { hold: 0 };
    assert!(near(shape_point(none, 0.25), (0.0, 0.0)));
    assert!(near(shape_point(none, 0.5), (1.0, 0.0)));
    // More than the cap still sweeps.
    let capped = MovementShape::Fan { hold: 255 };
    let (a, b) = (shape_point(capped, 0.05), shape_point(capped, 0.1));
    assert!(a.0.is_finite() && b.0.is_finite());
    let xs: Vec<f32> = cycle(capped, 200).iter().map(|p| p.0).collect();
    assert!(xs.iter().any(|x| *x > 0.99) && xs.iter().any(|x| *x < -0.99));
    assert!(xs.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-3));
}

#[test]
fn a_show_saved_with_a_square_still_loads_as_a_four_sided_polygon() {
    let old: MovementShape = serde_json::from_str(r#"{"type":"square"}"#).unwrap();
    assert_eq!(old, MovementShape::Polygon { sides: 4 });
    let polygon: MovementShape = serde_json::from_str(r#"{"type":"polygon","sides":6}"#).unwrap();
    assert_eq!(polygon, MovementShape::Polygon { sides: 6 });
    // A polygon without its sides is a square too.
    let bare: MovementShape = serde_json::from_str(r#"{"type":"polygon"}"#).unwrap();
    assert_eq!(bare, MovementShape::Polygon { sides: 4 });
}

/// A fan of 60° that opens and closes once a second.
fn fan(hold: u8) -> MovementEffect {
    MovementEffect {
        group: klight_model::GroupId(1),
        shape: MovementShape::Fan { hold },
        center_pan: 10.0,
        center_tilt: -5.0,
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

/// Where head `order` of `n` is at `turns` of the cycle.
fn head(e: &MovementEffect, turns: f64, order: usize, n: usize) -> (f32, f32) {
    movement_point(
        e,
        turns,
        0.0,
        head_mirrored(e, order, n),
        head_spread(order, n),
    )
}

#[test]
fn the_heads_of_a_group_are_spread_from_the_first_to_the_last() {
    let spreads = |n| (0..n).map(|i| head_spread(i, n)).collect::<Vec<_>>();
    assert_eq!(spreads(1), [0.0]);
    assert_eq!(spreads(2), [-1.0, 1.0]);
    assert_eq!(spreads(3), [-1.0, 0.0, 1.0]);
    let four = spreads(4);
    assert!((four[1] + 1.0 / 3.0).abs() < EPS && (four[2] - 1.0 / 3.0).abs() < EPS);
    assert_eq!(head_spread(0, 0), 0.0);
}

#[test]
fn a_fan_starts_closed_and_opens_its_heads_to_both_sides() {
    let e = fan(0);
    // Closed: every head at the center.
    for i in 0..3 {
        assert!(near(head(&e, 0.0, i, 3), (10.0, -5.0)), "head {i}");
    }
    // Open, half way through the cycle: the outer heads at the edges of the
    // width, the one in the middle where it was.
    assert!(near(head(&e, 0.5, 0, 3), (-20.0, -5.0)));
    assert!(near(head(&e, 0.5, 1, 3), (10.0, -5.0)));
    assert!(near(head(&e, 0.5, 2, 3), (40.0, -5.0)));
    // Half open: half way there.
    assert!(near(head(&e, 0.25, 0, 3), (-5.0, -5.0)));
    assert!(near(head(&e, 0.25, 2, 3), (25.0, -5.0)));
    // And closed again.
    assert!(near(head(&e, 1.0, 0, 3), (10.0, -5.0)));
}

#[test]
fn the_heads_of_a_fan_open_together_not_one_after_the_other() {
    let e = fan(0);
    for turns in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let (a, _) = head(&e, turns, 0, 4);
        let (d, _) = head(&e, turns, 3, 4);
        // Symmetric around the center, at every moment.
        assert!((a + d - 20.0).abs() < EPS, "turns {turns}: {a} {d}");
    }
}

#[test]
fn a_fan_holds_open_and_closed_for_the_share_of_the_cycle_asked() {
    let e = fan(20);
    // Closed for the first... the cycle opens in 30%, holds open for 20%,
    // closes in 30% and holds closed for the last 20%.
    assert!(near(head(&e, 0.3, 2, 3), (40.0, -5.0)));
    assert!(near(head(&e, 0.4, 2, 3), (40.0, -5.0)));
    assert!(near(head(&e, 0.5, 2, 3), (40.0, -5.0)));
    assert!(near(head(&e, 0.65, 2, 3), (25.0, -5.0)));
    assert!(near(head(&e, 0.85, 2, 3), (10.0, -5.0)));
    assert!(near(head(&e, 0.95, 2, 3), (10.0, -5.0)));
}

#[test]
fn a_single_head_has_no_fan_to_open() {
    let e = fan(0);
    for turns in [0.0, 0.25, 0.5, 0.75] {
        assert!(near(head(&e, turns, 0, 1), (10.0, -5.0)), "turns {turns}");
    }
}

#[test]
fn a_fan_can_be_turned_and_its_heads_can_be_mirrored() {
    // Turned a quarter, it opens up and down instead of left and right.
    let mut e = fan(0);
    e.rotation = 90.0;
    assert!(near(head(&e, 0.5, 2, 3), (10.0, 25.0)));
    assert!(near(head(&e, 0.5, 0, 3), (10.0, -35.0)));
}

#[test]
fn mirroring_does_not_change_a_fan() {
    // A fan already sends each head to its own side: whichever heads are
    // asked to mirror, the positions are those without mirroring.
    for mirror in [MovementMirror::Alternate, MovementMirror::Halves] {
        for n in 2..=4 {
            let plain = fan(0);
            let mut mirrored = fan(0);
            mirrored.mirror = mirror;
            for i in 0..n {
                for turns in [0.0, 0.25, 0.5, 0.75] {
                    assert!(
                        near(head(&plain, turns, i, n), head(&mirrored, turns, i, n)),
                        "{mirror:?} head {i} of {n} at {turns}"
                    );
                }
            }
        }
    }
    // The preview line is the same too.
    let e = fan(0);
    assert!(near(
        movement_path_point(&e, 0.5, true),
        movement_path_point(&e, 0.5, false)
    ));
    // And no head is reported as mirrored.
    let mut e = fan(0);
    e.mirror = MovementMirror::Halves;
    assert!((0..4).all(|i| !head_mirrored(&e, i, 4)));
    // For any other shape it still follows the setting.
    let mut c = fan(0);
    c.shape = MovementShape::Circle;
    c.mirror = MovementMirror::Halves;
    assert!(head_mirrored(&c, 3, 4) && !head_mirrored(&c, 0, 4));
}

#[test]
fn the_preview_draws_the_line_the_whole_fan_covers() {
    let e = fan(0);
    let (left, _) = movement_path_point(&e, 0.0, false);
    let (right, _) = movement_path_point(&e, 0.5, false);
    assert!((left + 20.0).abs() < EPS && (right - 40.0).abs() < EPS);
}
