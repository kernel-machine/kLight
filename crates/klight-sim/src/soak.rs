//! Soak harness: statistics over the whole run, and judging the engine apart
//! from the machine it runs on.
//!
//! The wall time of a tick mixes the engine's work with everything the host
//! does to the process: other programs on the same CPU, the hypervisor taking
//! the virtual CPU away (steal), the whole VM paused. None of that is a bug in
//! kLight, and on a shared CI machine it happens every now and then. So every
//! tick is judged on what the host cannot change, read on the tick thread
//! right at the start and the end of its work:
//! - its **CPU time**: how much the engine computed;
//! - its **voluntary context switches**: the thread gave up the CPU by itself
//!   (a contended lock, a sleep, I/O), which the tick must never do (SPEC §16).
//!
//! Wall-time statistics stay, over the whole run, to tell a machine that is too
//! busy for a meaningful soak; a witness thread that sleeps 1 ms in a loop sees
//! when the whole process was paused.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use klight_engine_core::{Clock, ThreadProbe, TickTiming};

/// A span of time on the soak's clock, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub from_us: u64,
    pub to_us: u64,
}

impl Interval {
    pub fn len_us(&self) -> u64 {
        self.to_us.saturating_sub(self.from_us)
    }
}

/// Counts of values in fixed-width buckets, for quantiles over a whole run
/// (not the worst of many short windows, where one slow sample in a window of
/// 44 already is its p99).
#[derive(Debug, Clone)]
pub struct Histogram {
    bucket_us: u64,
    counts: Vec<u64>,
    count: u64,
    max: u64,
}

impl Histogram {
    /// Buckets `bucket_us` wide up to `range_us`; larger values go to the last.
    pub fn new(bucket_us: u64, range_us: u64) -> Self {
        let bucket_us = bucket_us.max(1);
        let n = usize::try_from(range_us / bucket_us).unwrap_or(1).max(1);
        Self {
            bucket_us,
            counts: vec![0; n],
            count: 0,
            max: 0,
        }
    }

    pub fn record(&mut self, us: u64) {
        let last = self.counts.len().saturating_sub(1);
        let i = usize::try_from(us / self.bucket_us)
            .unwrap_or(last)
            .min(last);
        if let Some(c) = self.counts.get_mut(i) {
            *c += 1;
        }
        self.count += 1;
        self.max = self.max.max(us);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn max(&self) -> u64 {
        self.max
    }

    /// Upper bound of the bucket holding the `q` quantile (0 < q ≤ 1), never
    /// above the largest value seen. 0 when empty.
    pub fn quantile(&self, q: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let target = ((self.count as f64) * q).ceil().max(1.0) as u64;
        let mut acc = 0u64;
        for (i, &c) in self.counts.iter().enumerate() {
            acc += c;
            if acc >= target {
                let upper = (i as u64 + 1) * self.bucket_us;
                return upper.min(self.max);
            }
        }
        self.max
    }
}

/// Something the engine itself did wrong in one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// It computed for longer than the limit (CPU time, in us).
    Cpu(u64),
    /// It gave up the CPU by itself this many times during its work: a lock,
    /// a sleep or I/O inside the tick.
    Blocked(u64),
    /// No probe on this OS: its wall-time work was over the limit (in us).
    Wall(u64),
}

/// The engine's faults in one tick. With a probe: CPU time over `cpu_us` and
/// any voluntary context switch. Without one (other OSes): wall-time work
/// over `wall_us`, which the host can inflate.
pub fn tick_faults(t: &TickTiming, cpu_us: u64, wall_us: u64) -> Vec<Fault> {
    match t.probe {
        Some(p) => {
            let mut f = Vec::new();
            if p.cpu_us > cpu_us {
                f.push(Fault::Cpu(p.cpu_us));
            }
            if p.voluntary_switches > 0 {
                f.push(Fault::Blocked(p.voluntary_switches));
            }
            f
        }
        None if t.work_us > wall_us => vec![Fault::Wall(t.work_us)],
        None => Vec::new(),
    }
}

