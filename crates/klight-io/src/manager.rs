//! Device manager: one thread per output, reconnection with capped
//! exponential backoff, panic isolation, hot-plug by USB serial number.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use klight_engine_core::EngineShared;
use klight_model::{OutputAssignment, OutputKind, Show, UniverseId};
use serde::Serialize;

use crate::driver::{DriverError, DriverState, DriverStatus, OutputDriver};
use crate::fake::{FakeOutput, Recorder};

/// Creates drivers for assignments. Called again on every reconnection, so
/// a device moved to another USB port is found again by serial number.
pub trait DriverFactory: Send + Sync {
    fn create(&self, assignment: &OutputAssignment) -> Result<Box<dyn OutputDriver>, DriverError>;
}

/// Real hardware factory.
#[derive(Debug, Default)]
pub struct SystemFactory {
    /// Recorders used for `OutputKind::Fake` assignments (dry runs).
    pub fakes: FakeFactory,
}

impl DriverFactory for SystemFactory {
    fn create(&self, a: &OutputAssignment) -> Result<Box<dyn OutputDriver>, DriverError> {
        match a.kind {
            OutputKind::Fake => self.fakes.create(a),
            #[cfg(feature = "serial")]
            OutputKind::EnttecPro | OutputKind::OpenDmx => {
                let port = crate::discovery::resolve_port(a)?;
                Ok(match a.kind {
                    OutputKind::EnttecPro => Box::new(crate::enttec::EnttecPro::new(
                        &port,
                        a.serial_number.clone(),
                    )),
                    _ => Box::new(crate::enttec::OpenDmx::new(&port, a.serial_number.clone())),
                })
            }
            #[cfg(not(feature = "serial"))]
            OutputKind::EnttecPro | OutputKind::OpenDmx => {
                Err(DriverError::Unsupported("built without serial support"))
            }
        }
    }
}

/// Factory of fake outputs, keyed by the assignment's serial number.
#[derive(Debug, Default)]
pub struct FakeFactory {
    recorders: Mutex<BTreeMap<String, Arc<Recorder>>>,
}

impl FakeFactory {
    pub fn recorder(&self, key: &str) -> Arc<Recorder> {
        match self.recorders.lock() {
            Ok(mut r) => r
                .entry(key.to_owned())
                .or_insert_with(Recorder::new)
                .clone(),
            Err(_) => Recorder::new(),
        }
    }
}

impl DriverFactory for FakeFactory {
    fn create(&self, a: &OutputAssignment) -> Result<Box<dyn OutputDriver>, DriverError> {
        let key = a.serial_number.clone().unwrap_or_else(|| "fake".into());
        Ok(Box::new(FakeOutput::new(&key, self.recorder(&key))))
    }
}

