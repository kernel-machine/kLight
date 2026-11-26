//! Values tried out on the lights while editing, without touching the show.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::PREVIEW_HOLD_US;
use klight_model::*;

fn preview(values: Vec<SceneValue>) -> Command {
    Command::SetPreview {
        values,
        members: vec![group(G_DIMMERS)],
    }
}

fn level(target: Target, level: f32) -> SceneValue {
    explicit(target, intensity(level))
}

/// An engine with the dimmers at full from a running scene.
fn running() -> klight_engine_core::Engine {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![level(group(G_DIMMERS), 1.0)], 0),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(1_000);
    e
}

#[test]
fn a_preview_shows_on_top_of_what_is_running() {
    let mut e = running();
    // Lower than the running scene: what is previewed is what shows.
    ok(&mut e, 2_000, preview(vec![level(fixture(DIM1), 0.2)]));
    e.tick(3_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (51, 255));
    // Values of the whole scene reach the lights given with them.
    ok(&mut e, 4_000, preview(vec![level(Target::Scene, 0.4)]));
    e.tick(5_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (102, 102));
    ok(&mut e, 6_000, Command::ClearPreview);
    e.tick(7_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 255));
}

#[test]
fn a_preview_needs_no_running_function() {
    let mut e = engine(test_show());
    ok(&mut e, 0, preview(vec![level(fixture(DIM2), 1.0)]));
    e.tick(1_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 255));
    assert!(e.runtime_state().active.is_empty());
}

#[test]
fn grand_master_and_blackout_stay_above_it() {
    let mut e = engine(test_show());
    ok(&mut e, 0, preview(vec![level(fixture(DIM1), 1.0)]));
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.5 });
    e.tick(1_000);
    assert_eq!(dmx(&e, 1), 128);
    ok(&mut e, 2_000, Command::SetBlackout { on: true });
    e.tick(3_000);
    assert_eq!(dmx(&e, 1), 0);
}

#[test]
fn a_preview_nobody_renews_ends_by_itself() {
    let mut e = running();
    ok(&mut e, 1_000, preview(vec![level(fixture(DIM1), 0.0)]));
    e.tick(1_000 + PREVIEW_HOLD_US - 1);
    assert_eq!(dmx(&e, 1), 0);
    // Renewed: it holds for as long again.
    let again = 1_000 + PREVIEW_HOLD_US - 1;
    ok(&mut e, again, preview(vec![level(fixture(DIM1), 0.0)]));
    e.tick(again + PREVIEW_HOLD_US - 1);
    assert_eq!(dmx(&e, 1), 0);
    e.tick(again + PREVIEW_HOLD_US);
    assert_eq!(dmx(&e, 1), 255);
}

#[test]
fn a_preview_changes_neither_the_show_nor_the_state() {
    let mut e = running();
    let (show, before) = (e.show().clone(), e.snapshot());
    let out = ok(&mut e, 2_000, preview(vec![level(fixture(DIM1), 0.0)]));
    assert!(out.warnings.is_empty());
    e.tick(3_000);
    assert_eq!(e.show(), &show);
    // Nothing to publish, undo or save.
    assert_eq!(e.snapshot(), before);
    assert_eq!(e.shared().snapshot.load().seq, before.seq);
}

#[test]
fn a_preview_outlives_an_edit_of_the_show() {
    let mut e = engine(test_show());
    ok(&mut e, 0, preview(vec![level(fixture(DIM2), 1.0)]));
    ok(&mut e, 0, Command::SetShowName { name: "x".into() });
    e.tick(1_000);
    assert_eq!(dmx(&e, 2), 255);
}

#[test]
fn previews_belong_to_design_mode() {
    let mut e = engine(test_show());
    ok(&mut e, 0, preview(vec![level(fixture(DIM1), 1.0)]));
    let show_mode = Command::SetMode {
        mode: OperatingMode::Show,
        pin: None,
    };
    ok(&mut e, 0, show_mode);
    e.tick(1_000);
    assert_eq!(dmx(&e, 1), 0, "going live ends it");
    assert_eq!(
        err(&mut e, 2_000, preview(vec![level(fixture(DIM1), 1.0)])),
        CommandError::ShowModeLocked
    );
}
