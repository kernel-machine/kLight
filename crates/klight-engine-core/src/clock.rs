//! Injected clocks. The engine never reads the system time directly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Monotonic time source in microseconds.
pub trait Clock: Send + Sync {
    fn now_us(&self) -> u64;
}

/// Real monotonic clock (production).
#[derive(Debug)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_us(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// Manually advanced clock for deterministic tests and simulation.
#[derive(Debug, Clone, Default)]
pub struct VirtualClock {
    now: Arc<AtomicU64>,
}

impl VirtualClock {
    pub fn new(start_us: u64) -> Self {
        Self {
            now: Arc::new(AtomicU64::new(start_us)),
        }
    }
    pub fn set(&self, us: u64) {
        self.now.store(us, Ordering::SeqCst);
    }
    pub fn advance(&self, us: u64) -> u64 {
        self.now.fetch_add(us, Ordering::SeqCst).saturating_add(us)
    }
}

impl Clock for VirtualClock {
    fn now_us(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}
