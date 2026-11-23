//! State shared between the tick loop and the rest of the process. Readers
//! (API server, drivers, autosave) only ever see immutable snapshots or
//! lock-free slots: they can never block the tick loop.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use klight_model::{RuntimeSnapshot, Show, UniverseId};

use crate::frame::FrameSlot;

#[derive(Debug, Clone)]
pub struct UniverseOutput {
    pub id: UniverseId,
    pub slot: Arc<FrameSlot>,
}

/// Tick timing statistics, in microseconds (lock-free).
#[derive(Debug, Default)]
pub struct TickStats {
    pub ticks: AtomicU64,
    pub jitter_min_us: AtomicU32,
    pub jitter_avg_us: AtomicU32,
    pub jitter_max_us: AtomicU32,
    pub jitter_p99_us: AtomicU32,
    /// Duration of the last tick's work.
    pub work_us: AtomicU32,
    pub overruns: AtomicU64,
    /// Commands dropped because the per-tick limit was reached (deferred, not lost).
    pub deferred_commands: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TickStatsSnapshot {
    pub ticks: u64,
    pub jitter_min_us: u32,
    pub jitter_avg_us: u32,
    pub jitter_max_us: u32,
    pub jitter_p99_us: u32,
    pub work_us: u32,
    pub overruns: u64,
}

impl TickStats {
    pub fn snapshot(&self) -> TickStatsSnapshot {
        TickStatsSnapshot {
            ticks: self.ticks.load(Ordering::Relaxed),
            jitter_min_us: self.jitter_min_us.load(Ordering::Relaxed),
            jitter_avg_us: self.jitter_avg_us.load(Ordering::Relaxed),
            jitter_max_us: self.jitter_max_us.load(Ordering::Relaxed),
            jitter_p99_us: self.jitter_p99_us.load(Ordering::Relaxed),
            work_us: self.work_us.load(Ordering::Relaxed),
            overruns: self.overruns.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct EngineShared {
    pub show: ArcSwap<Show>,
    pub snapshot: ArcSwap<RuntimeSnapshot>,
    pub outputs: ArcSwap<Vec<UniverseOutput>>,
    pub stats: TickStats,
    /// Current beat position (32.32 fixed point).
    pub beat: AtomicU64,
    /// Increments every time the show changes.
    pub show_seq: AtomicU64,
}

impl EngineShared {
    pub fn new(show: Arc<Show>) -> Self {
        Self {
            show: ArcSwap::new(show),
            snapshot: ArcSwap::from_pointee(RuntimeSnapshot::default()),
            outputs: ArcSwap::from_pointee(Vec::new()),
            stats: TickStats::default(),
            beat: AtomicU64::new(0),
            show_seq: AtomicU64::new(0),
        }
    }

    pub fn output(&self, id: UniverseId) -> Option<Arc<FrameSlot>> {
        self.outputs
            .load()
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.slot.clone())
    }
}
