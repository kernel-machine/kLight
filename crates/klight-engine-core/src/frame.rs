//! Lock-free "latest frame" slot between the tick loop (single writer) and
//! an output driver (reader). The writer never blocks and never allocates;
//! the reader always gets the most recent complete frame (seqlock built on
//! atomics, no `unsafe`).

use std::sync::atomic::{fence, AtomicU64, Ordering};

use klight_model::UNIVERSE_SIZE;

pub type DmxFrame = [u8; UNIVERSE_SIZE];

const WORDS: usize = UNIVERSE_SIZE / 8;

#[derive(Debug)]
pub struct FrameSlot {
    seq: AtomicU64,
    data: [AtomicU64; WORDS],
}

impl Default for FrameSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSlot {
    pub fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            data: [const { AtomicU64::new(0) }; WORDS],
        }
    }

    /// Publishes a frame. Must only be called from one thread at a time.
    pub fn write(&self, frame: &DmxFrame) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        for (word, chunk) in self.data.iter().zip(frame.chunks_exact(8)) {
            let bytes = <[u8; 8]>::try_from(chunk).unwrap_or([0; 8]);
            word.store(u64::from_le_bytes(bytes), Ordering::Relaxed);
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// Copies the latest complete frame into `out` and returns its version
    /// (even number, increases on every write).
    pub fn read(&self, out: &mut DmxFrame) -> u64 {
        loop {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            for (word, chunk) in self.data.iter().zip(out.chunks_exact_mut(8)) {
                chunk.copy_from_slice(&word.load(Ordering::Relaxed).to_le_bytes());
            }
            fence(Ordering::Acquire);
            let s2 = self.seq.load(Ordering::Relaxed);
            if s1 == s2 {
                return s1;
            }
        }
    }

    /// Version of the latest frame (cheap change detection).
    pub fn version(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn write_then_read() {
        let slot = FrameSlot::new();
        let mut f = [0u8; UNIVERSE_SIZE];
        f[0] = 1;
        f[511] = 255;
        slot.write(&f);
        let mut out = [0u8; UNIVERSE_SIZE];
        let v = slot.read(&mut out);
        assert_eq!(v, 2);
        assert_eq!(out, f);
    }

    #[test]
    fn concurrent_reads_never_tear() {
        let slot = Arc::new(FrameSlot::new());
        let writer = {
            let slot = slot.clone();
            std::thread::spawn(move || {
                for i in 0..20_000u32 {
                    let b = (i % 256) as u8;
                    slot.write(&[b; UNIVERSE_SIZE]);
                }
            })
        };
        let mut out = [0u8; UNIVERSE_SIZE];
        for _ in 0..20_000 {
            slot.read(&mut out);
            let first = out[0];
            assert!(out.iter().all(|&b| b == first), "torn frame");
        }
        writer.join().unwrap();
    }
}
