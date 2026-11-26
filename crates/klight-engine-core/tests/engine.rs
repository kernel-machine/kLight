#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::vc::{ButtonAction, VirtualConsole, Widget, WidgetKind, WidgetProps};
use klight_model::*;

#[test]
fn empty_show_outputs_zeros() {
    let mut e = engine(test_show());
    e.tick(0);
    assert!(e.frame(U1).unwrap().iter().all(|&b| b == 0));
}

#[test]
fn scene_fades_in_linearly_and_reaches_target() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 1000),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(0);
    assert_eq!(dmx(&e, 1), 0);
    e.tick(500_000);
    assert!((126..=128).contains(&dmx(&e, 1)), "{}", dmx(&e, 1));
    e.tick(1_000_000);
    assert_eq!(dmx(&e, 1), 255);
    e.tick(2_000_000);
    assert_eq!(dmx(&e, 1), 255);
}

#[test]
fn scene_fades_out_on_stop() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 1000),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(1_000_000);
    ok(
        &mut e,
        1_000_000,
        Command::StopFunction { id: FunctionId(1) },
    );
    e.tick(1_500_000);
    assert!((126..=128).contains(&dmx(&e, 1)));
    e.tick(2_000_000);
    assert_eq!(dmx(&e, 1), 0);
    e.tick(2_100_000);
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn htp_between_scenes_and_ltp_programmer() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(0.5))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(2, vec![explicit(fixture(DIM1), intensity(0.2))], 0),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(2) });
    e.tick(10);
    assert_eq!(dmx(&e, 1), 128); // max(0.5, 0.2)

    // LTP: pan of the programmer beats scenes.
    ok(
        &mut e,
        10,
        Command::SetFunction {
            function: scene(
                3,
                vec![explicit(
                    fixture(SPOT1),
                    AttributeValue::Position {
                        pan: -270.0,
                        tilt: 0.0,
                    },
                )],
                0,
            ),
        },
    );
    ok(&mut e, 10, Command::StartFunction { id: FunctionId(3) });
    e.tick(20);
    assert_eq!(dmx(&e, 40), 0);
    ok(
        &mut e,
        20,
        Command::ProgrammerSet {
            target: fixture(SPOT1),
            value: AttributeValue::Position {
                pan: 270.0,
                tilt: 0.0,
            },
        },
    );
    e.tick(30);
    assert_eq!(dmx(&e, 40), 255);
    assert_eq!(dmx(&e, 41), 255);
    ok(&mut e, 30, Command::ProgrammerClear);
    e.tick(40);
    assert_eq!(dmx(&e, 40), 0);
}

#[test]
fn grand_master_and_blackout_affect_intensity_only() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(
                1,
                vec![
                    explicit(fixture(DIM1), intensity(1.0)),
                    explicit(fixture(SPOT1), intensity(1.0)),
                    explicit(
                        fixture(SPOT1),
                        AttributeValue::Position {
                            pan: 100.0,
                            tilt: 20.0,
                        },
                    ),
                    explicit(fixture(RGB), AttributeValue::Color { color: Color::RED }),
                ],
                0,
            ),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(0);
    let pan_before = dmx(&e, 40);
    assert_eq!(dmx(&e, 10), 255);
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.0 });
    e.tick(1);
    assert_eq!(dmx(&e, 1), 0);
    assert_eq!(dmx(&e, 48), 0);
    assert_eq!(dmx(&e, 10), 0, "virtual dimmer scaled by GM");
    assert_eq!(dmx(&e, 40), pan_before, "GM does not touch pan");
    ok(&mut e, 1, Command::SetGrandMaster { level: 1.0 });
    ok(&mut e, 1, Command::SetBlackout { on: true });
    e.tick(2);
    assert_eq!(dmx(&e, 1), 0);
    assert_eq!(dmx(&e, 10), 0);
    assert_eq!(dmx(&e, 40), pan_before);
    ok(&mut e, 2, Command::SetBlackout { on: false });
    e.tick(3);
    assert_eq!(dmx(&e, 1), 255);
}

#[test]
fn virtual_dimmer_scales_rgb() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(
                1,
                vec![
                    explicit(
                        fixture(RGB),
                        AttributeValue::Color {
                            color: Color::WHITE,
                        },
                    ),
                    explicit(fixture(RGB), intensity(0.5)),
                ],
                0,
            ),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(0);
    assert_eq!((dmx(&e, 10), dmx(&e, 11), dmx(&e, 12)), (128, 128, 128));
}

#[test]
fn rgbw_extracts_white_and_strobe_in_hz() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: fixture(RGBW),
            value: AttributeValue::Color {
                color: Color::new(1.0, 1.0, 0.5),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: fixture(RGBW),
            value: AttributeValue::Strobe {
                strobe: StrobeValue::Hz(13.0),
            },
        },
    );
    e.tick(0);
    // white = 0.5, red = green = 0.5, blue = 0
    assert_eq!(dmx(&e, 21), 128);
    assert_eq!(dmx(&e, 22), 128);
    assert_eq!(dmx(&e, 23), 0);
    assert_eq!(dmx(&e, 24), 128);
    // 13 Hz within 1..25 over 10..250 => 10 + 0.5*240 = 130
    assert_eq!(dmx(&e, 25), 130);
}

#[test]
fn color_wheel_gobo_prism_zoom_by_name_and_units() {
    let mut e = engine(test_show());
    let t = fixture(SPOT1);
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: t,
            value: AttributeValue::Color {
                color: Color::new(0.9, 0.05, 0.0),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: t,
            value: AttributeValue::Gobo {
                name: "stars".into(),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: t,
            value: AttributeValue::Prism {
                name: "3-facet".into(),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: t,
            value: AttributeValue::Zoom { degrees: 25.0 },
        },
    );
    e.tick(0);
    assert_eq!(dmx(&e, 44), 14, "red slot midpoint");
    assert_eq!(dmx(&e, 45), 11);
    assert_eq!(dmx(&e, 46), 191);
    assert_eq!(dmx(&e, 49), 128);
    // Unknown gobo: ignored with a warning, command still accepted.
    let out = ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: t,
            value: AttributeValue::Gobo {
                name: "nope".into(),
            },
        },
    );
    assert!(!out.warnings.is_empty());
}

