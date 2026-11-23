//! Output drivers and the device manager.
//!
//! Every driver runs on its own thread; a failing or panicking driver only
//! affects its own universe and is reconnected with exponential backoff.

pub mod discovery;
pub mod driver;
pub mod enttec;
pub mod fake;
pub mod manager;

pub use driver::{DriverError, DriverState, DriverStatus, FrameTiming, OutputDriver};
pub use fake::{FakeOutput, Faults, RecordedFrame, Recorder};
pub use manager::{
    DeviceManager, DriverFactory, FakeFactory, ManagerConfig, OutputStatus, SystemFactory,
};
