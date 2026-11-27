//! Chaos tests: faulty drivers must never stop the tick loop or other universes.
//!
//! The tick loop being "not stopped" is checked on the work time of its ticks
//! (a tick waiting on a driver lasts as long as the driver blocks), and on
//! progress waited for with a generous timeout. Never on how many ticks fit a
//! fixed window of wall time: a starved CI runner (macOS, with the other chaos
//! tests spinning in parallel) slows every thread down and failed such checks
//! with nothing wrong.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use klight_engine_core::{Engine, EngineConfig, LoopHooks, MonotonicClock, TickLoop};
use klight_io::{DeviceManager, DriverState, FakeFactory, ManagerConfig};
use klight_model::*;
use klight_sim::scenario::*;

struct Rig {
    tick: Option<TickLoop>,
    manager: DeviceManager,
    factory: Arc<FakeFactory>,
    shared: Arc<klight_engine_core::EngineShared>,
    tx: crossbeam_channel::Sender<CommandEnvelope>,
    /// Longest wall-time work of a tick since it was last reset to 0, in us.
    max_work_us: Arc<AtomicU64>,
}

impl Rig {
    fn ticks(&self) -> u64 {
        self.shared.stats.ticks.load(Ordering::Relaxed)
    }
}

/// A tick blocked by a driver lasts as long as the driver blocks (500 ms or
/// forever below); a free tick does not come anywhere near this, even on a
/// starved machine.
const BLOCKED_US: u64 = 250_000;

fn rig() -> Rig {
    let mut show = test_show();
    show.universes[0].output = Some(OutputAssignment {
        kind: OutputKind::Fake,
        serial_number: Some("u1".into()),
        port_hint: None,
    });
    show.universes.push(Universe {
        id: UniverseId(2),
        name: "U2".into(),
        output: Some(OutputAssignment {
            kind: OutputKind::Fake,
            serial_number: Some("u2".into()),
            port_hint: None,
        }),
    });
    let mut dim2 = show.fixtures[0].clone();
    dim2.id = FixtureId(50);
    dim2.universe = UniverseId(2);
    show.fixtures.push(dim2);
    show.functions.push(scene(
        1,
        vec![
            explicit(fixture(DIM1), intensity(1.0)),
            explicit(fixture(FixtureId(50)), intensity(1.0)),
        ],
        0,
    ));

    let engine = Engine::new(show.clone(), EngineConfig::default());
    let shared = engine.shared();
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let (rtx, _rrx) = crossbeam_channel::bounded(1024);
    let max_work_us = Arc::new(AtomicU64::new(0));
    let max = max_work_us.clone();
    let hooks = LoopHooks {
        on_timing: Some(Box::new(move |t| {
            max.fetch_max(t.work_us, Ordering::Relaxed);
        })),
        ..Default::default()
    };
    let tick = TickLoop::spawn(engine, Arc::new(MonotonicClock::new()), rx, rtx, hooks).unwrap();
    let factory = Arc::new(FakeFactory::default());
    let config = ManagerConfig {
        backoff_initial: Duration::from_millis(5),
        backoff_max: Duration::from_millis(40),
        refresh: Duration::from_millis(50),
        poll: Duration::from_millis(1),
    };
    let mut manager = DeviceManager::new(factory.clone(), shared.clone(), config);
    manager.sync(&show);
    tx.send(CommandEnvelope {
        id: 1,
        source: CommandSource::Internal,
        command: Command::StartFunction { id: FunctionId(1) },
    })
    .unwrap();
    Rig {
        tick: Some(tick),
        manager,
        factory,
        shared,
        tx,
        max_work_us,
    }
}

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn lit(r: &klight_io::Recorder) -> bool {
    r.last().is_some_and(|f| f.data[0] == 255)
}

#[test]
fn frames_reach_both_outputs() {
    let rig = rig();
    let (u1, u2) = (rig.factory.recorder("u1"), rig.factory.recorder("u2"));
    wait_for("frames", Duration::from_secs(5), || lit(&u1) && lit(&u2));
    drop(rig);
}

