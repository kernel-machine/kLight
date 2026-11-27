//! Soak test: a realistic show running with the real clock for a long time.
//!
//! The engine is judged on what the machine cannot change (see
//! `klight_sim::soak`), read on the tick thread at the start and the end of
//! every tick's work:
//! - CPU time of one tick above `--max-tick-cpu-us`: fails, even once;
//! - any voluntary context switch during a tick's work (a lock, a sleep or I/O
//!   in the tick, forbidden by SPEC §16): fails, even once;
//! - CPU time p99.9 over the whole run above `--max-cpu-p999-us`: fails.
//!
//! And on stable memory, no lost ticks or timings, no output errors.
//!
//! Wall time mixes in what the host does (other programs, the hypervisor
//! taking the CPU away, a paused VM), so it tells whether the machine is fit
//! for a meaningful soak: wake-up jitter p99 over the whole run above
//! `--max-p99-us`, or the process paused longer than `--max-host-stall-us`,
//! fails as "machine unfit". Slow ticks in wall time are listed with their CPU
//! time, to see who held them up.
//!
//! Where the OS gives no per-thread figures, each tick's wall-time work is
//! judged against `--max-work-us` instead.
//!
//! `--inject-slow-tick-ms N` / `--inject-busy-tick-ms N`: one tick in 440
//! sleeps / computes for N ms, to check the soak fails as it must.
//!
//! `cargo run -p klight-sim --release --bin klight-soak -- --minutes 30`
//! (CI: 30 minutes on pull requests, 24-48 h nightly.)

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use klight_engine_core::{Engine, EngineConfig, LoopHooks, MonotonicClock, TickLoop, TickTiming};
use klight_io::{DeviceManager, FakeFactory, ManagerConfig};
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::*;
use klight_sim::scenario::*;
use klight_sim::soak::{thread_probe, tick_faults, Fault, Histogram, Interval, Witness};

/// Limits; all in microseconds.
struct Limits {
    /// CPU time of one tick.
    tick_cpu: u64,
    /// CPU time p99.9 over the whole run.
    cpu_p999: u64,
    /// Wall-time work of one tick, only where there is no probe.
    work: u64,
    /// Wake-up jitter p99 over the whole run (the machine).
    late_p99: u64,
    /// Longest pause of the whole process (the machine).
    host_stall: u64,
}

/// A tick slower than this in wall time is listed (at most `LIST` of them).
const SLOW_US: u64 = 5_000;
const LIST: usize = 10;

fn parse(args: &mut impl Iterator<Item = String>, current: u64) -> u64 {
    args.next().and_then(|v| v.parse().ok()).unwrap_or(current)
}

