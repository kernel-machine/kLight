//! The tick loop reports the timing of every tick to `on_timing`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use klight_engine_core::{Engine, EngineConfig, LoopHooks, MonotonicClock, TickLoop, TickTiming};

#[test]
fn every_tick_reports_when_it_was_due_started_and_worked() {
    let seen: Arc<Mutex<Vec<TickTiming>>> = Arc::default();
    let sink = seen.clone();
    let (_tx, rx) = crossbeam_channel::bounded(16);
    let (rtx, _rrx) = crossbeam_channel::bounded(16);
    let hooks = LoopHooks {
        on_timing: Some(Box::new(move |t| sink.lock().unwrap().push(t))),
        ..Default::default()
    };
    let engine = Engine::new(test_show(), EngineConfig::default());
    let tick = TickLoop::spawn(engine, Arc::new(MonotonicClock::new()), rx, rtx, hooks).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = tick.stop();

    let seen = seen.lock().unwrap();
    assert!(
        seen.len() >= 5,
        "about 13 ticks at 44 Hz, got {}",
        seen.len()
    );
    for t in seen.iter() {
        assert!(
            t.start_us >= t.due_us,
            "a tick never starts before it is due: {t:?}"
        );
    }
    for w in seen.windows(2) {
        assert!(w[1].due_us > w[0].due_us, "due times move forward");
    }
}

#[test]
fn a_panicking_timing_hook_does_not_stop_the_loop() {
    let (_tx, rx) = crossbeam_channel::bounded(16);
    let (rtx, _rrx) = crossbeam_channel::bounded(16);
    let hooks = LoopHooks {
        on_timing: Some(Box::new(|_| panic!("hook"))),
        ..Default::default()
    };
    let engine = Engine::new(test_show(), EngineConfig::default());
    let shared = engine.shared();
    let tick = TickLoop::spawn(engine, Arc::new(MonotonicClock::new()), rx, rtx, hooks).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(shared.stats.snapshot().ticks >= 4);
    let _ = tick.stop();
}

#[test]
fn the_probe_is_read_around_the_work_of_every_tick() {
    use klight_engine_core::{ProbeDelta, ThreadSample};
    // A fake probe whose counters move by a fixed step at every reading: the
    // delta of one tick is one step (one reading at the start, one at the end).
    let mut n = 0u64;
    let seen: Arc<Mutex<Vec<TickTiming>>> = Arc::default();
    let sink = seen.clone();
    let (_tx, rx) = crossbeam_channel::bounded(16);
    let (rtx, _rrx) = crossbeam_channel::bounded(16);
    let hooks = LoopHooks {
        on_timing: Some(Box::new(move |t| sink.lock().unwrap().push(t))),
        probe: Some(Box::new(move || {
            n += 1;
            Some(ThreadSample {
                cpu_us: n * 7,
                voluntary_switches: n * 2,
            })
        })),
        ..Default::default()
    };
    let engine = Engine::new(test_show(), EngineConfig::default());
    let tick = TickLoop::spawn(engine, Arc::new(MonotonicClock::new()), rx, rtx, hooks).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let _ = tick.stop();
    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty());
    for t in seen.iter() {
        assert_eq!(
            t.probe,
            Some(ProbeDelta {
                cpu_us: 7,
                voluntary_switches: 2
            })
        );
    }
}

#[test]
fn without_a_probe_there_is_no_reading() {
    let seen: Arc<Mutex<Vec<TickTiming>>> = Arc::default();
    let sink = seen.clone();
    let (_tx, rx) = crossbeam_channel::bounded(16);
    let (rtx, _rrx) = crossbeam_channel::bounded(16);
    let hooks = LoopHooks {
        on_timing: Some(Box::new(move |t| sink.lock().unwrap().push(t))),
        ..Default::default()
    };
    let engine = Engine::new(test_show(), EngineConfig::default());
    let tick = TickLoop::spawn(engine, Arc::new(MonotonicClock::new()), rx, rtx, hooks).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let _ = tick.stop();
    assert!(seen.lock().unwrap().iter().all(|t| t.probe.is_none()));
}