#[test]
fn service_channels_are_rejected() {
    let mut e = engine(test_show());
    let v = AttributeValue::Raw {
        attribute: Attribute::Maintenance,
        value: 255,
    };
    assert!(matches!(
        err(
            &mut e,
            0,
            Command::ProgrammerSet {
                target: fixture(SPOT1),
                value: v.clone()
            }
        ),
        CommandError::InvalidValue { .. }
    ));
    assert!(matches!(
        err(
            &mut e,
            0,
            Command::SetFunction {
                function: scene(1, vec![explicit(fixture(SPOT1), v)], 0)
            }
        ),
        CommandError::InvalidValue { .. }
    ));
}

#[test]
fn orientation_inverts_pan() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: group(G_SPOTS),
            value: AttributeValue::Position {
                pan: 270.0,
                tilt: 0.0,
            },
        },
    );
    e.tick(0);
    assert_eq!(dmx(&e, 40), 255);
    assert_eq!(dmx(&e, 60), 0, "spot 2 is mounted with inverted pan");
}

#[test]
fn palettes_are_references() {
    let mut e = engine(test_show());
    let pal = |r: f32| Palette {
        id: PaletteId(1),
        name: "Red".into(),
        kind: PaletteKind::Color,
        generic: vec![AttributeValue::Color {
            color: Color::new(r, 0.0, 0.0),
        }],
        per_fixture: vec![],
    };
    ok(&mut e, 0, Command::SetPalette { palette: pal(1.0) });
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(1),
                folder: None,
                name: "s".into(),
                kind: FunctionKind::Scene(Scene {
                    values: vec![SceneValue {
                        target: fixture(RGB),
                        value: ValueSource::Palette { id: PaletteId(1) },
                    }],
                    ..Scene::default()
                }),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(0);
    assert_eq!(dmx(&e, 10), 255);
    ok(&mut e, 0, Command::SetPalette { palette: pal(0.5) });
    e.tick(1);
    assert_eq!(
        dmx(&e, 10),
        128,
        "editing the palette updates the running scene"
    );
}

#[test]
fn chaser_at_120_bpm_changes_step_every_500ms() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(2, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(10),
                folder: None,
                name: "chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: vec![
                        ChaserStep {
                            scene: FunctionId(1),
                            hold: None,
                            fade: None,
                        },
                        ChaserStep {
                            scene: FunctionId(2),
                            hold: None,
                            fade: None,
                        },
                    ],
                    hold: TimeValue::Beats(1.0),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(10) });
    let mut changes = Vec::new();
    let mut last = None;
    for t in (0..=3_000_000u64).step_by(10_000) {
        e.tick(t);
        let step = (dmx(&e, 1), dmx(&e, 2));
        if last.is_some_and(|l| l != step) {
            changes.push(t);
        }
        last = Some(step);
        // The published state names the function the current step plays.
        let playing = if step.0 > 0 { 1 } else { 2 };
        let snapshot = e.shared().snapshot.load_full();
        assert_eq!(
            snapshot.state.active.first().map(|a| a.playing.clone()),
            Some(vec![FunctionId(playing)]),
            "at {t} µs"
        );
    }
    assert_eq!(
        changes,
        vec![500_000, 1_000_000, 1_500_000, 2_000_000, 2_500_000, 3_000_000]
    );
}

#[test]
fn chaser_follows_bpm_change_without_phase_jump() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(2, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(10),
                folder: None,
                name: "chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: vec![
                        ChaserStep {
                            scene: FunctionId(1),
                            hold: None,
                            fade: None,
                        },
                        ChaserStep {
                            scene: FunctionId(2),
                            hold: None,
                            fade: None,
                        },
                    ],
                    hold: TimeValue::Beats(1.0),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(10) });
    e.tick(250_000); // half a beat
    ok(&mut e, 250_000, Command::SetBpm { bpm: 60.0 });
    // Remaining half beat at 60 BPM lasts 500 ms: step changes at 750 ms.
    e.tick(740_000);
    assert_eq!(dmx(&e, 1), 255);
    e.tick(750_000);
    assert_eq!(dmx(&e, 2), 255);
    assert_eq!(dmx(&e, 1), 0);
}

/// Regression: leaving a step timed in milliseconds did not move the beat
/// origin, so a following step timed in beats was over as soon as it started.
#[test]
fn chaser_beat_step_after_a_millisecond_step_lasts_its_beats() {
    let mut e = engine(test_show());
    for (id, dimmer) in [(1, DIM1), (2, DIM2)] {
        ok(
            &mut e,
            0,
            Command::SetFunction {
                function: scene(id, vec![explicit(fixture(dimmer), intensity(1.0))], 0),
            },
        );
    }
    let step = |scene, hold| ChaserStep {
        scene: FunctionId(scene),
        hold: Some(hold),
        fade: None,
    };
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(10),
                folder: None,
                name: "mixed".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: vec![step(1, TimeValue::Ms(2000)), step(2, TimeValue::Beats(2.0))],
                    hold: TimeValue::Beats(1.0),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(10) });
    e.tick(2_000_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 255));
    // Two beats at 120 BPM last one second.
    e.tick(2_990_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 255));
    e.tick(3_000_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
}

/// Regression: shortening the step of a running chaser made it race through
/// one step per tick until it had "caught up" with the time already gone by.
#[test]
fn shortening_the_step_of_a_running_chaser_advances_once() {
    let mut e = engine(test_show());
    for (id, dimmer) in [(1, DIM1), (2, DIM2)] {
        ok(
            &mut e,
            0,
            Command::SetFunction {
                function: scene(id, vec![explicit(fixture(dimmer), intensity(1.0))], 0),
            },
        );
    }
    let chaser = |hold| Function {
        id: FunctionId(10),
        folder: None,
        name: "chase".into(),
        kind: FunctionKind::Chaser(Chaser {
            steps: [1, 2]
                .map(|scene| ChaserStep {
                    scene: FunctionId(scene),
                    hold: None,
                    fade: None,
                })
                .to_vec(),
            hold: TimeValue::Beats(hold),
            fade: TimeValue::ZERO,
            direction: Direction::Forward,
            run_mode: RunMode::Loop,
        }),
    };
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: chaser(8.0),
        },
    );
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(10) });
    // 7.2 beats into the first step, it becomes one beat long.
    e.tick(3_600_000);
    ok(
        &mut e,
        3_600_000,
        Command::SetFunction {
            function: chaser(1.0),
        },
    );
    let mut steps = Vec::new();
    for t in (3_600_000..=4_490_000u64).step_by(10_000) {
        e.tick(t);
        let step = (dmx(&e, 1), dmx(&e, 2));
        if steps.last().is_none_or(|(_, s)| *s != step) {
            steps.push((t, step));
        }
    }
    // The overdue step ends at once; the next ones stay on the beat grid.
    assert_eq!(steps, vec![(3_600_000, (0, 255)), (4_000_000, (255, 0))]);
}

