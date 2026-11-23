//! Jitter statistics without allocation (fixed histogram).

use std::sync::atomic::Ordering;

use crate::shared::TickStats;

const BUCKET_US: u32 = 50;
const BUCKETS: usize = 1000; // up to 50 ms

#[derive(Debug)]
pub struct JitterWindow {
    hist: [u32; BUCKETS],
    count: u32,
    sum: u64,
    min: u32,
    max: u32,
}

impl Default for JitterWindow {
    fn default() -> Self {
        Self {
            hist: [0; BUCKETS],
            count: 0,
            sum: 0,
            min: u32::MAX,
            max: 0,
        }
    }
}

impl JitterWindow {
    pub fn record(&mut self, jitter_us: u32) {
        let b = ((jitter_us / BUCKET_US) as usize).min(BUCKETS - 1);
        if let Some(h) = self.hist.get_mut(b) {
            *h += 1;
        }
        self.count += 1;
        self.sum += u64::from(jitter_us);
        self.min = self.min.min(jitter_us);
        self.max = self.max.max(jitter_us);
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn p99(&self) -> u32 {
        let target = (u64::from(self.count) * 99).div_ceil(100);
        let mut acc = 0u64;
        for (i, &h) in self.hist.iter().enumerate() {
            acc += u64::from(h);
            if acc >= target && target > 0 {
                return (i as u32 + 1) * BUCKET_US;
            }
        }
        self.max
    }

    /// Publishes the window and resets it.
    pub fn publish(&mut self, stats: &TickStats) {
        if self.count == 0 {
            return;
        }
        stats.jitter_min_us.store(self.min, Ordering::Relaxed);
        stats.jitter_max_us.store(self.max, Ordering::Relaxed);
        stats.jitter_avg_us.store(
            u32::try_from(self.sum / u64::from(self.count)).unwrap_or(u32::MAX),
            Ordering::Relaxed,
        );
        stats.jitter_p99_us.store(self.p99(), Ordering::Relaxed);
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p99_of_uniform() {
        let mut w = JitterWindow::default();
        for i in 0..100 {
            w.record(i * 10);
        }
        assert!(w.p99() >= 980 && w.p99() <= 1000, "{}", w.p99());
    }
}
