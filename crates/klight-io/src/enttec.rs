//! Enttec DMX USB Pro and Open DMX drivers.

/// Label of the "Output Only Send DMX Packet" request.
pub const LABEL_SEND_DMX: u8 = 6;
const START: u8 = 0x7E;
const END: u8 = 0xE7;
/// Size of a Pro "send DMX" packet for a full universe.
pub const PRO_PACKET_LEN: usize = 4 + 513 + 1;

/// Encodes a full universe as an Enttec Pro packet:
/// `0x7E, label, len LSB, len MSB, start code 0x00, 512 channels, 0xE7`.
pub fn encode_pro_packet(universe: &[u8; 512], out: &mut [u8; PRO_PACKET_LEN]) {
    let len: u16 = 513;
    let [lsb, msb] = len.to_le_bytes();
    let (header, rest) = out.split_at_mut(5);
    header.copy_from_slice(&[START, LABEL_SEND_DMX, lsb, msb, 0x00]);
    let (data, tail) = rest.split_at_mut(512);
    data.copy_from_slice(universe);
    if let Some(end) = tail.first_mut() {
        *end = END;
    }
}

#[cfg(feature = "serial")]
pub use serial::{EnttecPro, OpenDmx};

#[cfg(feature = "serial")]
mod serial {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use serialport::{DataBits, Parity, SerialPort, StopBits};

    use super::{encode_pro_packet, PRO_PACKET_LEN};
    use crate::driver::{DriverError, DriverState, DriverStatus, FrameTiming, OutputDriver};

    fn io_err(e: impl std::fmt::Display) -> DriverError {
        DriverError::Io(e.to_string())
    }

    /// Enttec DMX USB Pro: the widget repeats the last frame by itself, so
    /// the stage keeps its look even if the engine stops.
    pub struct EnttecPro {
        port_name: String,
        serial_number: Option<String>,
        port: Option<Box<dyn SerialPort>>,
        packet: [u8; PRO_PACKET_LEN],
    }

    impl EnttecPro {
        pub fn new(port_name: &str, serial_number: Option<String>) -> Self {
            Self {
                port_name: port_name.to_owned(),
                serial_number,
                port: None,
                packet: [0; PRO_PACKET_LEN],
            }
        }
    }

    impl OutputDriver for EnttecPro {
        fn open(&mut self) -> Result<(), DriverError> {
            let port = serialport::new(&self.port_name, 115_200)
                .timeout(Duration::from_millis(200))
                .open()
                .map_err(io_err)?;
            self.port = Some(port);
            Ok(())
        }

        fn send_frame(&mut self, universe: &[u8; 512]) -> Result<(), DriverError> {
            let port = self.port.as_mut().ok_or(DriverError::NotOpen)?;
            encode_pro_packet(universe, &mut self.packet);
            port.write_all(&self.packet).map_err(io_err)?;
            port.flush().map_err(io_err)
        }

        fn close(&mut self) {
            self.port = None;
        }

        fn status(&self) -> DriverStatus {
            DriverStatus {
                state: if self.port.is_some() {
                    DriverState::Ok
                } else {
                    DriverState::Disconnected
                },
                port: Some(self.port_name.clone()),
                serial_number: self.serial_number.clone(),
                hardware_buffer: true,
                timing: None,
            }
        }
    }

    /// Enttec Open DMX (bare FTDI): the PC generates break, MAB and the
    /// frame. If the engine stops, the output stops ("no hardware buffer").
    pub struct OpenDmx {
        port_name: String,
        serial_number: Option<String>,
        port: Option<Box<dyn SerialPort>>,
        frame: [u8; 513],
        timing: FrameTiming,
    }

    impl OpenDmx {
        pub fn new(port_name: &str, serial_number: Option<String>) -> Self {
            Self {
                port_name: port_name.to_owned(),
                serial_number,
                port: None,
                frame: [0; 513],
                timing: FrameTiming::default(),
            }
        }
    }

    fn spin_for(d: Duration) {
        let start = Instant::now();
        while start.elapsed() < d {
            std::hint::spin_loop();
        }
    }

    impl OutputDriver for OpenDmx {
        fn open(&mut self) -> Result<(), DriverError> {
            let port = serialport::new(&self.port_name, 250_000)
                .data_bits(DataBits::Eight)
                .stop_bits(StopBits::Two)
                .parity(Parity::None)
                .timeout(Duration::from_millis(100))
                .open()
                .map_err(io_err)?;
            self.port = Some(port);
            Ok(())
        }

        fn send_frame(&mut self, universe: &[u8; 512]) -> Result<(), DriverError> {
            let port = self.port.as_mut().ok_or(DriverError::NotOpen)?;
            let t0 = Instant::now();
            // Break >= 88 us (we aim for ~110 us; sleep granularity is coarse).
            port.set_break().map_err(io_err)?;
            spin_for(Duration::from_micros(110));
            port.clear_break().map_err(io_err)?;
            let t1 = Instant::now();
            // Mark after break >= 8 us.
            spin_for(Duration::from_micros(12));
            let t2 = Instant::now();
            let (start, data) = self.frame.split_at_mut(1);
            if let Some(s) = start.first_mut() {
                *s = 0x00;
            }
            data.copy_from_slice(universe);
            port.write_all(&self.frame).map_err(io_err)?;
            port.flush().map_err(io_err)?;
            let t3 = Instant::now();
            let us = |d: Duration| u32::try_from(d.as_micros()).unwrap_or(u32::MAX);
            self.timing = FrameTiming {
                break_us: us(t1 - t0),
                mab_us: us(t2 - t1),
                frame_us: us(t3 - t0),
            };
            Ok(())
        }

        fn close(&mut self) {
            self.port = None;
        }

        fn status(&self) -> DriverStatus {
            DriverStatus {
                state: if self.port.is_some() {
                    DriverState::Ok
                } else {
                    DriverState::Disconnected
                },
                port: Some(self.port_name.clone()),
                serial_number: self.serial_number.clone(),
                hardware_buffer: false,
                timing: Some(self.timing),
            }
        }

        fn min_frame_period_us(&self) -> u64 {
            // 513 slots at 44 us + break/MAB: ~22.8 ms per frame.
            23_000
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pro_packet_layout() {
        let mut u = [0u8; 512];
        u[0] = 0xAA;
        u[511] = 0x55;
        let mut p = [0u8; PRO_PACKET_LEN];
        encode_pro_packet(&u, &mut p);
        assert_eq!(&p[..5], &[0x7E, 6, 0x01, 0x02, 0x00]);
        assert_eq!(p[5], 0xAA);
        assert_eq!(p[516], 0x55);
        assert_eq!(p[517], 0xE7);
        assert_eq!(p.len(), 518);
    }
}