#[test]
fn failing_driver_reconnects_and_others_are_unaffected() {
    let rig = rig();
    let (u1, u2) = (rig.factory.recorder("u1"), rig.factory.recorder("u2"));
    wait_for("frames", Duration::from_secs(5), || lit(&u1) && lit(&u2));

    u1.set_faults(|f| f.unplugged = true);
    let before_u2 = u2.count();
    let ticks_before = rig.ticks();
    rig.max_work_us.store(0, Ordering::Relaxed);
    // Universe 2 keeps receiving frames and the tick loop keeps running.
    wait_for(
        "u2 frames and ticks with u1 unplugged",
        Duration::from_secs(10),
        || u2.count() > before_u2 + 3 && rig.ticks() >= ticks_before + 20,
    );
    wait_for("u1 seen as not ok", Duration::from_secs(5), || {
        rig.manager
            .statuses()
            .iter()
            .any(|s| s.universe == U1 && s.driver.state != DriverState::Ok)
    });
    let max = rig.max_work_us.load(Ordering::Relaxed);
    assert!(
        max < BLOCKED_US,
        "a tick took {max} us: blocked by the unplugged driver"
    );

    // Replug: the universe comes back by itself.
    u1.set_faults(|f| f.unplugged = false);
    let opens = u1.open_count();
    wait_for("reconnect", Duration::from_secs(5), || {
        u1.open_count() > opens && lit(&u1)
    });
    wait_for("status ok", Duration::from_secs(5), || {
        rig.manager
            .statuses()
            .iter()
            .any(|s| s.universe == U1 && s.driver.state == DriverState::Ok && s.reconnects >= 1)
    });
}

#[test]
fn transient_errors_and_partial_writes_recover() {
    let rig = rig();
    let u1 = rig.factory.recorder("u1");
    wait_for("frames", Duration::from_secs(5), || lit(&u1));
    u1.set_faults(|f| {
        f.fail_next_sends = 3;
        f.partial_next_sends = 2;
    });
    // Change the frame so the driver must send again.
    rig.tx
        .send(CommandEnvelope {
            id: 2,
            source: CommandSource::Internal,
            command: Command::SetGrandMaster { level: 0.5 },
        })
        .unwrap();
    wait_for("recovery", Duration::from_secs(5), || {
        u1.last().is_some_and(|f| f.data[0] == 128)
    });
    let s = rig
        .manager
        .statuses()
        .into_iter()
        .find(|s| s.universe == U1)
        .unwrap();
    assert!(s.errors >= 1, "{s:?}");
}

#[test]
fn panicking_driver_is_restarted() {
    let rig = rig();
    let u1 = rig.factory.recorder("u1");
    wait_for("frames", Duration::from_secs(5), || lit(&u1));
    u1.set_faults(|f| f.panic_next_sends = 2);
    rig.tx
        .send(CommandEnvelope {
            id: 2,
            source: CommandSource::Internal,
            command: Command::SetGrandMaster { level: 0.5 },
        })
        .unwrap();
    wait_for("recovery after panic", Duration::from_secs(5), || {
        u1.last().is_some_and(|f| f.data[0] == 128)
    });
    let s = rig
        .manager
        .statuses()
        .into_iter()
        .find(|s| s.universe == U1)
        .unwrap();
    assert!(s.panics >= 1, "{s:?}");
}

#[test]
fn slow_driver_does_not_slow_the_tick_loop() {
    let rig = rig();
    let u1 = rig.factory.recorder("u1");
    wait_for("frames", Duration::from_secs(5), || lit(&u1));
    // Each send blocks the driver for 500 ms. A tick loop waiting on the
    // driver would take 500 ms per tick; decoupled, every tick stays short
    // however slow the machine. 20 ticks span several blocked sends.
    u1.set_faults(|f| f.latency = Some(Duration::from_millis(500)));
    let sends_before = u1.count();
    let t0 = rig.ticks();
    rig.max_work_us.store(0, Ordering::Relaxed);
    wait_for(
        "20 ticks with a slow driver",
        Duration::from_secs(20),
        || rig.ticks() >= t0 + 20,
    );
    wait_for("the slow driver to send", Duration::from_secs(5), || {
        u1.count() > sends_before
    });
    let max = rig.max_work_us.load(Ordering::Relaxed);
    assert!(
        max < BLOCKED_US,
        "a tick took {max} us: it waited on the driver"
    );
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.manager.shutdown();
        if let Some(t) = self.tick.take() {
            t.stop();
        }
    }
}
