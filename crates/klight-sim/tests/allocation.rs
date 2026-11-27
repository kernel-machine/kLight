//! The tick loop must not allocate after warm-up (SPEC §8.1).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;
use klight_sim::scenario::*;

struct Counting;

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.with(|t| t.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACK.with(|t| t.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count<F: FnOnce()>(f: F) -> u64 {
    COUNT.with(|c| c.set(0));
    TRACK.with(|t| t.set(true));
    f();
    TRACK.with(|t| t.set(false));
    COUNT.with(|c| c.get())
}

#[test]
fn counter_detects_allocations() {
    let n = count(|| {
        let v: Vec<u8> = Vec::with_capacity(10);
        std::hint::black_box(v);
    });
    assert!(n >= 1);
}

#[test]
fn steady_state_tick_does_not_allocate() {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(
                1,
                vec![
                    explicit(group(G_DIMMERS), intensity(1.0)),
                    explicit(
                        fixture(RGB),
                        AttributeValue::Color {
                            color: Color::new(0.3, 0.6, 0.9),
                        },
                    ),
                    explicit(
                        fixture(SPOT1),
                        AttributeValue::Position {
                            pan: 30.0,
                            tilt: 10.0,
                        },
                    ),
                ],
                60_000,
            ),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(2),
                folder: None,
                name: "efx".into(),
                kind: FunctionKind::MovementEffect(MovementEffect {
                    group: G_SPOTS,
                    shape: MovementShape::Eight,
                    center_pan: 0.0,
                    center_tilt: 0.0,
                    width: 40.0,
                    height: 20.0,
                    period: TimeValue::Beats(4.0),
                    phase_spread: 90.0,
                    direction: Direction::Forward,
                    // Every path of the movement: turned, mirrored, back and
                    // forth, added to the scenes, growing in.
                    rotation: 30.0,
                    mirror: MovementMirror::Alternate,
                    run_mode: klight_model::function::RunMode::PingPong,
                    relative: true,
                    fade_in: TimeValue::Beats(2.0),
                    fade_out: TimeValue::ZERO,
                }),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(3),
                folder: None,
                name: "matrix".into(),
                kind: FunctionKind::MatrixEffect(MatrixEffect {
                    group: G_PIXELS,
                    algorithm: MatrixAlgorithm::Text {
                        text: "KLIGHT".into(),
                        color: Color::RED,
                    },
                    period: TimeValue::Ms(3000),
                    direction: Direction::Forward,
                    axis: MatrixAxis::Vertical,
                    controls: MatrixControls::Color,
                    run_mode: klight_model::function::RunMode::PingPong,
                    fade_in: TimeValue::Beats(2.0),
                    fade_out: TimeValue::ZERO,
                }),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(4, vec![explicit(fixture(DIM1), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(5, vec![explicit(fixture(DIM2), intensity(1.0))], 0),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(6),
                folder: None,
                name: "chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: vec![
                        ChaserStep {
                            scene: FunctionId(4),
                            hold: None,
                            fade: None,
                        },
                        ChaserStep {
                            scene: FunctionId(5),
                            hold: None,
                            fade: None,
                        },
                    ],
                    hold: TimeValue::Ms(100_000),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    // A second chaser, resting on a matrix step.
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(7),
                folder: None,
                name: "effect chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: [3, 2]
                        .map(|id| ChaserStep {
                            scene: FunctionId(id),
                            hold: None,
                            fade: None,
                        })
                        .to_vec(),
                    hold: TimeValue::Ms(100_000),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    // A collection of scenes and effects, alone and as a chaser step.
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(8),
                folder: None,
                name: "collection".into(),
                kind: FunctionKind::Collection(Collection {
                    functions: [1, 4, 2, 3].map(FunctionId).to_vec(),
                }),
            },
        },
    );
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(11),
                folder: None,
                name: "collection chase".into(),
                kind: FunctionKind::Chaser(Chaser {
                    steps: vec![ChaserStep {
                        scene: FunctionId(8),
                        hold: None,
                        fade: None,
                    }],
                    hold: TimeValue::Ms(100_000),
                    fade: TimeValue::ZERO,
                    direction: Direction::Forward,
                    run_mode: RunMode::Loop,
                }),
            },
        },
    );
    for id in [1, 2, 3, 6, 7, 8, 11] {
        ok(&mut e, 0, Command::StartFunction { id: FunctionId(id) });
    }
    ok(
        &mut e,
        0,
        Command::ProgrammerSet {
            target: fixture(RGBW),
            value: intensity(0.5),
        },
    );

    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 300,
            value: Some(9),
        },
    );
    ok(
        &mut e,
        0,
        Command::SetChannel {
            universe: U1,
            address: 1,
            value: Some(200),
        },
    );

    // Warm-up.
    let mut t = 0u64;
    for _ in 0..10 {
        t += 22_727;
        e.tick(t);
    }
    // Steady state: fades running, effects moving, no commands.
    let n = count(|| {
        for _ in 0..2000 {
            t += 22_727;
            e.tick(t);
        }
    });
    assert_eq!(n, 0, "tick allocated {n} times");
}
