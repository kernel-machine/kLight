//! Jumping a chaser to one of its steps (the editor's step buttons and clicks).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_model::function::{Direction, RunMode};
use klight_model::*;

const CHASE: FunctionId = FunctionId(10);

fn step(scene: u32, hold: Option<TimeValue>) -> ChaserStep {
    ChaserStep {
        scene: FunctionId(scene),
        hold,
        fade: None,
    }
}

/// Three scenes on three channels (1: DIM1, 2: DIM2, then the RGB par's
/// red), and a chaser of 1 beat steps at 120 BPM (500 ms), with `steps`.
fn rig(steps: Vec<ChaserStep>) -> klight_engine_core::Engine {
    let mut e = engine(test_show());
    for (id, target) in [(1, fixture(DIM1)), (2, fixture(DIM2)), (3, fixture(RGB))] {
        ok(
            &mut e,
            0,
            Command::SetFunction {
                function: scene(id, vec![explicit(target, intensity(1.0))], 0),
            },
        );
    }
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: CHASE,
                folder: None,
                name: "chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps,
                    hold: TimeValue::Beats(1.0),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    e
}

fn position(e: &klight_engine_core::Engine) -> Option<u32> {
    let snapshot = e.shared().snapshot.load_full();
    snapshot
        .state
        .active
        .iter()
        .find(|a| a.id == CHASE)
        .and_then(|a| a.position)
}

fn three() -> Vec<ChaserStep> {
    vec![step(1, None), step(2, None), step(3, None)]
}

#[test]
fn going_to_a_step_starts_a_stopped_chaser_there() {
    let mut e = rig(three());
    ok(
        &mut e,
        0,
        Command::ChaserGoTo {
            id: CHASE,
            index: 2,
        },
    );
    e.tick(10_000);
    assert_eq!(position(&e), Some(2));
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 0), "only the third scene");
}

#[test]
fn a_step_gone_to_lasts_its_full_hold_from_then() {
    let mut e = rig(three());
    ok(&mut e, 0, Command::StartFunction { id: CHASE });
    e.tick(300_000);
    assert_eq!(position(&e), Some(0));
    // At 300 ms, to step 1: it lasts 500 ms from now, not until 500 ms.
    ok(
        &mut e,
        300_000,
        Command::ChaserGoTo {
            id: CHASE,
            index: 1,
        },
    );
    e.tick(310_000);
    assert_eq!(position(&e), Some(1));
    assert_eq!(dmx(&e, 2), 255);
    e.tick(700_000);
    assert_eq!(position(&e), Some(1), "still within its hold");
    e.tick(820_000);
    assert_eq!(position(&e), Some(2), "then the chaser goes on from there");
}

#[test]
fn steps_are_named_by_their_place_in_the_show() {
    // The second step plays a function that does not exist: it is skipped
    // when playing, but the third is still number 2.
    let mut e = rig(vec![step(1, None), step(99, None), step(3, None)]);
    ok(
        &mut e,
        0,
        Command::ChaserGoTo {
            id: CHASE,
            index: 2,
        },
    );
    e.tick(10_000);
    assert_eq!(position(&e), Some(2));
    let missing = err(
        &mut e,
        10_000,
        Command::ChaserGoTo {
            id: CHASE,
            index: 1,
        },
    );
    assert!(
        matches!(missing, CommandError::NotFound { .. }),
        "{missing:?}"
    );
    assert_eq!(
        position(&e),
        Some(2),
        "a step that cannot play changes nothing"
    );
}

#[test]
fn only_chasers_have_steps_to_go_to() {
    let mut e = rig(three());
    let not_chaser = err(
        &mut e,
        0,
        Command::ChaserGoTo {
            id: FunctionId(1),
            index: 0,
        },
    );
    assert!(
        matches!(not_chaser, CommandError::NotFound { .. }),
        "{not_chaser:?}"
    );
    let beyond = err(
        &mut e,
        0,
        Command::ChaserGoTo {
            id: CHASE,
            index: 9,
        },
    );
    assert!(
        matches!(beyond, CommandError::NotFound { .. }),
        "{beyond:?}"
    );
}

#[test]
fn a_step_with_its_own_hold_keeps_it_when_gone_to() {
    let mut e = rig(vec![step(1, None), step(2, Some(TimeValue::Beats(2.0)))]);
    ok(
        &mut e,
        0,
        Command::ChaserGoTo {
            id: CHASE,
            index: 1,
        },
    );
    e.tick(10_000);
    e.tick(900_000);
    assert_eq!(position(&e), Some(1), "2 beats are 1 s at 120 BPM");
    e.tick(1_020_000);
    assert_eq!(position(&e), Some(0));
}
