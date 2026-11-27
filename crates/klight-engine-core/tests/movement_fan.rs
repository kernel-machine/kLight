//! A fan on the DMX the lights get: the heads of the group open to opposite
//! sides, and a single head stays where it is.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_fixtures::convert::degrees_to_dmx16;
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;

const FX: FunctionId = FunctionId(30);
const SPOT1_ONLY: GroupId = GroupId(50);
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

fn fan(group: GroupId) -> MovementEffect {
    MovementEffect {
        group,
        shape: MovementShape::Fan { hold: 0 },
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

fn started(m: MovementEffect) -> klight_engine_core::Engine {
    let mut e = engine(show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FX,
                folder: None,
                name: "fan".into(),
                kind: FunctionKind::MovementEffect(m),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FX });
    e
}

/// 16-bit pan of the spot whose pan channel is at `address` (with its fine).
fn pan(e: &klight_engine_core::Engine, address: usize) -> u16 {
    u16::from(dmx(e, address)) << 8 | u16::from(dmx(e, address + 1))
}

fn near(a: u16, b: u16) -> bool {
    a.abs_diff(b) <= 2
}

#[test]
fn the_two_spots_of_a_fan_open_to_opposite_sides() {
    let mut e = started(fan(G_SPOTS));
    // Closed at the start: both at the center.
    e.tick(0);
    let center = degrees_to_dmx16(0.0, 540.0);
    assert!(near(pan(&e, 40), center), "{}", pan(&e, 40));
    assert!(near(pan(&e, 60), center), "{}", pan(&e, 60));
    // Open half way through the cycle: Spot 1, the first head, 30° to the
    // left. Spot 2, the last head, goes 30° to the right in the show, but it is
    // patched with its pan inverted, so on the DMX it gets Spot 1's value. If
    // both heads opened to the same side, Spot 1 would be at +30° instead.
    e.tick(500 * MS);
    let left = degrees_to_dmx16(-30.0, 540.0);
    assert!(near(pan(&e, 40), left), "{}", pan(&e, 40));
    assert!(near(pan(&e, 60), left), "{}", pan(&e, 60));
}

#[test]
fn a_fan_of_one_spot_does_not_move() {
    let mut e = started(fan(SPOT1_ONLY));
    let center = degrees_to_dmx16(0.0, 540.0);
    for t in [0, 250, 500, 750] {
        e.tick(t * MS);
        assert!(near(pan(&e, 40), center), "at {t} ms: {}", pan(&e, 40));
    }
}
