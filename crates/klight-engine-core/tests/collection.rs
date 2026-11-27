//! Collections: functions played all together.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use klight_engine_core::Engine;
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;

fn function(id: u32, kind: FunctionKind) -> Function {
    Function {
        id: FunctionId(id),
        folder: None,
        name: format!("f{id}"),
        kind,
    }
}

fn collection(id: u32, members: &[u32]) -> Function {
    function(
        id,
        FunctionKind::Collection(Collection {
            functions: members.iter().copied().map(FunctionId).collect(),
        }),
    )
}

/// Steps of one second each, with the given crossfade.
fn chaser(id: u32, steps: &[u32], fade_ms: u32, run_mode: RunMode) -> Function {
    function(
        id,
        FunctionKind::Chaser(Chaser {
            steps: steps
                .iter()
                .map(|&s| ChaserStep {
                    scene: FunctionId(s),
                    hold: None,
                    fade: None,
                })
                .collect(),
            hold: TimeValue::Ms(1000),
            fade: TimeValue::Ms(fade_ms),
            direction: Direction::Forward,
            run_mode,
        }),
    )
}

fn color(r: f32, g: f32, b: f32) -> AttributeValue {
    AttributeValue::Color {
        color: Color::new(r, g, b),
    }
}

/// 30: a circle of the spots. 40: a red column moving over the pixel bar.
fn add_effects(e: &mut Engine) {
    let movement = FunctionKind::MovementEffect(MovementEffect {
        group: G_SPOTS,
        shape: MovementShape::Circle,
        center_pan: 0.0,
        center_tilt: 0.0,
        width: 90.0,
        height: 90.0,
        period: TimeValue::Ms(1000),
        phase_spread: 0.0,
        direction: Direction::Forward,
        rotation: 0.0,
        mirror: MovementMirror::None,
        run_mode: klight_model::function::RunMode::Loop,
        relative: false,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    });
    let matrix = FunctionKind::MatrixEffect(MatrixEffect {
        group: G_PIXELS,
        algorithm: MatrixAlgorithm::Chase {
            color: Color::RED,
            background: Color::BLACK,
        },
        period: TimeValue::Ms(400),
        direction: Direction::Forward,
        axis: MatrixAxis::Horizontal,
        run_mode: klight_model::function::RunMode::Loop,
        fade_in: TimeValue::ZERO,
        fade_out: TimeValue::ZERO,
    });
    set(e, 0, function(30, movement));
    set(e, 0, function(40, matrix));
}

fn matrix_reds(e: &Engine) -> Vec<u8> {
    (0..4).map(|h| dmx(e, 100 + h * 3)).collect()
}

fn set(e: &mut Engine, now: u64, f: Function) {
    ok(e, now, Command::SetFunction { function: f });
}

fn start(e: &mut Engine, now: u64, id: u32) {
    ok(e, now, Command::StartFunction { id: FunctionId(id) });
}

fn stop(e: &mut Engine, now: u64, id: u32) {
    ok(e, now, Command::StopFunction { id: FunctionId(id) });
}

/// Red, green and blue of the RGB par.
fn rgb(e: &Engine) -> (u8, u8, u8) {
    (dmx(e, 10), dmx(e, 11), dmx(e, 12))
}

fn dimmers(e: &Engine) -> (u8, u8) {
    (dmx(e, 1), dmx(e, 2))
}

fn playing(e: &Engine) -> Vec<u32> {
    let mut ids: Vec<u32> = e.snapshot().state.active[0]
        .playing
        .iter()
        .map(|f| f.0)
        .collect();
    ids.sort_unstable();
    ids
}

/// 1: dimmer 1. 2: dimmer 2. 3: RGB reddish. 4: RGB greenish. 5: RGB blue.
fn looks() -> Engine {
    let mut e = engine(test_show());
    let full = |f| explicit(fixture(f), intensity(1.0));
    set(&mut e, 0, scene(1, vec![full(DIM1)], 0));
    set(&mut e, 0, scene(2, vec![full(DIM2)], 0));
    for (id, c) in [
        (3, color(1.0, 0.0, 0.0)),
        (4, color(0.0, 1.0, 0.0)),
        (5, color(0.0, 0.0, 1.0)),
    ] {
        set(
            &mut e,
            0,
            scene(id, vec![full(RGB), explicit(fixture(RGB), c)], 0),
        );
    }
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    e
}

