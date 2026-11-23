//! The output driver contract.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriverError {
    #[error("device not found{}", .0.as_ref().map(|s| format!(" (serial {s})")).unwrap_or_default())]
    NotFound(Option<String>),
    #[error("device not open")]
    NotOpen,
    #[error("I/O error: {0}")]
    Io(String),
    #[error("partial write: {written} of {expected} bytes")]
    PartialWrite { written: usize, expected: usize },
    #[error("operation timed out")]
    Timeout,
    #[error("driver not available on this build: {0}")]
    Unsupported(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum DriverState {
    #[default]
    Disconnected,
    Connecting,
    Ok,
    /// Errors happened; the manager is reconnecting with backoff.
    Degraded,
}

/// Measured timing of a software-generated DMX frame (Open DMX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct FrameTiming {
    pub break_us: u32,
    pub mab_us: u32,
    pub frame_us: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DriverStatus {
    pub state: DriverState,
    pub port: Option<String>,
    pub serial_number: Option<String>,
    /// The interface repeats the last frame on its own (Enttec Pro).
    pub hardware_buffer: bool,
    pub timing: Option<FrameTiming>,
}

/// One DMX output device. Implementations are used from a single
/// dedicated thread owned by the [`crate::DeviceManager`].
pub trait OutputDriver: Send {
    fn open(&mut self) -> Result<(), DriverError>;
    fn send_frame(&mut self, universe: &[u8; 512]) -> Result<(), DriverError>;
    fn close(&mut self);
    fn status(&self) -> DriverStatus;
    /// Minimum period between frames this device supports, in microseconds.
    fn min_frame_period_us(&self) -> u64 {
        22_727
    }
}