fn cue(number: f32, values: Vec<SceneValue>, fade_ms: u32) -> Cue {
    Cue {
        number,
        name: format!("cue {number}"),
        values,
        timing: Timing {
            delay: TimeValue::ZERO,
            fade_in: TimeValue::Ms(fade_ms),
            fade_out: TimeValue::Ms(fade_ms),
        },
        follow: CueFollow::Manual,
        move_in_black: None,
    }
}

#[test]
fn cue_list_go_back_and_crossfade() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(20),
                folder: None,
                name: "cues".into(),
                kind: FunctionKind::CueList(CueList {
                    cues: vec![
                        cue(1.0, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
                        cue(2.0, vec![explicit(fixture(DIM2), intensity(1.0))], 1000),
                    ],
                    move_in_black: false,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::CueGo { id: FunctionId(20) });
    e.tick(0);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
    ok(&mut e, 0, Command::CueGo { id: FunctionId(20) });
    e.tick(500_000);
    let (a, b) = (dmx(&e, 1), dmx(&e, 2));
    assert!(
        (126..=128).contains(&a) && (126..=128).contains(&b),
        "{a} {b}"
    );
    e.tick(1_000_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 255));
    assert_eq!(e.snapshot().state.active[0].position, Some(1));
    ok(&mut e, 1_000_000, Command::CueBack { id: FunctionId(20) });
    e.tick(1_000_001);
    assert_eq!(dmx(&e, 1), 255);
}

#[test]
fn cue_list_pause_freezes_fade() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(20),
                folder: None,
                name: "cues".into(),
                kind: FunctionKind::CueList(CueList {
                    cues: vec![cue(
                        1.0,
                        vec![explicit(fixture(DIM1), intensity(1.0))],
                        1000,
                    )],
                    move_in_black: false,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::CueGo { id: FunctionId(20) });
    e.tick(500_000);
    let mid = dmx(&e, 1);
    ok(&mut e, 500_000, Command::CuePause { id: FunctionId(20) });
    e.tick(5_000_000);
    assert_eq!(dmx(&e, 1), mid);
    ok(&mut e, 5_000_000, Command::CuePause { id: FunctionId(20) });
    e.tick(5_500_000);
    assert_eq!(dmx(&e, 1), 255);
}

#[test]
fn move_in_black_presets_dark_heads() {
    let mut e = engine(test_show());
    let pos = AttributeValue::Position {
        pan: 270.0,
        tilt: 0.0,
    };
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(20),
                folder: None,
                name: "cues".into(),
                kind: FunctionKind::CueList(CueList {
                    cues: vec![
                        cue(1.0, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
                        cue(
                            2.0,
                            vec![
                                explicit(fixture(SPOT1), intensity(1.0)),
                                explicit(fixture(SPOT1), pos),
                            ],
                            0,
                        ),
                    ],
                    move_in_black: true,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::CueGo { id: FunctionId(20) });
    e.tick(0);
    assert_eq!(dmx(&e, 48), 0, "spot is dark in cue 1");
    assert_eq!(dmx(&e, 40), 255, "but already in position for cue 2");
}

#[test]
fn movement_effect_moves_heads_with_phase() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(30),
                folder: None,
                name: "circle".into(),
                kind: FunctionKind::MovementEffect(MovementEffect {
                    group: G_SPOTS,
                    shape: MovementShape::Circle,
                    center_pan: 0.0,
                    center_tilt: 0.0,
                    width: 90.0,
                    height: 90.0,
                    period: TimeValue::Ms(1000),
                    phase_spread: 180.0,
                    direction: Direction::Forward,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(30) });
    e.tick(0);
    let a = dmx(&e, 40);
    e.tick(250_000);
    let b = dmx(&e, 40);
    assert_ne!(a, b);
}

/// A chaser stepping through a scene (dimmer 1), a movement effect and a
/// matrix effect, one second each.
fn chaser_with_effect_steps() -> klight_engine_core::Engine {
    let mut e = engine(test_show());
    let function = |id, kind| Function {
        id: FunctionId(id),
        folder: None,
        name: format!("f{id}"),
        kind,
    };
    let movement = FunctionKind::MovementEffect(MovementEffect {
        group: G_SPOTS,
        shape: MovementShape::Circle,
        center_pan: 0.0,
        center_tilt: 0.0,
        width: 90.0,
        height: 90.0,
        period: TimeValue::Beats(2.0),
        phase_spread: 180.0,
        direction: Direction::Forward,
    });
    let matrix = FunctionKind::MatrixEffect(MatrixEffect {
        group: G_PIXELS,
        algorithm: MatrixAlgorithm::Chase {
            color: Color::RED,
            background: Color::BLACK,
        },
        period: TimeValue::Ms(400),
        direction: Direction::Forward,
    });
    let chaser = FunctionKind::Chaser(Chaser {
        steps: [1, 30, 40]
            .map(|id| ChaserStep {
                scene: FunctionId(id),
                hold: None,
                fade: None,
            })
            .to_vec(),
        hold: TimeValue::Ms(1000),
        fade: TimeValue::ZERO,
        direction: Direction::Forward,
        run_mode: RunMode::Loop,
    });
    for f in [
        scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
        function(30, movement),
        function(40, matrix),
        function(10, chaser),
    ] {
        ok(&mut e, 0, Command::SetFunction { function: f });
    }
    ok(&mut e, 0, Command::SetBpm { bpm: 120.0 });
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(10) });
    e.tick(0);
    assert!(e.snapshot().warnings.is_empty());
    e
}