#[test]
fn a_collection_plays_all_its_scenes_together() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[1, 2]));
    start(&mut e, 0, 20);
    e.tick(0);
    assert_eq!(dimmers(&e), (255, 255));
    assert_eq!(playing(&e), vec![1, 2]);
    stop(&mut e, 0, 20);
    e.tick(0);
    assert_eq!(dimmers(&e), (0, 0));
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn scenes_of_a_collection_merge_htp_on_every_channel() {
    let mut e = looks();
    // In both orders: the highest value wins, not the last scene.
    for (id, members) in [(20, [3, 5]), (21, [5, 3])] {
        set(&mut e, 0, collection(id, &members));
        start(&mut e, 0, id);
        e.tick(0);
        assert_eq!(rgb(&e), (255, 0, 255));
        stop(&mut e, 0, id);
        e.tick(0);
    }
}

#[test]
fn positions_merge_htp_inside_a_collection() {
    let mut e = looks();
    let look = |id, pan| {
        let position = AttributeValue::Position { pan, tilt: 0.0 };
        scene(id, vec![explicit(fixture(SPOT1), position)], 0)
    };
    set(&mut e, 0, look(6, 200.0));
    set(&mut e, 0, look(7, -100.0));
    start(&mut e, 0, 6);
    e.tick(0);
    let high = dmx(&e, 40);
    stop(&mut e, 0, 6);
    set(&mut e, 0, collection(20, &[6, 7]));
    start(&mut e, 0, 20);
    e.tick(0);
    assert_eq!(dmx(&e, 40), high);
}

#[test]
fn outside_a_collection_the_usual_rules_apply() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[3, 5]));
    start(&mut e, 0, 20);
    // Started later: it wins the color, and the dimmers are HTP as ever.
    start(&mut e, 0, 4);
    e.tick(0);
    assert_eq!(rgb(&e), (0, 255, 0));
    // The collection started after the scene wins in turn.
    stop(&mut e, 0, 20);
    start(&mut e, 0, 20);
    e.tick(0);
    assert_eq!(rgb(&e), (255, 0, 255));
}

#[test]
fn a_collection_runs_scenes_and_effects_in_parallel() {
    let mut e = looks();
    add_effects(&mut e);
    set(&mut e, 0, collection(20, &[1, 30, 40, 1]));
    start(&mut e, 0, 20);
    e.tick(0);
    let pan = dmx(&e, 40);
    assert_eq!((dmx(&e, 1), matrix_reds(&e)), (255, vec![255, 0, 0, 0]));
    // Listed twice, the scene plays once.
    assert_eq!(playing(&e), vec![1, 30, 40]);
    e.tick(100_000);
    assert_ne!(dmx(&e, 40), pan);
    assert_eq!(matrix_reds(&e), vec![0, 255, 0, 0]);
    assert_eq!(e.snapshot().state.active.len(), 1);
    // Stopped, the effects end at once.
    stop(&mut e, 100_000, 20);
    e.tick(100_001);
    assert_eq!((dmx(&e, 1), matrix_reds(&e)), (0, vec![0; 4]));
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn a_collection_of_effects_only_stops_at_once() {
    let mut e = looks();
    add_effects(&mut e);
    set(&mut e, 0, collection(20, &[40]));
    start(&mut e, 0, 20);
    e.tick(0);
    stop(&mut e, 0, 20);
    assert!(e.runtime_state().active.is_empty());
}

#[test]
fn stopping_a_collection_fades_each_scene_out_by_its_own_time() {
    let mut e = looks();
    set(
        &mut e,
        0,
        scene(6, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
    );
    let mut slow = scene(7, vec![explicit(fixture(DIM2), intensity(1.0))], 0);
    if let FunctionKind::Scene(s) = &mut slow.kind {
        s.timing.fade_out = TimeValue::Ms(1000);
    }
    set(&mut e, 0, slow);
    set(&mut e, 0, collection(20, &[6, 7]));
    start(&mut e, 0, 20);
    e.tick(0);
    stop(&mut e, 0, 20);
    e.tick(500_000);
    assert_eq!(dimmers(&e), (0, 128));
    e.tick(1_000_000);
    e.tick(1_000_001);
    assert_eq!(dimmers(&e), (0, 0));
}

#[test]
fn a_collection_level_scales_the_intensity_of_its_members() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[1, 2]));
    start(&mut e, 0, 20);
    ok(
        &mut e,
        0,
        Command::SetFunctionLevel {
            id: FunctionId(20),
            level: 0.5,
        },
    );
    e.tick(0);
    assert_eq!(dimmers(&e), (128, 128));
}

