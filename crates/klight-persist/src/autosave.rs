//! Autosave thread: saves the show after changes (debounced) and the runtime
//! state frequently. Reads only snapshots: never blocks the tick loop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use klight_engine_core::EngineShared;
use status_cell::Status;

/// Minimal status cell (a mutex is fine here: the tick loop never touches it).
mod status_cell {
    use std::sync::Mutex;

    use super::AutosaveStatus;

    #[derive(Debug, Default)]
    pub struct Status(Mutex<AutosaveStatus>);

    impl Status {
        pub fn get(&self) -> AutosaveStatus {
            self.0.lock().map(|s| s.clone()).unwrap_or_default()
        }
        pub fn update(&self, f: impl FnOnce(&mut AutosaveStatus)) {
            if let Ok(mut s) = self.0.lock() {
                f(&mut s);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutosaveConfig {
    pub show_path: PathBuf,
    /// Wait this long after the last change before saving.
    pub debounce: Duration,
    /// Save at least this often while changes keep coming.
    pub max_delay: Duration,
    pub runtime_interval: Duration,
    pub keep_backups: usize,
}

impl AutosaveConfig {
    pub fn new(show_path: PathBuf) -> Self {
        Self {
            show_path,
            debounce: Duration::from_secs(2),
            max_delay: Duration::from_secs(30),
            runtime_interval: Duration::from_millis(500),
            keep_backups: 10,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct AutosaveStatus {
    /// Unix time in milliseconds of the last successful show save.
    pub last_show_save_ms: Option<u64>,
    pub last_runtime_save_ms: Option<u64>,
    pub last_error: Option<String>,
    pub pending: bool,
}

pub struct Autosaver {
    stop: Arc<AtomicBool>,
    status: Arc<Status>,
    handle: Option<JoinHandle<()>>,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

impl Autosaver {
    pub fn spawn(shared: Arc<EngineShared>, config: AutosaveConfig) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Status::default());
        let (s2, st2) = (stop.clone(), status.clone());
        // Baseline taken now: changes made after spawn() returns are saved.
        let baseline = shared.show_seq.load(Ordering::SeqCst);
        let handle = std::thread::Builder::new()
            .name("klight-autosave".into())
            .spawn(move || run(shared, config, baseline, s2, st2))?;
        Ok(Self {
            stop,
            status,
            handle: Some(handle),
        })
    }

    pub fn status(&self) -> AutosaveStatus {
        self.status.get()
    }

    /// Stops the thread after a final flush.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    shared: Arc<EngineShared>,
    cfg: AutosaveConfig,
    baseline: u64,
    stop: Arc<AtomicBool>,
    status: Arc<Status>,
) {
    let runtime_path = crate::runtime_path(&cfg.show_path);
    let mut saved_show_seq = baseline;
    let mut saved_state_seq = 0u64;
    let mut first_change: Option<Instant> = None;
    let mut last_change = Instant::now();
    let mut seen_seq = saved_show_seq;
    let mut last_runtime = Instant::now() - cfg.runtime_interval;

    let save_show = |status: &Status| -> bool {
        let show = shared.show.load_full();
        match crate::save_show(&cfg.show_path, &show, cfg.keep_backups) {
            Ok(()) => {
                status.update(|s| {
                    s.last_show_save_ms = Some(unix_ms());
                    s.last_error = None;
                    s.pending = false;
                });
                true
            }
            Err(e) => {
                tracing::error!("autosave failed: {e}");
                status.update(|s| s.last_error = Some(e.to_string()));
                false
            }
        }
    };

    loop {
        let stopping = stop.load(Ordering::Relaxed);
        let seq = shared.show_seq.load(Ordering::Relaxed);
        if seq != seen_seq {
            seen_seq = seq;
            last_change = Instant::now();
            first_change.get_or_insert(last_change);
            status.update(|s| s.pending = true);
        }
        if seq != saved_show_seq {
            let quiet = last_change.elapsed() >= cfg.debounce;
            let overdue = first_change.is_some_and(|t| t.elapsed() >= cfg.max_delay);
            if (quiet || overdue || stopping) && save_show(&status) {
                saved_show_seq = seq;
                first_change = None;
            }
        }

        let snap = shared.snapshot.load();
        if snap.seq != saved_state_seq
            && (last_runtime.elapsed() >= cfg.runtime_interval || stopping)
        {
            match crate::save_runtime(&runtime_path, &snap.state) {
                Ok(()) => {
                    saved_state_seq = snap.seq;
                    status.update(|s| s.last_runtime_save_ms = Some(unix_ms()));
                }
                Err(e) => {
                    tracing::error!("runtime state save failed: {e}");
                    status.update(|s| s.last_error = Some(e.to_string()));
                }
            }
            last_runtime = Instant::now();
        }
        if stopping {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