impl DriverFactory for Arc<FakeFactory> {
    fn create(&self, a: &OutputAssignment) -> Result<Box<dyn OutputDriver>, DriverError> {
        self.as_ref().create(a)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ManagerConfig {
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    /// Re-send the frame at least this often even if it did not change.
    pub refresh: Duration,
    /// How often a worker checks for a new frame.
    pub poll: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            backoff_initial: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
            refresh: Duration::from_secs(1),
            poll: Duration::from_millis(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputStatus {
    pub universe: UniverseId,
    pub kind: OutputKind,
    pub driver: DriverStatus,
    pub frames_sent: u64,
    pub errors: u64,
    pub reconnects: u64,
    pub panics: u64,
    pub last_error: Option<String>,
}

struct Worker {
    assignment: OutputAssignment,
    stop: Arc<AtomicBool>,
    status: Arc<ArcSwap<OutputStatus>>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub struct DeviceManager {
    factory: Arc<dyn DriverFactory>,
    shared: Arc<EngineShared>,
    config: ManagerConfig,
    workers: BTreeMap<UniverseId, Worker>,
}

impl DeviceManager {
    pub fn new(
        factory: Arc<dyn DriverFactory>,
        shared: Arc<EngineShared>,
        config: ManagerConfig,
    ) -> Self {
        Self {
            factory,
            shared,
            config,
            workers: BTreeMap::new(),
        }
    }

    /// Starts/stops workers so that every universe with an output
    /// assignment has exactly one driver thread.
    pub fn sync(&mut self, show: &Show) {
        let wanted: BTreeMap<UniverseId, OutputAssignment> = show
            .universes
            .iter()
            .filter_map(|u| u.output.clone().map(|o| (u.id, o)))
            .collect();
        let stale: Vec<UniverseId> = self
            .workers
            .iter()
            .filter(|(id, w)| wanted.get(id) != Some(&w.assignment))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(w) = self.workers.remove(&id) {
                tracing::info!(universe = id.0, "stopping output worker");
                w.stop();
            }
        }
        for (id, a) in wanted {
            if self.workers.contains_key(&id) {
                continue;
            }
            match self.spawn(id, a.clone()) {
                Ok(w) => {
                    self.workers.insert(id, w);
                }
                Err(e) => tracing::error!(universe = id.0, "cannot spawn output thread: {e}"),
            }
        }
    }

    fn spawn(&self, universe: UniverseId, assignment: OutputAssignment) -> std::io::Result<Worker> {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(ArcSwap::from_pointee(OutputStatus {
            universe,
            kind: assignment.kind,
            driver: DriverStatus::default(),
            frames_sent: 0,
            errors: 0,
            reconnects: 0,
            panics: 0,
            last_error: None,
        }));
        let ctx = WorkerCtx {
            universe,
            assignment: assignment.clone(),
            factory: self.factory.clone(),
            shared: self.shared.clone(),
            config: self.config,
            stop: stop.clone(),
            status: status.clone(),
        };
        let handle = std::thread::Builder::new()
            .name(format!("klight-out-{}", universe.0))
            .spawn(move || ctx.run())?;
        Ok(Worker {
            assignment,
            stop,
            status,
            handle: Some(handle),
        })
    }

    pub fn statuses(&self) -> Vec<OutputStatus> {
        self.workers
            .values()
            .map(|w| w.status.load().as_ref().clone())
            .collect()
    }

    pub fn shutdown(&mut self) {
        let workers = std::mem::take(&mut self.workers);
        for (_, w) in workers {
            w.stop();
        }
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct WorkerCtx {
    universe: UniverseId,
    assignment: OutputAssignment,
    factory: Arc<dyn DriverFactory>,
    shared: Arc<EngineShared>,
    config: ManagerConfig,
    stop: Arc<AtomicBool>,
    status: Arc<ArcSwap<OutputStatus>>,
}

struct WorkerState {
    driver: Option<Box<dyn OutputDriver>>,
    backoff: Duration,
    last_version: u64,
    last_send: Option<Instant>,
    ever_connected: bool,
    frame: [u8; 512],
}

impl WorkerCtx {
    fn update(&self, f: impl FnOnce(&mut OutputStatus)) {
        let mut s = self.status.load().as_ref().clone();
        f(&mut s);
        self.status.store(Arc::new(s));
    }

    /// Sleeps in small slices so that stop requests are honoured quickly.
    fn sleep(&self, d: Duration) {
        let end = Instant::now() + d;
        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now >= end {
                break;
            }
            std::thread::sleep((end - now).min(Duration::from_millis(20)));
        }
    }

    fn run(self) {
        let mut st = WorkerState {
            driver: None,
            backoff: self.config.backoff_initial,
            last_version: u64::MAX,
            last_send: None,
            ever_connected: false,
            frame: [0; 512],
        };
        while !self.stop.load(Ordering::Relaxed) {
            let r = catch_unwind(AssertUnwindSafe(|| self.step(&mut st)));
            if r.is_err() {
                tracing::error!(
                    universe = self.universe.0,
                    "output driver panicked; restarting it"
                );
                // Drop the driver (closing the port) and reconnect.
                if let Some(mut d) = st.driver.take() {
                    let _ = catch_unwind(AssertUnwindSafe(|| d.close()));
                }
                st.last_version = u64::MAX;
                self.update(|s| {
                    s.panics += 1;
                    s.driver.state = DriverState::Degraded;
                    s.last_error = Some("driver panicked".into());
                });
                self.sleep(st.backoff);
                st.backoff = (st.backoff * 2).min(self.config.backoff_max);
            }
        }
        if let Some(mut d) = st.driver.take() {
            d.close();
        }
        self.update(|s| s.driver.state = DriverState::Disconnected);
    }

    fn step(&self, st: &mut WorkerState) {
        if st.driver.is_none() {
            self.update(|s| s.driver.state = DriverState::Connecting);
            let opened = self.factory.create(&self.assignment).and_then(|mut d| {
                d.open()?;
                Ok(d)
            });
            match opened {
                Ok(d) => {
                    let status = d.status();
                    tracing::info!(universe = self.universe.0, port = ?status.port, "output connected");
                    let reconnect = st.ever_connected;
                    st.ever_connected = true;
                    st.driver = Some(d);
                    st.backoff = self.config.backoff_initial;
                    st.last_version = u64::MAX;
                    self.update(|s| {
                        s.driver = status;
                        s.driver.state = DriverState::Ok;
                        if reconnect {
                            s.reconnects += 1;
                        }
                    });
                }
                Err(e) => {
                    self.update(|s| {
                        s.driver.state = if st.ever_connected {
                            DriverState::Degraded
                        } else {
                            DriverState::Disconnected
                        };
                        s.last_error = Some(e.to_string());
                    });
                    self.sleep(st.backoff);
                    st.backoff = (st.backoff * 2).min(self.config.backoff_max);
                }
            }
            return;
        }

        let Some(slot) = self.shared.output(self.universe) else {
            self.sleep(Duration::from_millis(100));
            return;
        };
        let Some(driver) = st.driver.as_mut() else {
            return;
        };
        let continuous = !driver.status().hardware_buffer;
        let version = slot.version();
        let refresh_due = st
            .last_send
            .is_none_or(|t| t.elapsed() >= self.config.refresh);
        if version != st.last_version || refresh_due || continuous {
            slot.read(&mut st.frame);
            let started = Instant::now();
            match driver.send_frame(&st.frame) {
                Ok(()) => {
                    st.last_version = version;
                    st.last_send = Some(started);
                    let status = driver.status();
                    self.update(|s| {
                        s.frames_sent += 1;
                        s.driver = status;
                    });
                }
                Err(e) => {
                    tracing::warn!(universe = self.universe.0, "output error: {e}");
                    driver.close();
                    st.driver = None;
                    self.update(|s| {
                        s.errors += 1;
                        s.driver.state = DriverState::Degraded;
                        s.last_error = Some(e.to_string());
                    });
                    return;
                }
            }
            if continuous {
                let period = Duration::from_micros(driver.min_frame_period_us());
                let spent = started.elapsed();
                if spent < period {
                    self.sleep(period - spent);
                }
                return;
            }
        }
        self.sleep(self.config.poll);
    }
}
