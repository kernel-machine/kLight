//! USB serial device discovery. Devices are identified by serial number so
//! that a universe follows its interface across USB ports.

use serde::Serialize;

#[cfg(feature = "serial")]
use klight_model::OutputAssignment;

#[cfg(feature = "serial")]
use crate::driver::DriverError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortInfo {
    pub port: String,
    pub serial_number: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl PortInfo {
    /// FTDI FT232 (used by both Enttec Pro and Open DMX).
    pub fn is_ftdi(&self) -> bool {
        self.vid == Some(0x0403)
    }
}

#[cfg(feature = "serial")]
pub fn list_ports() -> Vec<PortInfo> {
    match serialport::available_ports() {
        Ok(ports) => ports
            .into_iter()
            .map(|p| match p.port_type {
                serialport::SerialPortType::UsbPort(u) => PortInfo {
                    port: p.port_name,
                    serial_number: u.serial_number,
                    vid: Some(u.vid),
                    pid: Some(u.pid),
                    manufacturer: u.manufacturer,
                    product: u.product,
                },
                _ => PortInfo {
                    port: p.port_name,
                    serial_number: None,
                    vid: None,
                    pid: None,
                    manufacturer: None,
                    product: None,
                },
            })
            .collect(),
        Err(e) => {
            tracing::warn!("cannot enumerate serial ports: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(feature = "serial"))]
pub fn list_ports() -> Vec<PortInfo> {
    Vec::new()
}

/// Finds the current port of an assignment: by serial number first, then
/// by port hint.
#[cfg(feature = "serial")]
pub fn resolve_port(a: &OutputAssignment) -> Result<String, DriverError> {
    let ports = list_ports();
    if let Some(serial) = &a.serial_number {
        return ports
            .iter()
            .find(|p| p.serial_number.as_deref() == Some(serial.as_str()))
            .map(|p| p.port.clone())
            .ok_or_else(|| DriverError::NotFound(Some(serial.clone())));
    }
    a.port_hint.clone().ok_or(DriverError::NotFound(None))
}