fn matrix_reds(e: &klight_engine_core::Engine) -> Vec<u8> {
    (0..4).map(|h| dmx(e, 100 + h * 3)).collect()
}

#[test]
fn chaser_plays_movement_and_matrix_steps() {
    let mut e = chaser_with_effect_steps();
    let still = dmx(&e, 40);
    assert_eq!((dmx(&e, 1), matrix_reds(&e)), (255, vec![0; 4]));

    // Movement step: the head starts its cycle with the step.
    e.tick(1_000_000);
    assert_eq!(e.snapshot().state.active[0].playing, vec![FunctionId(30)]);
    assert_eq!(dmx(&e, 1), 0);
    let start = dmx(&e, 40);
    assert_ne!(start, still);
    e.tick(1_250_000);
    assert_ne!(dmx(&e, 40), start);

    // Matrix step: from its first column; the movement is over.
    e.tick(2_000_000);
    assert_eq!(dmx(&e, 40), still);
    assert_eq!(matrix_reds(&e), vec![255, 0, 0, 0]);
    e.tick(2_100_000);
    assert_eq!(matrix_reds(&e), vec![0, 255, 0, 0]);

    // Back to the scene, then the movement again from its start.
    e.tick(3_000_000);
    assert_eq!((dmx(&e, 1), matrix_reds(&e)), (255, vec![0; 4]));
    e.tick(4_000_000);
    assert_eq!(dmx(&e, 40), start);
}

#[test]
fn stopping_a_chaser_stops_its_effect_step() {
    let mut e = chaser_with_effect_steps();
    e.tick(1_000_000);
    e.tick(2_000_000);
    assert_eq!(matrix_reds(&e), vec![255, 0, 0, 0]);
    ok(
        &mut e,
        2_000_000,
        Command::StopFunction { id: FunctionId(10) },
    );
    e.tick(2_000_000);
    assert_eq!(matrix_reds(&e), vec![0; 4]);
    assert!(e.snapshot().state.active.is_empty());
}

#[test]
fn restored_chaser_resumes_its_effect_step() {
    let mut e = chaser_with_effect_steps();
    e.tick(1_000_000);
    e.tick(2_000_000);
    let state = e.runtime_state();
    let show = (**e.show()).clone();
    let mut restored = engine(show);
    assert!(restored.restore(0, &state).is_empty());
    restored.tick(0);
    assert_eq!(
        (dmx(&restored, 1), matrix_reds(&restored)),
        (0, vec![255, 0, 0, 0])
    );
}

#[test]
fn editing_an_effect_changes_the_running_chaser_step_at_once() {
    let mut e = chaser_with_effect_steps();
    e.tick(1_000_000);
    e.tick(2_000_000);
    assert_eq!(matrix_reds(&e), vec![255, 0, 0, 0]);
    // Edited from the virtual console while its step plays: a cycle half as
    // long reaches the third column where the old one reached the second.
    let Some(mut matrix) = e.show().function(FunctionId(40)).cloned() else {
        panic!("matrix missing");
    };
    if let FunctionKind::MatrixEffect(m) = &mut matrix.kind {
        m.period = TimeValue::Ms(200);
    }
    ok(&mut e, 2_000_000, Command::SetFunction { function: matrix });
    e.tick(2_100_000);
    assert_eq!(matrix_reds(&e), vec![0, 0, 255, 0]);
    assert_eq!(e.snapshot().state.active[0].playing, vec![FunctionId(40)]);
}

#[test]
fn chasers_and_cue_lists_cannot_be_chaser_steps() {
    let mut e = chaser_with_effect_steps();
    let cue_list = Function {
        id: FunctionId(20),
        folder: None,
        name: "cues".into(),
        kind: FunctionKind::CueList(CueList {
            cues: vec![cue(1.0, vec![explicit(fixture(DIM2), intensity(1.0))], 0)],
            move_in_black: false,
        }),
    };
    ok(&mut e, 0, Command::SetFunction { function: cue_list });
    let Some(mut other) = e.show().function(FunctionId(10)).cloned() else {
        panic!("chaser missing");
    };
    other.id = FunctionId(11);
    ok(&mut e, 0, Command::SetFunction { function: other });
    let Some(mut chaser) = e.show().function(FunctionId(10)).cloned() else {
        panic!("chaser missing");
    };
    if let FunctionKind::Chaser(c) = &mut chaser.kind {
        // Another chaser, a cue list, then the scene.
        c.steps = [11, 20, 1]
            .map(|id| ChaserStep {
                scene: FunctionId(id),
                hold: None,
                fade: None,
            })
            .to_vec();
    }
    ok(&mut e, 0, Command::SetFunction { function: chaser });
    e.tick(0);
    let warnings = e.snapshot().warnings;
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().all(|w| w.contains("a chaser cannot play")));
    // Only the scene is left: the chaser plays it and nothing else.
    assert_eq!(e.snapshot().state.active[0].playing, vec![FunctionId(1)]);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
    e.tick(1_000_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
}

/// Regression: every edit reported all the warnings of the show, so one left
/// standing popped up again (covering the screen) at each unrelated change.
#[test]
fn edits_report_only_the_warnings_they_bring() {
    let mut e = chaser_with_effect_steps();
    let Some(mut chaser) = e.show().function(FunctionId(10)).cloned() else {
        panic!("chaser missing");
    };
    if let FunctionKind::Chaser(c) = &mut chaser.kind {
        c.steps.push(ChaserStep {
            scene: FunctionId(99),
            hold: None,
            fade: None,
        });
    }
    let out = ok(
        &mut e,
        0,
        Command::SetFunction {
            function: chaser.clone(),
        },
    );
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    // An unrelated edit: the warning stands, but it is not news.
    let out = ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(2, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
        },
    );
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(e.snapshot().warnings.len(), 1);
}