fn minutes(us: u64) -> f64 {
    us as f64 / 60_000_000.0
}

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
    let mut minutes_total = 1.0f64;
    let mut lim = Limits {
        tick_cpu: 3_000,
        cpu_p999: 1_000,
        work: 3_000,
        late_p99: 5_000,
        host_stall: 100_000,
    };
    let mut inject_sleep_ms = 0u64;
    let mut inject_busy_ms = 0u64;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--minutes" => {
                minutes_total = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(minutes_total)
            }
            "--max-tick-cpu-us" => lim.tick_cpu = parse(&mut args, lim.tick_cpu),
            "--max-cpu-p999-us" => lim.cpu_p999 = parse(&mut args, lim.cpu_p999),
            "--max-work-us" => lim.work = parse(&mut args, lim.work),
            "--max-p99-us" => lim.late_p99 = parse(&mut args, lim.late_p99),
            "--max-host-stall-us" => lim.host_stall = parse(&mut args, lim.host_stall),
            "--inject-slow-tick-ms" => inject_sleep_ms = parse(&mut args, inject_sleep_ms),
            "--inject-busy-tick-ms" => inject_busy_ms = parse(&mut args, inject_busy_ms),
            _ => {
                eprintln!(
                    "usage: klight-soak [--minutes N] [--max-tick-cpu-us N] [--max-cpu-p999-us N] \
                     [--max-work-us N] [--max-p99-us N] [--max-host-stall-us N] \
                     [--inject-slow-tick-ms N] [--inject-busy-tick-ms N]"
                );
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
            rotation: 0.0,
            mirror: MovementMirror::None,
            run_mode: klight_model::function::RunMode::Loop,
            relative: false,
            fade_in: TimeValue::ZERO,
            fade_out: TimeValue::ZERO,
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
            axis: MatrixAxis::Horizontal,
            run_mode: klight_model::function::RunMode::Loop,
            fade_in: TimeValue::ZERO,
            fade_out: TimeValue::ZERO,
        }),
    });

    let engine = Engine::new(show.clone(), EngineConfig::default());
    let shared = engine.shared();
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let (rtx, rrx) = crossbeam_channel::bounded(1024);
    // Tick timings reach this thread through a bounded channel: `try_send`
    // neither blocks nor allocates on the tick thread.
    let (ttx, trx) = crossbeam_channel::bounded::<TickTiming>(1 << 16);
    let dropped = Arc::new(AtomicU64::new(0));
    let dropped_in_hook = dropped.clone();
    let probe = thread_probe();
    let probed = probe.is_some();
    let clock = Arc::new(MonotonicClock::new());
    let hooks = LoopHooks {
        on_timing: Some(Box::new(move |t| {
            if ttx.try_send(t).is_err() {
                dropped_in_hook.fetch_add(1, Ordering::Relaxed);
            }
        })),
        on_tick: (inject_sleep_ms > 0 || inject_busy_ms > 0).then(|| {
            let mut ticks = 0u64;
            // Computes until the thread has used that much CPU (on a busy
            // machine wall time would give it less), else for that wall time.
            let mut cpu = thread_probe();
            Box::new(move |_: &Engine| {
                ticks += 1;
                if ticks.is_multiple_of(440) {
                    if inject_sleep_ms > 0 {
                        std::thread::sleep(Duration::from_millis(inject_sleep_ms));
                    }
                    let want_us = inject_busy_ms * 1_000;
                    let from = cpu.as_mut().and_then(|c| c());
                    let wall = Instant::now();
                    let mut done = 0;
                    while done < want_us {
                        std::hint::spin_loop();
                        done = match (from, cpu.as_mut().and_then(|c| c())) {
                            (Some(a), Some(b)) => b.cpu_us.saturating_sub(a.cpu_us),
                            _ => u64::try_from(wall.elapsed().as_micros()).unwrap_or(u64::MAX),
                        };
                    }
                }
            }) as klight_engine_core::runner::TickHook
        }),
        probe,
        ..Default::default()
    };
    let tick = match TickLoop::spawn(engine, clock.clone(), rx, rtx, hooks) {
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
    if !probed {
        println!("no per-thread CPU time on this OS: ticks are judged on wall time");
    }

    let duration = Duration::from_secs_f64(minutes_total * 60.0);
    let start = Instant::now();
    let recorder = factory.recorder("soak");
    let witness = match Witness::spawn(clock.clone(), 1_000, 1_000) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("cannot start the witness thread: {e}");
            std::process::exit(1);
        }
    };
    std::thread::sleep(Duration::from_secs(5)); // warm-up
    use klight_engine_core::Clock;
    let measure_from = clock.now_us();
    let rss_start = rss_kib();

    struct Run {
        late: Histogram,
        work: Histogram,
        cpu: Histogram,
        faults: Vec<(u64, Fault)>,
        slow: Vec<TickTiming>,
        slow_count: usize,
    }
    let run = Mutex::new(Run {
        late: Histogram::new(10, 100_000),
        work: Histogram::new(10, 100_000),
        cpu: Histogram::new(10, 100_000),
        faults: Vec::new(),
        slow: Vec::new(),
        slow_count: 0,
    });
    let take = |r: &mut Run| {
        for t in trx.try_iter() {
            if t.due_us < measure_from {
                continue;
            }
            r.late.record(t.start_us.saturating_sub(t.due_us));
            r.work.record(t.work_us);
            if let Some(p) = t.probe {
                r.cpu.record(p.cpu_us);
            }
            for f in tick_faults(&t, lim.tick_cpu, lim.work) {
                r.faults.push((t.start_us, f));
            }
            if t.start_us.saturating_sub(t.due_us).max(t.work_us) > SLOW_US {
                r.slow_count += 1;
                if r.slow.len() < LIST {
                    r.slow.push(t);
                }
            }
        }
    };

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
        if let Ok(mut r) = run.lock() {
            take(&mut r);
            if n.is_multiple_of(60) {
                let st = shared.stats.snapshot();
                println!(
                    "{:>6.1} min  ticks {}  cpu p99.9 {} us max {} us  engine faults {}  \
                     wall: late p99 {} us max {} us, work max {} us  overruns {}  frames out {}  rss {:?} KiB",
                    start.elapsed().as_secs_f64() / 60.0,
                    st.ticks,
                    r.cpu.quantile(0.999),
                    r.cpu.max(),
                    r.faults.len(),
                    r.late.quantile(0.99),
                    r.late.max(),
                    r.work.max(),
                    st.overruns,
                    recorder.count(),
                    rss_kib()
                );
            }
        }
        // Frames are recorded in memory by the fake driver: keep the last ones only.
        if let Ok(mut f) = recorder.frames.lock() {
            let len = f.len();
            if len > 10_000 {
                f.drain(..len - 1000);
            }
        }
    }
    let stalls: Vec<Interval> = witness.stop();
    let st = shared.stats.snapshot();
    let elapsed = start.elapsed().as_secs_f64();
    let Ok(mut r) = run.into_inner() else {
        eprintln!("SOAK FAILURE: statistics lost");
        std::process::exit(1);
    };
    take(&mut r);

    // The engine.
    for (at, f) in &r.faults {
        let what = match f {
            Fault::Cpu(us) => format!("computed for {us} us (limit {} us)", lim.tick_cpu),
            Fault::Blocked(n) => format!(
                "gave up the CPU by itself {n} time(s) during its work: a lock, a sleep or I/O in the tick"
            ),
            Fault::Wall(us) => format!("work took {us} us of wall time (limit {} us)", lim.work),
        };
        failures.push(format!("engine: tick at {:.2} min {what}", minutes(*at)));
    }
    if probed && r.cpu.quantile(0.999) > lim.cpu_p999 {
        failures.push(format!(
            "engine: CPU time p99.9 {} us above {} us (whole run)",
            r.cpu.quantile(0.999),
            lim.cpu_p999
        ));
    }
    let expected_ticks = (elapsed * 44.0) as u64;
    if st.ticks < expected_ticks * 98 / 100 {
        failures.push(format!("lost ticks: {} of ~{expected_ticks}", st.ticks));
    }
    let lost = dropped.load(Ordering::Relaxed);
    if lost > 0 {
        failures.push(format!(
            "{lost} tick timings lost: the measure is incomplete"
        ));
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

    // The machine.
    let worst_stall = stalls.iter().map(Interval::len_us).max().unwrap_or(0);
    println!(
        "machine: wake-up jitter p99 {} us max {} us; process paused {} times (worst {worst_stall} us); \
         {} ticks over {SLOW_US} us of wall time",
        r.late.quantile(0.99),
        r.late.max(),
        stalls.len(),
        r.slow_count
    );
    for t in &r.slow {
        let cpu = t.probe.map_or("?".into(), |p| p.cpu_us.to_string());
        println!(
            "  at {:.2} min: late {} us, work {} us wall / {cpu} us CPU",
            minutes(t.start_us),
            t.start_us.saturating_sub(t.due_us),
            t.work_us
        );
    }
    if r.slow_count > r.slow.len() {
        println!("  ... and {} more", r.slow_count - r.slow.len());
    }
    if r.late.quantile(0.99) > lim.late_p99 {
        failures.push(format!(
            "machine unfit: wake-up jitter p99 {} us above {} us over the whole run",
            r.late.quantile(0.99),
            lim.late_p99
        ));
    }
    if worst_stall > lim.host_stall {
        failures.push(format!(
            "machine unfit: the process was paused for {worst_stall} us (limit {} us)",
            lim.host_stall
        ));
    }

    manager.shutdown();
    let _ = tick.stop();
    if failures.is_empty() {
        println!(
            "soak OK: {elapsed:.0} s, {} ticks, CPU p99.9 {} us max {} us, wall late p99 {} us",
            st.ticks,
            r.cpu.quantile(0.999),
            r.cpu.max(),
            r.late.quantile(0.99)
        );
    } else {
        for f in &failures {
            eprintln!("SOAK FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