#[test]
fn a_chaser_steps_through_collections() {
    let mut e = looks();
    add_effects(&mut e);
    set(&mut e, 0, collection(20, &[3, 5, 40]));
    set(&mut e, 0, collection(21, &[4, 1]));
    set(&mut e, 0, chaser(10, &[20, 21], 500, RunMode::Loop));
    start(&mut e, 0, 10);
    e.tick(0);
    // The step's fade is the crossfade of the collection's scenes.
    e.tick(250_000);
    let (r, _, b) = rgb(&e);
    assert!(r > 0 && r < 255 && b == r, "{r} {b}");
    e.tick(500_000);
    assert_eq!(rgb(&e), (255, 0, 255));
    assert_eq!(playing(&e), vec![3, 5, 20, 40]);
    // Its effects run from the start of the step.
    assert_eq!(matrix_reds(&e), vec![0, 255, 0, 0]);

    // To the next collection: the two crossfade (not merged HTP with one
    // another), and the effects of the first are over.
    e.tick(1_000_000);
    assert_eq!(playing(&e), vec![1, 4, 21]);
    assert_eq!(matrix_reds(&e), vec![0; 4]);
    e.tick(1_250_000);
    let (r, g, b) = rgb(&e);
    assert!(r > 0 && r < 255 && g > 0 && g < 255 && b > 0 && b < 255);
    e.tick(1_500_000);
    assert_eq!((dmx(&e, 1), rgb(&e)), (255, (0, 255, 0)));

    // And back, the effect from its start.
    e.tick(2_000_000);
    assert_eq!(matrix_reds(&e), vec![255, 0, 0, 0]);
    e.tick(2_500_000);
    assert_eq!(rgb(&e), (255, 0, 255));
    stop(&mut e, 2_500_000, 10);
    e.tick(3_100_000);
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn edits_show_at_once_in_running_collections() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[1, 3]));
    set(&mut e, 0, collection(21, &[2]));
    set(&mut e, 0, chaser(10, &[4, 21], 500, RunMode::Loop));
    start(&mut e, 0, 20);
    start(&mut e, 0, 10);
    e.tick(0);
    e.tick(1_000_000);
    e.tick(1_600_000);
    assert_eq!(dimmers(&e), (255, 255));
    // A member added starts, a removed one stops; the chaser stays on its step.
    set(&mut e, 1_600_000, collection(20, &[5]));
    set(&mut e, 1_600_000, collection(21, &[2, 1]));
    e.tick(1_600_001);
    assert_eq!((dimmers(&e), rgb(&e)), ((255, 255), (0, 0, 255)));
    // Edits of a member show at once, without fading.
    set(
        &mut e,
        1_600_001,
        scene(2, vec![explicit(fixture(DIM2), intensity(0.5))], 0),
    );
    e.tick(1_600_002);
    assert_eq!(dimmers(&e), (255, 128));
}

#[test]
fn restore_brings_back_collections() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[1, 3]));
    set(&mut e, 0, collection(21, &[2, 5]));
    set(&mut e, 0, chaser(10, &[4, 21], 500, RunMode::Loop));
    start(&mut e, 0, 10);
    start(&mut e, 0, 20);
    e.tick(0);
    e.tick(1_000_000);
    e.tick(1_600_000);
    let state = e.runtime_state();
    let show = (**e.show()).clone();
    let mut restored = engine(show);
    assert!(restored.restore(0, &state).is_empty());
    restored.tick(0);
    // The collection started last wins the color.
    assert_eq!(
        (dimmers(&restored), rgb(&restored)),
        ((255, 255), (255, 0, 0))
    );
    assert_eq!(restored.runtime_state().active, state.active);
}

#[test]
fn a_collection_with_nothing_to_play_does_not_start() {
    let mut e = looks();
    set(&mut e, 0, collection(20, &[]));
    let error = err(&mut e, 0, Command::StartFunction { id: FunctionId(20) });
    assert!(
        error.to_string().contains("no playable functions"),
        "{error}"
    );
}

#[test]
fn a_collection_cannot_play_functions_that_contain_others() {
    let mut e = looks();
    set(&mut e, 0, chaser(10, &[2], 0, RunMode::Loop));
    set(
        &mut e,
        0,
        function(
            11,
            FunctionKind::CueList(CueList {
                cues: Vec::new(),
                move_in_black: false,
            }),
        ),
    );
    set(&mut e, 0, collection(21, &[2]));
    let out = ok(
        &mut e,
        0,
        Command::SetFunction {
            function: collection(20, &[10, 11, 21, 20, 99, 1]),
        },
    );
    assert_eq!(out.warnings.len(), 5, "{:?}", out.warnings);
    assert!(out
        .warnings
        .iter()
        .all(|w| w.contains("a collection cannot play")));
    start(&mut e, 0, 20);
    e.tick(0);
    assert_eq!((dimmers(&e), playing(&e)), ((255, 0), vec![1]));
}