/// Regression: the position of a chaser counted only its valid steps, so
/// after an invalid one the screens highlighted the row before the playing one.
#[test]
fn chaser_position_counts_invalid_steps_too() {
    let mut e = chaser_with_effect_steps();
    let Some(mut chaser) = e.show().function(FunctionId(10)).cloned() else {
        panic!("chaser missing");
    };
    if let FunctionKind::Chaser(c) = &mut chaser.kind {
        // A deleted function, then the scene and the movement.
        c.steps = [99, 1, 30]
            .map(|id| ChaserStep {
                scene: FunctionId(id),
                hold: None,
                fade: None,
            })
            .to_vec();
    }
    ok(&mut e, 0, Command::SetFunction { function: chaser });
    e.tick(0);
    let at = |e: &klight_engine_core::Engine| {
        e.snapshot()
            .state
            .active
            .first()
            .map(|a| (a.position, a.playing.clone()))
    };
    assert_eq!(at(&e), Some((Some(1), vec![FunctionId(1)])));
    e.tick(1_000_000);
    assert_eq!(at(&e), Some((Some(2), vec![FunctionId(30)])));

    // Restored from that position, it plays the same step.
    let state = e.runtime_state();
    let show = (**e.show()).clone();
    let mut restored = engine(show);
    restored.restore(1_000_000, &state);
    restored.tick(1_000_000);
    assert_eq!(at(&restored), Some((Some(2), vec![FunctionId(30)])));
}

#[test]
fn matrix_chase_lights_one_column() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(40),
                folder: None,
                name: "chase".into(),
                kind: FunctionKind::MatrixEffect(MatrixEffect {
                    group: G_PIXELS,
                    algorithm: MatrixAlgorithm::Chase {
                        color: Color::RED,
                        background: Color::BLACK,
                    },
                    period: TimeValue::Ms(400),
                    direction: Direction::Forward,
                }),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(40) });
    e.tick(0);
    let reds: Vec<u8> = (0..4).map(|h| dmx(&e, 100 + h * 3)).collect();
    assert_eq!(reds, vec![255, 0, 0, 0]);
    e.tick(100_000);
    let reds: Vec<u8> = (0..4).map(|h| dmx(&e, 100 + h * 3)).collect();
    assert_eq!(reds, vec![0, 255, 0, 0]);
}

#[test]
fn patch_conflicts_are_rejected_unless_forced() {
    let mut e = engine(test_show());
    let mut f = e.show().fixtures[0].clone();
    f.id = FixtureId(99);
    let c = err(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: f.clone(),
            force: false,
        },
    );
    assert!(matches!(c, CommandError::AddressConflicts { .. }), "{c:?}");
    let out = ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: f,
            force: true,
        },
    );
    assert!(!out.warnings.is_empty());
    f_out_of_range(&mut e);
}

fn f_out_of_range(e: &mut klight_engine_core::Engine) {
    let mut f = e.show().fixtures[4].clone();
    f.id = FixtureId(100);
    f.address = 510;
    assert!(matches!(
        err(
            e,
            0,
            Command::PatchFixture {
                fixture: f,
                force: true
            }
        ),
        CommandError::Patch { .. }
    ));
}

#[test]
fn undo_redo_and_show_mode() {
    let mut e = engine(test_show());
    ok(&mut e, 0, Command::SetShowName { name: "A".into() });
    ok(&mut e, 0, Command::SetShowName { name: "B".into() });
    ok(&mut e, 0, Command::Undo);
    assert_eq!(e.show().name, "A");
    ok(&mut e, 0, Command::Redo);
    assert_eq!(e.show().name, "B");
    ok(
        &mut e,
        0,
        Command::SetMode {
            mode: OperatingMode::Show,
            pin: None,
        },
    );
    assert_eq!(
        err(&mut e, 0, Command::SetShowName { name: "C".into() }),
        CommandError::ShowModeLocked
    );
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.5 }); // playback still allowed
}

#[test]
fn remote_design_mode_needs_pin() {
    let mut show = test_show();
    show.settings.design_pin = Some("1234".into());
    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::SetMode {
            mode: OperatingMode::Show,
            pin: None,
        },
    );
    let remote = |pin: Option<&str>| CommandEnvelope {
        id: 1,
        source: CommandSource::Remote {
            client: ClientId(7),
        },
        command: Command::SetMode {
            mode: OperatingMode::Design,
            pin: pin.map(str::to_owned),
        },
    };
    assert_eq!(
        e.apply(0, remote(Some("0000"))).result.unwrap_err(),
        CommandError::BadPin
    );
    assert!(e.apply(0, remote(Some("1234"))).result.is_ok());
}

#[test]
fn operator_overrides_automation_until_release() {
    let mut e = engine(test_show());
    let auto = |level| CommandEnvelope {
        id: 1,
        source: CommandSource::Automation {
            name: "sync".into(),
        },
        command: Command::SetGrandMaster { level },
    };
    assert!(e.apply(0, auto(0.3)).result.is_ok());
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.8 });
    assert_eq!(
        e.apply(0, auto(0.1)).result.unwrap_err(),
        CommandError::Overridden
    );
    ok(
        &mut e,
        0,
        Command::ReleaseControl {
            control: ControlKey::GrandMaster,
        },
    );
    assert!(e.apply(0, auto(0.1)).result.is_ok());
}

#[test]
fn invalid_commands_are_typed_errors() {
    let mut e = engine(test_show());
    assert!(matches!(
        err(
            &mut e,
            0,
            Command::StartFunction {
                id: FunctionId(999)
            }
        ),
        CommandError::NotFound { .. }
    ));
    assert!(matches!(
        err(&mut e, 0, Command::SetGrandMaster { level: f32::NAN }),
        CommandError::InvalidValue { .. }
    ));
    assert!(matches!(
        err(&mut e, 0, Command::SetBpm { bpm: -1.0 }),
        CommandError::InvalidValue { .. }
    ));
    assert_eq!(err(&mut e, 0, Command::Undo), CommandError::NothingToUndo);
}

#[test]
fn solo_frame_keeps_one_function_active() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(2, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
        },
    );
    let button = |id, f| Widget {
        id: WidgetId(id),
        kind: WidgetKind::Button {
            action: ButtonAction::Toggle,
        },
        props: WidgetProps::default(),
        function: Some(FunctionId(f)),
        children: vec![],
    };
    let vc = VirtualConsole {
        widgets: vec![Widget {
            id: WidgetId(1),
            kind: WidgetKind::SoloFrame,
            props: WidgetProps::default(),
            function: None,
            children: vec![button(2, 1), button(3, 2)],
        }],
    };
    ok(&mut e, 0, Command::SetVirtualConsole { console: vc });
    ok(&mut e, 0, Command::WidgetPress { id: WidgetId(2) });
    e.tick(0);
    ok(&mut e, 0, Command::WidgetPress { id: WidgetId(3) });
    e.tick(1);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (0, 255));
    let active: Vec<_> = e.snapshot().state.active.iter().map(|a| a.id).collect();
    assert_eq!(active, vec![FunctionId(2)]);
}

