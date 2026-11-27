//! The tick loop thread: drains the command queue, ticks the engine at a
//! fixed rate and measures jitter. A panic inside a tick is caught and
//! logged; the last valid frames stay published to the drivers.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError};
use klight_model::{CommandEnvelope, CommandResult};

use crate::clock::Clock;
use crate::engine::Engine;
use crate::stats::JitterWindow;

/// Callback run after every tick.
pub type TickHook = Box<dyn FnMut(&Engine) + Send>;
/// Timing of one tick, in microseconds of the loop's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickTiming {
    /// When the tick was due.
    pub due_us: u64,
    /// When it started (`start_us - due_us` is the wake-up jitter: the OS).
    pub start_us: u64,
    /// How long commands and the tick took, in wall time (the engine's work,
    /// plus any time the OS did not run it).
    pub work_us: u64,
    /// What the thread probe saw during the work, when there is one.
    pub probe: Option<ProbeDelta>,
}

/// A reading of the tick thread taken by a [`ThreadProbe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThreadSample {
    /// CPU time used by the thread so far (not counting time the OS or the
    /// hypervisor did not run it).
    pub cpu_us: u64,
    /// Times the thread gave up the CPU by itself so far: blocked on a lock,
    /// slept or waited for I/O.
    pub voluntary_switches: u64,
}

/// What happened on the tick thread between the start and the end of the
/// work of one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProbeDelta {
    pub cpu_us: u64,
    pub voluntary_switches: u64,
}

/// Reads the calling (tick) thread's CPU time and voluntary context switches,
/// at the start and the end of every tick's work. OS calls stay out of this
/// crate: the soak test passes one. It must be quick and must not block.
pub type ThreadProbe = Box<dyn FnMut() -> Option<ThreadSample> + Send>;

/// Callback run after every tick with its timing, on the tick thread: it must
/// not block or allocate (e.g. a `try_send` on a bounded channel). Used by
/// the soak test to tell the engine's slowness from the host's.
pub type TimingHook = Box<dyn FnMut(TickTiming) + Send>;
/// Callback run once on the tick thread before the loop starts (e.g. to
/// raise the thread priority, which needs OS calls kept out of this crate).
pub type StartHook = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub struct LoopHooks {
    pub on_start: Option<StartHook>,
    pub on_tick: Option<TickHook>,
    pub on_timing: Option<TimingHook>,
    pub probe: Option<ThreadProbe>,
}

/// Handle to a running tick loop.
pub struct TickLoop {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Engine>>,
}

impl TickLoop {
    /// Spawns the loop. `results` receives one result per command (dropped
    /// when the consumer is too slow: the tick loop never waits).
    pub fn spawn(
        mut engine: Engine,
        clock: Arc<dyn Clock>,
        commands: Receiver<CommandEnvelope>,
        results: Sender<CommandResult>,
        hooks: LoopHooks,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let handle = std::thread::Builder::new()
            .name("klight-tick".into())
            .spawn(move || {
                let mut hooks = hooks;
                if let Some(f) = hooks.on_start.take() {
                    f();
                }
                run(
                    &mut engine,
                    clock.as_ref(),
                    &commands,
                    &results,
                    &stop_flag,
                    &mut hooks,
                );
                engine
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    /// Stops the loop and returns the engine.
    pub fn stop(mut self) -> Option<Engine> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take().and_then(|h| h.join().ok())
    }
}

impl Drop for TickLoop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(
    engine: &mut Engine,
    clock: &dyn Clock,
    commands: &Receiver<CommandEnvelope>,
    results: &Sender<CommandResult>,
    stop: &AtomicBool,
    hooks: &mut LoopHooks,
) {
    let shared = engine.shared();
    let mut window = JitterWindow::default();
    let mut next = clock.now_us();
    let mut panics: u32 = 0;
    while !stop.load(Ordering::Relaxed) {
        let hz = u64::from(engine.show().settings.tick_hz.clamp(1, 1000));
        let period = 1_000_000 / hz;
        let now = clock.now_us();
        let before = sample(&mut hooks.probe);
        let jitter = now.saturating_sub(next);
        window.record(u32::try_from(jitter).unwrap_or(u32::MAX));
        if window.count() >= u32::try_from(hz).unwrap_or(44) {
            window.publish(&shared.stats);
        }

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let limit = engine.config().max_commands_per_tick;
            for _ in 0..limit {
                let Ok(env) = commands.try_recv() else { break };
                let r = engine.apply(now, env);
                if let Err(TrySendError::Full(_)) = results.try_send(r) {
                    tracing::warn!("command result dropped: consumer too slow");
                }
            }
            if !commands.is_empty() {
                shared
                    .stats
                    .deferred_commands
                    .fetch_add(1, Ordering::Relaxed);
            }
            engine.tick(now);
            if let Some(cb) = hooks.on_tick.as_mut() {
                cb(engine);
            }
        }));
        if outcome.is_err() {
            panics = panics.saturating_add(1);
            tracing::error!(
                panics,
                "panic in tick loop caught; keeping last valid frames"
            );
        }
        let work = clock.now_us().saturating_sub(now);
        let after = sample(&mut hooks.probe);
        shared
            .stats
            .work_us
            .store(u32::try_from(work).unwrap_or(u32::MAX), Ordering::Relaxed);
        shared.stats.ticks.fetch_add(1, Ordering::Relaxed);
        if let Some(cb) = hooks.on_timing.as_mut() {
            let timing = TickTiming {
                due_us: next,
                start_us: now,
                work_us: work,
                probe: before.zip(after).map(|(b, a)| ProbeDelta {
                    cpu_us: a.cpu_us.saturating_sub(b.cpu_us),
                    voluntary_switches: a.voluntary_switches.saturating_sub(b.voluntary_switches),
                }),
            };
            if catch_unwind(AssertUnwindSafe(|| cb(timing))).is_err() {
                tracing::error!("panic in timing hook caught");
            }
        }

        next = next.saturating_add(period);
        let end = clock.now_us();
        if end > next {
            // Overrun: skip missed ticks instead of bursting.
            shared.stats.overruns.fetch_add(1, Ordering::Relaxed);
            next = end;
            continue;
        }
        wait_until(clock, next, stop);
    }
}

/// One reading of the probe, if any; a panicking probe reads as nothing.
fn sample(probe: &mut Option<ThreadProbe>) -> Option<ThreadSample> {
    let p = probe.as_mut()?;
    catch_unwind(AssertUnwindSafe(p)).ok().flatten()
}

/// Sleeps most of the remaining time, then spins for precision.
fn wait_until(clock: &dyn Clock, deadline: u64, stop: &AtomicBool) {
    loop {
        let now = clock.now_us();
        if now >= deadline || stop.load(Ordering::Relaxed) {
            return;
        }
        let left = deadline - now;
        if left > 1500 {
            std::thread::sleep(Duration::from_micros(left - 1000));
        } else if left > 100 {
            std::thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}
