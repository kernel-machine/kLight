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
/// Callback run once on the tick thread before the loop starts (e.g. to
/// raise the thread priority, which needs OS calls kept out of this crate).
pub type StartHook = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub struct LoopHooks {
    pub on_start: Option<StartHook>,
    pub on_tick: Option<TickHook>,
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
                if let Some(f) = hooks.on_start {
                    f();
                }
                run(
                    &mut engine,
                    clock.as_ref(),
                    &commands,
                    &results,
                    &stop_flag,
                    hooks.on_tick,
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
    mut on_tick: Option<TickHook>,
) {
    let shared = engine.shared();
    let mut window = JitterWindow::default();
    let mut next = clock.now_us();
    let mut panics: u32 = 0;
    while !stop.load(Ordering::Relaxed) {
        let hz = u64::from(engine.show().settings.tick_hz.clamp(1, 1000));
        let period = 1_000_000 / hz;
        let now = clock.now_us();
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
            if let Some(cb) = on_tick.as_mut() {
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
        shared
            .stats
            .work_us
            .store(u32::try_from(work).unwrap_or(u32::MAX), Ordering::Relaxed);
        shared.stats.ticks.fetch_add(1, Ordering::Relaxed);

        next = next.saturating_add(period);
        let after = clock.now_us();
        if after > next {
            // Overrun: skip missed ticks instead of bursting.
            shared.stats.overruns.fetch_add(1, Ordering::Relaxed);
            next = after;
            continue;
        }
        wait_until(clock, next, stop);
    }
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
