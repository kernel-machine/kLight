//! In-memory driver that records every frame and can simulate faults.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::driver::{DriverError, DriverState, DriverStatus, OutputDriver};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedFrame {
    /// Microseconds since the recorder was created.
    pub at_us: u64,
    pub data: [u8; 512],
}

/// Faults to inject. Shared so tests can change them while the driver runs.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// Device is unplugged: open and send fail.
    pub unplugged: bool,
    /// Every send sleeps this long (slow device).
    pub latency: Option<Duration>,
    /// Next N sends fail with an I/O error.
    pub fail_next_sends: u32,
    /// Next N sends write only part of the frame.
    pub partial_next_sends: u32,
    /// Next N sends panic (tests the panic guard).
    pub panic_next_sends: u32,
}

#[derive(Debug)]
pub struct Recorder {
    origin: Instant,
    pub frames: Mutex<Vec<RecordedFrame>>,
    pub faults: Mutex<Faults>,
    pub opens: Mutex<u32>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            frames: Mutex::new(Vec::new()),
            faults: Mutex::new(Faults::default()),
            opens: Mutex::new(0),
        }
    }
}

impl Recorder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn frames(&self) -> Vec<RecordedFrame> {
        self.frames.lock().map(|f| f.clone()).unwrap_or_default()
    }

    pub fn last(&self) -> Option<RecordedFrame> {
        self.frames.lock().ok().and_then(|f| f.last().cloned())
    }

    pub fn count(&self) -> usize {
        self.frames.lock().map(|f| f.len()).unwrap_or(0)
    }

    pub fn set_faults(&self, f: impl FnOnce(&mut Faults)) {
        if let Ok(mut faults) = self.faults.lock() {
            f(&mut faults);
        }
    }

    pub fn open_count(&self) -> u32 {
        self.opens.lock().map(|o| *o).unwrap_or(0)
    }
}

pub struct FakeOutput {
    recorder: Arc<Recorder>,
    open: bool,
    name: String,
}

impl FakeOutput {
    pub fn new(name: &str, recorder: Arc<Recorder>) -> Self {
        Self {
            recorder,
            open: false,
            name: name.to_owned(),
        }
    }
}

impl OutputDriver for FakeOutput {
    fn open(&mut self) -> Result<(), DriverError> {
        let unplugged = self
            .recorder
            .faults
            .lock()
            .map(|f| f.unplugged)
            .unwrap_or(false);
        if unplugged {
            return Err(DriverError::NotFound(Some(self.name.clone())));
        }
        if let Ok(mut o) = self.recorder.opens.lock() {
            *o += 1;
        }
        self.open = true;
        Ok(())
    }

    fn send_frame(&mut self, universe: &[u8; 512]) -> Result<(), DriverError> {
        if !self.open {
            return Err(DriverError::NotOpen);
        }
        let (latency, fail, partial, panic_now, unplugged) = match self.recorder.faults.lock() {
            Ok(mut f) => {
                let fail = f.fail_next_sends > 0;
                f.fail_next_sends = f.fail_next_sends.saturating_sub(u32::from(fail));
                let partial = !fail && f.partial_next_sends > 0;
                f.partial_next_sends = f.partial_next_sends.saturating_sub(u32::from(partial));
                let panic_now = f.panic_next_sends > 0;
                f.panic_next_sends = f.panic_next_sends.saturating_sub(u32::from(panic_now));
                (f.latency, fail, partial, panic_now, f.unplugged)
            }
            Err(_) => (None, false, false, false, false),
        };
        if let Some(l) = latency {
            std::thread::sleep(l);
        }
        #[allow(clippy::panic)]
        if panic_now {
            panic!("injected driver panic");
        }
        if unplugged {
            self.open = false;
            return Err(DriverError::Io("device unplugged".into()));
        }
        if fail {
            return Err(DriverError::Io("injected failure".into()));
        }
        if partial {
            return Err(DriverError::PartialWrite {
                written: 100,
                expected: 518,
            });
        }
        let at_us = u64::try_from(self.recorder.origin.elapsed().as_micros()).unwrap_or(u64::MAX);
        if let Ok(mut frames) = self.recorder.frames.lock() {
            frames.push(RecordedFrame {
                at_us,
                data: *universe,
            });
        }
        Ok(())
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn status(&self) -> DriverStatus {
        DriverStatus {
            state: if self.open {
                DriverState::Ok
            } else {
                DriverState::Disconnected
            },
            port: Some(format!("fake:{}", self.name)),
            serial_number: Some(self.name.clone()),
            hardware_buffer: true,
            timing: None,
        }
    }

    fn min_frame_period_us(&self) -> u64 {
        1000
    }
}