#[test]
fn restore_runtime_state_skips_fades() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 5000),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.5 });
    e.tick(10_000_000);
    let state = e.snapshot().state;
    let show = (**e.show()).clone();

    let mut e2 = engine(show);
    let w = e2.restore(0, &state);
    assert!(w.is_empty(), "{w:?}");
    e2.tick(0);
    assert_eq!(dmx(&e2, 1), 128);
}

#[test]
fn spread_delays_heads_in_group_order() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(1),
                folder: None,
                name: "spread".into(),
                kind: FunctionKind::Scene(Scene {
                    values: vec![explicit(group(G_DIMMERS), intensity(1.0))],
                    timing: Timing::default(),
                    spread: Spread::Linear {
                        step: TimeValue::Ms(100),
                    },
                    attribute_timing: vec![],
                    ..Scene::default()
                }),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(50_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
    e.tick(100_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 255));
}

#[test]
fn store_programmer_as_scene_and_palette() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: group(G_SPOTS),
            value: AttributeValue::Position {
                pan: 10.0,
                tilt: 5.0,
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::StoreProgrammerAsScene {
            id: None,
            name: "pos".into(),
        },
    );
    ok(
        &mut e,
        0,
        Command::StoreProgrammerInPalette {
            id: None,
            name: "Drums".into(),
            kind: PaletteKind::Position,
        },
    );
    let show = e.show().clone();
    assert_eq!(show.functions.len(), 1);
    assert_eq!(show.palettes[0].per_fixture.len(), 2);
}

#[test]
fn determinism_same_inputs_same_frames() {
    let run = || {
        let mut e = engine(test_show());
        ok(
            &mut e,
            0,
            Command::SetFunction {
                function: scene(
                    1,
                    vec![
                        explicit(group(G_DIMMERS), intensity(0.7)),
                        explicit(
                            fixture(RGB),
                            AttributeValue::Color {
                                color: Color::new(0.2, 0.4, 0.9),
                            },
                        ),
                    ],
                    777,
                ),
            },
        );
        ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
        let mut frames = Vec::new();
        for t in (0..2_000_000u64).step_by(22_727) {
            e.tick(t);
            frames.push(*e.frame(U1).unwrap());
        }
        frames
    };
    assert_eq!(run(), run());
}

#[test]
fn restore_publishes_snapshot_immediately() {
    // Regression: the autosaver could persist the pre-restore snapshot.
    let mut e = engine(test_show());
    let state = RuntimeState {
        grand_master: 0.3,
        ..RuntimeState::default()
    };
    e.restore(0, &state);
    let published = e.shared().snapshot.load();
    assert!((published.state.grand_master - 0.3).abs() < 1e-6);
}

#[test]
fn simple_desk_raw_channels_override_and_release() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(
                1,
                vec![explicit(
                    fixture(SPOT1),
                    AttributeValue::Position {
                        pan: -270.0,
                        tilt: 0.0,
                    },
                )],
                0,
            ),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    // Unpatched address, patched non-intensity channel (pan), intensity channel.
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 300,
            value: Some(42),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 40,
            value: Some(200),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 1,
            value: Some(255),
        },
    );
    e.tick(0);
    assert_eq!((dmx(&e, 300), dmx(&e, 40), dmx(&e, 1)), (42, 200, 255));
    assert_eq!(e.snapshot().state.channels.len(), 3);

    // Intensity channels still obey grand master and blackout; pan does not.
    ok(&mut e, 0, Command::SetGrandMaster { level: 0.5 });
    e.tick(1);
    assert_eq!((dmx(&e, 1), dmx(&e, 40), dmx(&e, 300)), (128, 200, 42));
    ok(&mut e, 1, Command::SetBlackout { on: true });
    e.tick(2);
    assert_eq!((dmx(&e, 1), dmx(&e, 40)), (0, 200));
    ok(&mut e, 2, Command::SetBlackout { on: false });

    // Releasing a channel gives it back to the scene.
    ok(
        &mut e,
        2,
        Command::SetChannel {
            universe: U1,
            address: 40,
            value: None,
        },
    );
    e.tick(3);
    assert_eq!(dmx(&e, 40), 0);
    ok(&mut e, 3, Command::ClearChannels { universe: Some(U1) });
    e.tick(4);
    assert_eq!((dmx(&e, 300), dmx(&e, 1)), (0, 0));

    assert!(matches!(
        err(
            &mut e,
            4,
            Command::SetChannel {
                universe: U1,
                address: 0,
                value: Some(1)
            }
        ),
        CommandError::InvalidValue { .. }
    ));
    assert!(matches!(
        err(
            &mut e,
            4,
            Command::SetChannel {
                universe: UniverseId(9),
                address: 1,
                value: Some(1)
            }
        ),
        CommandError::NotFound { .. }
    ));
}

#[test]
fn raw_channels_survive_edits_and_restore() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 300,
            value: Some(7),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetShowName {
            name: "edited".into(),
        },
    );
    e.tick(0);
    assert_eq!(dmx(&e, 300), 7, "kept across recompile");
    let state = e.snapshot().state;
    let mut e2 = engine(test_show());
    assert!(e2.restore(0, &state).is_empty());
    e2.tick(0);
    assert_eq!(dmx(&e2, 300), 7);
    ok(&mut e2, 0, Command::ProgrammerClear);
    e2.tick(1);
    assert_eq!(dmx(&e2, 300), 0);
}

