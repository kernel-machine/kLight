//! Chaos tests: faulty drivers must never stop the tick loop or other universes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use klight_engine_core::{Engine, EngineConfig, MonotonicClock, TickLoop};
use klight_io::{DeviceManager, DriverState, FakeFactory, ManagerConfig};
use klight_model::*;
use klight_sim::scenario::*;

struct Rig {
    tick: Option<TickLoop>,
    manager: DeviceManager,
    factory: Arc<FakeFactory>,
    shared: Arc<klight_engine_core::EngineShared>,
    tx: crossbeam_channel::Sender<CommandEnvelope>,
}

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
    let tick = TickLoop::spawn(
        engine,
        Arc::new(MonotonicClock::new()),
        rx,
        rtx,
        Default::default(),
    )
    .unwrap();
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
    let ticks_before = rig.shared.stats.ticks.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(300));
    // Universe 2 keeps receiving frames and the tick loop keeps running.
    assert!(u2.count() > before_u2 + 3, "u2 starved");
    assert!(
        rig.shared.stats.ticks.load(Ordering::Relaxed) > ticks_before + 5,
        "tick loop stalled"
    );
    let st = rig.manager.statuses();
    let s1 = st.iter().find(|s| s.universe == U1).unwrap();
    assert_ne!(s1.driver.state, DriverState::Ok);

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
    u1.set_faults(|f| f.latency = Some(Duration::from_millis(200)));
    let t0 = rig.shared.stats.ticks.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));
    let ticks = rig.shared.stats.ticks.load(Ordering::Relaxed) - t0;
    // 44 Hz for 0.5 s ~ 22 ticks; allow scheduling slack on CI.
    assert!(ticks >= 15, "only {ticks} ticks");
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.manager.shutdown();
        if let Some(t) = self.tick.take() {
            t.stop();
        }
    }
}