/// Reads the calling thread's CPU time (its CPU clock, precise to the
/// nanosecond; `getrusage` times are split by timer samples and too coarse for
/// one tick) and voluntary context switches (`getrusage` per thread), on Linux.
/// None where the OS does not tell.
pub fn thread_probe() -> Option<ThreadProbe> {
    #[cfg(target_os = "linux")]
    {
        use nix::sys::resource::{getrusage, UsageWho};
        use nix::time::{clock_gettime, ClockId};
        let read = || {
            let cpu = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).ok()?;
            let usage = getrusage(UsageWho::RUSAGE_THREAD).ok()?;
            let ns = u64::try_from(cpu.tv_sec()).ok()? * 1_000_000_000
                + u64::try_from(cpu.tv_nsec()).ok()?;
            Some(klight_engine_core::ThreadSample {
                cpu_us: ns / 1_000,
                voluntary_switches: u64::try_from(usage.voluntary_context_switches()).ok()?,
            })
        };
        read()?;
        Some(Box::new(read))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// A thread that sleeps `period_us` in a loop and records when it woke up
/// more than `over_us` late: the moments the whole process was not running.
pub struct Witness {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<Vec<Interval>>>,
}

impl Witness {
    pub fn spawn(clock: Arc<dyn Clock>, period_us: u64, over_us: u64) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::Builder::new()
            .name("soak-witness".into())
            .spawn(move || {
                let mut stalls = Vec::new();
                let mut due = clock.now_us().saturating_add(period_us);
                while !flag.load(Ordering::Relaxed) {
                    let now = clock.now_us();
                    if now < due {
                        std::thread::sleep(Duration::from_micros(due - now));
                    }
                    let woke = clock.now_us();
                    if woke.saturating_sub(due) > over_us {
                        stalls.push(Interval {
                            from_us: due,
                            to_us: woke,
                        });
                    }
                    due = woke.saturating_add(period_us);
                }
                stalls
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    /// Stops the thread and returns the stalls it saw, in order.
    pub fn stop(mut self) -> Vec<Interval> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use klight_engine_core::ProbeDelta;

    fn tick(work_us: u64, probe: Option<(u64, u64)>) -> TickTiming {
        TickTiming {
            due_us: 0,
            start_us: 50,
            work_us,
            probe: probe.map(|(cpu_us, voluntary_switches)| ProbeDelta {
                cpu_us,
                voluntary_switches,
            }),
        }
    }

    #[test]
    fn quantiles_over_the_whole_run_ignore_one_slow_sample() {
        let mut h = Histogram::new(10, 100_000);
        for _ in 0..79_000 {
            h.record(60);
        }
        h.record(5_150);
        assert_eq!(h.quantile(0.99), 70, "upper bound of the 60-70 us bucket");
        assert_eq!(h.quantile(1.0), 5_150);
        assert_eq!(h.max(), 5_150);
        assert_eq!(h.count(), 79_001);
    }

    #[test]
    fn quantiles_see_a_systematic_slowness() {
        let mut h = Histogram::new(10, 100_000);
        for i in 0..1_000 {
            h.record(if i % 20 == 0 { 6_000 } else { 50 });
        }
        assert!(h.quantile(0.99) >= 6_000, "1 tick in 20 is slow");
    }

    #[test]
    fn values_beyond_the_range_count_in_the_last_bucket() {
        let mut h = Histogram::new(10, 100);
        h.record(1_000_000);
        assert_eq!(h.quantile(0.5), 100);
        assert_eq!(h.max(), 1_000_000);
        assert_eq!(Histogram::new(10, 100).quantile(0.99), 0);
    }

    #[test]
    fn a_tick_slow_in_wall_time_only_is_not_the_engine() {
        // 6 ms of wall time, 150 us of CPU, never blocked: the host held it up.
        assert!(tick_faults(&tick(6_000, Some((150, 0))), 3_000, 3_000).is_empty());
    }

    #[test]
    fn a_tick_computing_too_long_is_the_engine() {
        assert_eq!(
            tick_faults(&tick(6_000, Some((5_800, 0))), 3_000, 3_000),
            vec![Fault::Cpu(5_800)]
        );
    }

    #[test]
    fn a_tick_that_blocks_is_the_engine_however_quick() {
        assert_eq!(
            tick_faults(&tick(200, Some((80, 1))), 3_000, 3_000),
            vec![Fault::Blocked(1)]
        );
    }

    #[test]
    fn without_a_probe_wall_time_is_all_there_is() {
        assert_eq!(
            tick_faults(&tick(6_000, None), 3_000, 3_000),
            vec![Fault::Wall(6_000)]
        );
        assert!(tick_faults(&tick(2_000, None), 3_000, 3_000).is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_probe_sees_cpu_time_and_sleeps() {
        let mut probe = thread_probe().expect("getrusage per thread on Linux");
        let a = probe().unwrap();
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(20) {
            std::hint::spin_loop();
        }
        let b = probe().unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let c = probe().unwrap();
        assert!(
            b.cpu_us - a.cpu_us >= 10_000,
            "spinning uses CPU: {a:?} {b:?}"
        );
        assert_eq!(
            b.voluntary_switches, a.voluntary_switches,
            "spinning never blocks"
        );
        assert!(
            c.voluntary_switches > b.voluntary_switches,
            "a sleep is a voluntary switch"
        );
        assert!(c.cpu_us - b.cpu_us < 2_000, "sleeping uses no CPU");
    }

    #[test]
    fn the_witness_sees_a_stalled_process() {
        // A clock that jumps forward once, as a paused VM would look.
        struct Jumping {
            real: klight_engine_core::MonotonicClock,
            jump: std::sync::atomic::AtomicU64,
        }
        impl Clock for Jumping {
            fn now_us(&self) -> u64 {
                self.real.now_us() + self.jump.load(Ordering::Relaxed)
            }
        }
        let clock = Arc::new(Jumping {
            real: klight_engine_core::MonotonicClock::new(),
            jump: std::sync::atomic::AtomicU64::new(0),
        });
        let w = Witness::spawn(clock.clone(), 1_000, 3_000).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        clock.jump.store(50_000, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(30));
        let stalls = w.stop();
        assert!(
            stalls.iter().any(|s| s.len_us() >= 45_000),
            "the jump is seen: {stalls:?}"
        );
    }
}
