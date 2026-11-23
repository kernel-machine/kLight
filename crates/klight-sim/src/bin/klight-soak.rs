//! Soak test: a realistic show running with the real clock for a long time.
//! Checks stable memory, tick jitter within threshold and no lost frames.
//!
//! `cargo run -p klight-sim --release --bin klight-soak -- --minutes 30`
//! (CI: 30 minutes on pull requests, 24-48 h nightly.)

use std::sync::Arc;
use std::time::{Duration, Instant};

use klight_engine_core::{Engine, EngineConfig, MonotonicClock, TickLoop};
use klight_io::{DeviceManager, FakeFactory, ManagerConfig};
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;
use klight_sim::scenario::*;

fn rss_kib() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn main() {
    let mut minutes = 1.0f64;
    let mut max_p99_us = 5_000u32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--minutes" => minutes = args.next().and_then(|v| v.parse().ok()).unwrap_or(minutes),
            "--max-p99-us" => {
                max_p99_us = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(max_p99_us)
            }
            _ => {
                eprintln!("usage: klight-soak [--minutes N] [--max-p99-us N]");
                std::process::exit(2);
            }
        }
    }

    let mut show = test_show();
    show.universes[0].output = Some(OutputAssignment {
        kind: OutputKind::Fake,
        serial_number: Some("soak".into()),
        port_hint: None,
    });
    show.functions.push(scene(
        1,
        vec![explicit(group(G_DIMMERS), intensity(1.0))],
        300,
    ));
    show.functions.push(scene(
        2,
        vec![explicit(
            fixture(RGB),
            AttributeValue::Color { color: Color::BLUE },
        )],
        300,
    ));
    show.functions.push(Function {
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
            fade: TimeValue::Ms(200),
            direction: Direction::Forward,
            run_mode: RunMode::PingPong,
        }),
    });
    show.functions.push(Function {
        id: FunctionId(4),
        folder: None,
        name: "efx".into(),
        kind: FunctionKind::MovementEffect(MovementEffect {
            group: G_SPOTS,
            shape: MovementShape::Circle,
            center_pan: 0.0,
            center_tilt: 0.0,
            width: 90.0,
            height: 45.0,
            period: TimeValue::Beats(8.0),
            phase_spread: 90.0,
            direction: Direction::Forward,
        }),
    });
    show.functions.push(Function {
        id: FunctionId(5),
        folder: None,
        name: "matrix".into(),
        kind: FunctionKind::MatrixEffect(MatrixEffect {
            group: G_PIXELS,
            algorithm: MatrixAlgorithm::Wave {
                color: Color::RED,
                wavelength: 3.0,
            },
            period: TimeValue::Ms(1500),
            direction: Direction::Forward,
        }),
    });

    let engine = Engine::new(show.clone(), EngineConfig::default());
    let shared = engine.shared();
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let (rtx, rrx) = crossbeam_channel::bounded(1024);
    let tick = match TickLoop::spawn(
        engine,
        Arc::new(MonotonicClock::new()),
        rx,
        rtx,
        Default::default(),
    ) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot start tick loop: {e}");
            std::process::exit(1);
        }
    };
    let factory = Arc::new(FakeFactory::default());
    let mut manager = DeviceManager::new(factory.clone(), shared.clone(), ManagerConfig::default());
    manager.sync(&show);
    for id in [3, 4, 5] {
        let _ = tx.send(CommandEnvelope {
            id,
            source: CommandSource::Internal,
            command: Command::StartFunction {
                id: FunctionId(id as u32),
            },
        });
    }

    let duration = Duration::from_secs_f64(minutes * 60.0);
    let start = Instant::now();
    let recorder = factory.recorder("soak");
    std::thread::sleep(Duration::from_secs(5)); // warm-up
    let rss_start = rss_kib();
    let mut worst_p99 = 0u32;
    let mut failures = Vec::new();
    let mut n = 0u64;
    while start.elapsed() < duration {
        std::thread::sleep(Duration::from_secs(1));
        n += 1;
        // Operator activity: tempo changes and GM moves every few seconds.
        if n.is_multiple_of(7) {
            let _ = tx.try_send(CommandEnvelope {
                id: 0,
                source: CommandSource::LocalUi,
                command: Command::SetBpm {
                    bpm: 90.0 + (n % 60) as f64,
                },
            });
        }
        if n.is_multiple_of(5) {
            let _ = tx.try_send(CommandEnvelope {
                id: 0,
                source: CommandSource::LocalUi,
                command: Command::SetGrandMaster {
                    level: 0.5 + (n % 10) as f32 / 20.0,
                },
            });
        }
        while rrx.try_recv().is_ok() {}
        let st = shared.stats.snapshot();
        worst_p99 = worst_p99.max(st.jitter_p99_us);
        if n.is_multiple_of(60) {
            println!("{:>6.1} min  ticks {}  p99 {} us  max {} us  overruns {}  frames out {}  rss {:?} KiB",
                start.elapsed().as_secs_f64() / 60.0, st.ticks, st.jitter_p99_us, st.jitter_max_us, st.overruns, recorder.count(), rss_kib());
        }
        // Frames are recorded in memory by the fake driver: keep the last ones only.
        if let Ok(mut f) = recorder.frames.lock() {
            let len = f.len();
            if len > 10_000 {
                f.drain(..len - 1000);
            }
        }
    }
    let st = shared.stats.snapshot();
    let elapsed = start.elapsed().as_secs_f64();
    let expected_ticks = (elapsed * 44.0) as u64;
    if st.ticks < expected_ticks * 98 / 100 {
        failures.push(format!("lost ticks: {} of ~{expected_ticks}", st.ticks));
    }
    if worst_p99 > max_p99_us {
        failures.push(format!("jitter p99 {worst_p99} us above {max_p99_us} us"));
    }
    if let (Some(a), Some(b)) = (rss_start, rss_kib()) {
        // Allow some slack for allocator behaviour; growth beyond means a leak.
        if b > a + 16 * 1024 {
            failures.push(format!("memory grew from {a} KiB to {b} KiB"));
        }
    }
    let statuses = manager.statuses();
    if statuses.iter().any(|s| s.errors > 0) {
        failures.push(format!("output errors: {statuses:?}"));
    }
    manager.shutdown();
    let _ = tick.stop();
    if failures.is_empty() {
        println!(
            "soak OK: {elapsed:.0} s, {} ticks, worst p99 {worst_p99} us",
            st.ticks
        );
    } else {
        for f in &failures {
            eprintln!("SOAK FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