#[test]
fn removing_a_universe_removes_its_fixtures() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetPalette {
            palette: Palette {
                id: PaletteId(1),
                name: "p".into(),
                kind: PaletteKind::Intensity,
                generic: vec![],
                per_fixture: vec![klight_model::palette::FixturePaletteValues {
                    fixture: DIM1,
                    values: vec![intensity(1.0)],
                }],
            },
        },
    );
    let out = ok(&mut e, 0, Command::RemoveUniverse { id: U1 });
    assert!(
        out.warnings.iter().any(|w| w.contains("7 fixtures")),
        "{:?}",
        out.warnings
    );
    let show = e.show().clone();
    assert!(show.universes.is_empty());
    assert!(show.fixtures.is_empty());
    assert!(show.groups.iter().all(|g| g.members.is_empty()));
    assert!(show.palettes[0].per_fixture.is_empty());
    // Undo brings everything back.
    ok(&mut e, 0, Command::Undo);
    assert_eq!(e.show().fixtures.len(), 7);
}

#[test]
fn rename_universe() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::RenameUniverse {
            id: U1,
            name: " Stage left ".into(),
        },
    );
    assert_eq!(e.show().universes[0].name, "Stage left");
    assert!(matches!(
        err(
            &mut e,
            0,
            Command::RenameUniverse {
                id: U1,
                name: " ".into()
            }
        ),
        CommandError::InvalidValue { .. }
    ));
}

fn pan16(e: &klight_engine_core::Engine, address: usize) -> u16 {
    u16::from_be_bytes([dmx(e, address), dmx(e, address + 1)])
}

fn fan(pan_spread: f32, mode: FanMode) -> AttributeValue {
    AttributeValue::PositionFan {
        pan: 0.0,
        tilt: 0.0,
        pan_spread,
        tilt_spread: 0.0,
        mode,
    }
}

#[test]
fn fan_is_resolved_by_the_core_in_group_order() {
    let mut show = test_show();
    show.fixtures[5].orientation.invert_pan = false;
    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: group(G_SPOTS),
            value: fan(60.0, FanMode::Linear),
        },
    );
    e.tick(0);
    // 540° range: -30° and +30° around the center.
    let expect = |deg: f32| klight_fixtures::convert::degrees_to_dmx16(deg, 540.0);
    assert_eq!(
        (pan16(&e, 40), pan16(&e, 60)),
        (expect(-30.0), expect(30.0))
    );

    // A plain position on the same target replaces the fan (same slot).
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: group(G_SPOTS),
            value: AttributeValue::Position {
                pan: 10.0,
                tilt: 0.0,
            },
        },
    );
    assert_eq!(e.snapshot().state.programmer.len(), 1);
    e.tick(1);
    assert_eq!((pan16(&e, 40), pan16(&e, 60)), (expect(10.0), expect(10.0)));
}

#[test]
fn fan_respreads_when_the_group_changes() {
    let mut show = test_show();
    show.fixtures[5].orientation.invert_pan = false;
    // Group with only spot 1: no spread.
    show.groups[1].members.truncate(1);
    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(
                1,
                vec![explicit(group(G_SPOTS), fan(90.0, FanMode::FromCenter))],
                0,
            ),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(0);
    let expect = |deg: f32| klight_fixtures::convert::degrees_to_dmx16(deg, 540.0);
    assert_eq!(pan16(&e, 40), expect(0.0));
    // Add spot 2 to the group: the running scene spreads over both heads.
    let mut g = e.show().groups[1].clone();
    g.members.push(GroupMember {
        fixture: SPOT2,
        head: 0,
        grid: None,
    });
    ok(&mut e, 0, Command::SetGroup { group: g });
    e.tick(1);
    // From the center with 2 heads: both at the ends (+45°).
    assert_eq!((pan16(&e, 40), pan16(&e, 60)), (expect(45.0), expect(45.0)));
}

#[test]
fn fan_stored_in_palette_per_fixture() {
    let mut show = test_show();
    show.fixtures[5].orientation.invert_pan = false;
    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: group(G_SPOTS),
            value: fan(60.0, FanMode::Linear),
        },
    );
    ok(
        &mut e,
        0,
        Command::StoreProgrammerInPalette {
            id: None,
            name: "Fan".into(),
            kind: PaletteKind::Position,
        },
    );
    let p = e.show().palettes[0].clone();
    assert_eq!(
        p.per_fixture[0].values,
        vec![AttributeValue::Position {
            pan: -30.0,
            tilt: 0.0
        }]
    );
    assert_eq!(
        p.per_fixture[1].values,
        vec![AttributeValue::Position {
            pan: 30.0,
            tilt: 0.0
        }]
    );
}

fn new_fixture(template: &PatchedFixture, id: u32, name: &str, address: u16) -> PatchedFixture {
    let mut f = template.clone();
    f.id = FixtureId(id);
    f.name = name.into();
    f.address = address;
    f
}

#[test]
fn new_fixtures_get_unique_names() {
    let mut e = engine(test_show());
    let tpl = e.show().fixtures[0].clone(); // 1-channel dimmer
                                            // The base name is used while free, then numbered from 1.
    let out = ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 50, "Par", 300),
            force: false,
        },
    );
    assert!(out.warnings.is_empty());
    let out = ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 51, "Par", 301),
            force: false,
        },
    );
    assert!(
        out.warnings.iter().any(|w| w.contains("Par 1")),
        "{:?}",
        out.warnings
    );
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 52, "Par", 302),
            force: false,
        },
    );
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 53, "Par", 303),
            force: false,
        },
    );
    let names: Vec<String> = e
        .show()
        .fixtures
        .iter()
        .filter(|f| f.id.0 >= 50)
        .map(|f| f.name.clone())
        .collect();
    assert_eq!(names, vec!["Par", "Par 1", "Par 2", "Par 3"]);

    // A numbered name that is taken continues from its number ("Dim 1" and "Dim 2" exist).
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 54, "Dim 1", 304),
            force: false,
        },
    );
    assert_eq!(e.show().fixture(FixtureId(54)).unwrap().name, "Dim 3");

    // Re-patching a fixture with its own name is not a collision.
    let f = e.show().fixture(FixtureId(51)).unwrap().clone();
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: PatchedFixture { address: 310, ..f },
            force: false,
        },
    );
    assert_eq!(e.show().fixture(FixtureId(51)).unwrap().name, "Par 1");

    // Empty names fall back to the model.
    ok(
        &mut e,
        0,
        Command::PatchFixture {
            fixture: new_fixture(&tpl, 55, "  ", 305),
            force: false,
        },
    );
    assert_eq!(e.show().fixture(FixtureId(55)).unwrap().name, "Dimmer");
}

