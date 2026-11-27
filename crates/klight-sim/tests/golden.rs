//! Golden tests: reference DMX frames in `tests/golden/`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use klight_model::function::{Direction, RunMode};
use klight_model::*;
use klight_sim::golden::{check, Scenario};
use klight_sim::scenario::*;

fn scenario(name: &str, commands: Vec<(u64, Command)>, samples: Vec<u64>) -> Scenario {
    Scenario {
        name: name.into(),
        show: test_show(),
        commands,
        samples,
        tick_us: 22_727,
    }
}

#[test]
fn golden_scene_fade_and_grand_master() {
    let s = scenario(
        "scene_fade_gm",
        vec![
            (
                0,
                Command::SetFunction {
                    function: scene(
                        1,
                        vec![
                            explicit(group(G_DIMMERS), intensity(1.0)),
                            explicit(fixture(RGBW), intensity(0.8)),
                            explicit(
                                fixture(RGBW),
                                AttributeValue::Color {
                                    color: Color::new(1.0, 0.5, 0.25),
                                },
                            ),
                            explicit(
                                fixture(SPOT1),
                                AttributeValue::Position {
                                    pan: 45.0,
                                    tilt: -30.0,
                                },
                            ),
                            explicit(fixture(SPOT1), intensity(1.0)),
                        ],
                        1000,
                    ),
                },
            ),
            (0, Command::StartFunction { id: FunctionId(1) }),
            (1_500_000, Command::SetGrandMaster { level: 0.5 }),
            (2_000_000, Command::SetBlackout { on: true }),
            (2_500_000, Command::SetBlackout { on: false }),
            (3_000_000, Command::StopFunction { id: FunctionId(1) }),
        ],
        vec![
            0, 250_000, 500_000, 1_000_000, 1_600_000, 2_100_000, 2_600_000, 3_500_000, 4_100_000,
        ],
    );
    check(&s).unwrap();
}

#[test]
fn golden_chaser_and_effects() {
    let s = scenario(
        "chaser_effects",
        vec![
            (0, Command::SetBpm { bpm: 120.0 }),
            (
                0,
                Command::SetFunction {
                    function: scene(1, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
                },
            ),
            (
                0,
                Command::SetFunction {
                    function: scene(2, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
                },
            ),
            (
                0,
                Command::SetFunction {
                    function: Function {
                        id: FunctionId(3),
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
                            fade: TimeValue::Ms(100),
                            direction: Direction::Forward,
                            run_mode: RunMode::PingPong,
                        }),
                    },
                },
            ),
            (
                0,
                Command::SetFunction {
                    function: Function {
                        id: FunctionId(4),
                        folder: None,
                        name: "wave".into(),
                        kind: FunctionKind::MatrixEffect(MatrixEffect {
                            group: G_PIXELS,
                            algorithm: MatrixAlgorithm::Wave {
                                color: Color::BLUE,
                                wavelength: 4.0,
                            },
                            period: TimeValue::Beats(2.0),
                            direction: Direction::Forward,
                        }),
                    },
                },
            ),
            (
                0,
                Command::SetFunction {
                    function: Function {
                        id: FunctionId(5),
                        folder: None,
                        name: "circle".into(),
                        kind: FunctionKind::MovementEffect(MovementEffect {
                            group: G_SPOTS,
                            shape: klight_model::function::MovementShape::Circle,
                            center_pan: 0.0,
                            center_tilt: 0.0,
                            width: 60.0,
                            height: 30.0,
                            period: TimeValue::Ms(2000),
                            phase_spread: 180.0,
                            direction: Direction::Forward,
                            rotation: 0.0,
                            mirror: MovementMirror::None,
                            run_mode: klight_model::function::RunMode::Loop,
                            relative: false,
                            fade_in: TimeValue::ZERO,
                            fade_out: TimeValue::ZERO,
                        }),
                    },
                },
            ),
            (0, Command::StartFunction { id: FunctionId(3) }),
            (0, Command::StartFunction { id: FunctionId(4) }),
            (0, Command::StartFunction { id: FunctionId(5) }),
        ],
        (0..12).map(|i| i * 250_000).collect(),
    );
    check(&s).unwrap();
}