fn folder(id: u32, category: FunctionCategory, name: &str, parent: Option<u32>) -> FunctionFolder {
    FunctionFolder {
        id: FolderId(id),
        category,
        name: name.into(),
        parent: parent.map(FolderId),
    }
}

#[test]
fn function_folders_nest_and_move_contents_up_when_removed() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(1, FunctionCategory::Scene, "Act 1", None),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(2, FunctionCategory::Scene, "Song A", Some(1)),
        },
    );
    let mut s = scene(10, vec![explicit(fixture(DIM1), intensity(1.0))], 0);
    s.folder = Some(FolderId(2));
    ok(&mut e, 0, Command::SetFunction { function: s });

    // Rename and move keep the id.
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(2, FunctionCategory::Scene, "Song B", Some(1)),
        },
    );
    assert_eq!(e.show().folder(FolderId(2)).unwrap().name, "Song B");

    // Removing "Act 1": "Song B" goes to the top level; removing it moves the scene up too.
    ok(&mut e, 0, Command::RemoveFunctionFolder { id: FolderId(1) });
    assert_eq!(e.show().folder(FolderId(2)).unwrap().parent, None);
    ok(&mut e, 0, Command::RemoveFunctionFolder { id: FolderId(2) });
    assert_eq!(e.show().function(FunctionId(10)).unwrap().folder, None);
    assert!(e.show().function_folders.is_empty());
}

#[test]
fn function_folders_are_validated() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(1, FunctionCategory::Scene, "Looks", None),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(2, FunctionCategory::Scene, "Sub", Some(1)),
        },
    );
    let invalid = |e: &mut klight_engine_core::Engine, c| {
        matches!(
            err(e, 0, c),
            CommandError::InvalidValue { .. } | CommandError::NotFound { .. }
        )
    };
    // Empty name, parent of another type, unknown parent, cycles.
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(3, FunctionCategory::Scene, " ", None)
        }
    ));
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(3, FunctionCategory::Chaser, "C", Some(1))
        }
    ));
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(3, FunctionCategory::Scene, "X", Some(99))
        }
    ));
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(1, FunctionCategory::Scene, "Looks", Some(2))
        }
    ));
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(1, FunctionCategory::Scene, "Looks", Some(1))
        }
    ));
    // Changing the type of a folder that has content is refused.
    assert!(invalid(
        &mut e,
        Command::SetFunctionFolder {
            folder: folder(1, FunctionCategory::Chaser, "Looks", None)
        }
    ));
    // A function can only go in a folder of its own type.
    ok(
        &mut e,
        0,
        Command::SetFunctionFolder {
            folder: folder(5, FunctionCategory::Chaser, "Chases", None),
        },
    );
    let mut s = scene(10, vec![], 0);
    s.folder = Some(FolderId(5));
    assert!(invalid(
        &mut e,
        Command::SetFunction {
            function: s.clone()
        }
    ));
    s.folder = Some(FolderId(42));
    assert!(invalid(&mut e, Command::SetFunction { function: s }));
    assert!(matches!(
        err(
            &mut e,
            0,
            Command::RemoveFunctionFolder { id: FolderId(42) }
        ),
        CommandError::NotFound { .. }
    ));
}

#[test]
fn scene_channels_list_fixtures_and_channel_values() {
    use klight_engine_core::compile::CompiledShow;
    use klight_engine_core::preview::scene_channels;
    use klight_model::inspect::{ChannelOrigin, SceneChannel, SceneFixtureChannels};
    let mut show = test_show();
    show.fixtures[5].orientation.invert_pan = false;
    show.functions.push(scene(
        1,
        vec![
            explicit(
                group(G_SPOTS),
                AttributeValue::PositionFan {
                    pan: 0.0,
                    tilt: 0.0,
                    pan_spread: 60.0,
                    tilt_spread: 0.0,
                    mode: FanMode::Linear,
                },
            ),
            explicit(fixture(RGB), AttributeValue::Color { color: Color::RED }),
            explicit(fixture(RGB), intensity(0.5)),
        ],
        0,
    ));
    let compiled = CompiledShow::build(&show);
    let p = scene_channels(&show, &compiled, FunctionId(1)).unwrap();
    let names: Vec<&str> = p.iter().map(|f| f.fixture_name.as_str()).collect();
    assert_eq!(names, vec!["RGB", "Spot 1", "Spot 2"], "sorted by address");

    let rgb = &p[0];
    assert_eq!(rgb.channels[0].address, None, "virtual dimmer first");
    assert_eq!(rgb.channels[0].value, 128);
    assert_eq!(
        (
            rgb.channels[1].address,
            rgb.channels[1].channel_name.as_str(),
            rgb.channels[1].value
        ),
        (Some(10), "Red", 255)
    );

    // Every channel of a driven fixture is listed, set or not.
    let spot = &p[1];
    let addresses: Vec<u16> = spot.channels.iter().filter_map(|c| c.address).collect();
    let size = spot.channels.len() as u16;
    assert_eq!(addresses, (40..40 + size).collect::<Vec<_>>());
    let by_name = |f: &SceneFixtureChannels, n: &str| -> SceneChannel {
        f.channels
            .iter()
            .find(|c| c.channel_name == n)
            .unwrap()
            .clone()
    };
    assert_eq!(by_name(spot, "Pan").origin, ChannelOrigin::Value);
    assert!(spot
        .channels
        .iter()
        .any(|c| c.origin == ChannelOrigin::Unset));

    // Fans are resolved per head, with the fine byte of 16-bit channels.
    let pan = |f: &SceneFixtureChannels| {
        (
            by_name(f, "Pan").value,
            f.channels
                .iter()
                .find(|c| {
                    matches!(
                        c.channel,
                        ChannelRef::Dmx {
                            attribute: Attribute::Pan,
                            fine: true,
                            ..
                        }
                    )
                })
                .unwrap()
                .value,
        )
    };
    let expect = |deg: f32| {
        let [hi, lo] = klight_fixtures::convert::degrees_to_dmx16(deg, 540.0).to_be_bytes();
        (hi, lo)
    };
    assert_eq!(pan(&p[1]), expect(-30.0));
    assert_eq!(pan(&p[2]), expect(30.0));

    assert!(scene_channels(&show, &compiled, FunctionId(99)).is_none());
}
