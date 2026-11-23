//! Input manager: device threads (MIDI with hot-plug, OSC), mapping to
//! commands, MIDI learn and controller feedback.

use std::collections::BTreeMap;
use std::net::UdpSocket;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use klight_model::input_map::{InputDeviceKind, InputMessage};
use klight_model::{ClientId, Command, CommandSource, RuntimeSnapshot, Show};

use crate::feedback::{ControllerProfile, FeedbackTracker};
use crate::mapping::{InputEvent, Mapper};
use crate::midi::{self, MidiEvent};

/// What the input subsystem needs from the engine process.
pub trait InputHost: Send + Sync + 'static {
    fn show(&self) -> Arc<Show>;
    fn runtime(&self) -> Arc<RuntimeSnapshot>;
    fn submit(&self, source: CommandSource, command: Command);
    fn now_us(&self) -> u64;
    /// Feedback for a controller attached to a remote client.
    fn send_bridge(&self, client: ClientId, device: &str, bytes: Vec<u8>);
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InputDeviceStatus {
    pub name: String,
    pub kind: String,
    pub connected: bool,
}

/// Result of MIDI learn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Learned {
    pub device: String,
    pub message: InputMessage,
}

type LearnReply = Box<dyn FnOnce(Learned) + Send>;

enum Raw {
    Midi {
        device: String,
        bytes: Vec<u8>,
        at_us: u64,
    },
    Osc {
        device: String,
        bytes: Vec<u8>,
    },
    Bridge {
        client: ClientId,
        device: String,
        bytes: Vec<u8>,
    },
    Connected {
        device: String,
    },
}

#[derive(Default)]
struct Shared {
    statuses: Mutex<BTreeMap<String, InputDeviceStatus>>,
    learn: Mutex<Option<LearnReply>>,
    profiles: Mutex<BTreeMap<String, ControllerProfile>>,
    #[cfg(feature = "midi")]
    outputs: Mutex<BTreeMap<String, midir::MidiOutputConnection>>,
    bridges: Mutex<BTreeMap<String, ClientId>>,
}

pub struct InputManager {
    stop: Arc<AtomicBool>,
    tx: Sender<Raw>,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl InputManager {
    pub fn start<H: InputHost>(
        host: Arc<H>,
        profiles: BTreeMap<String, ControllerProfile>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded::<Raw>(4096);
        let shared = Arc::new(Shared::default());
        if let Ok(mut p) = shared.profiles.lock() {
            *p = profiles;
        }
        let mut threads = Vec::new();
        {
            let (stop, shared, host) = (stop.clone(), shared.clone(), host.clone());
            threads.push(
                std::thread::Builder::new()
                    .name("klight-input".into())
                    .spawn(move || {
                        guarded("input mapper", &stop, || {
                            mapper_loop(&*host, &rx, &shared, &stop)
                        });
                    })?,
            );
        }
        #[cfg(feature = "midi")]
        {
            let (stop, shared, tx, host) = (stop.clone(), shared.clone(), tx.clone(), host.clone());
            threads.push(
                std::thread::Builder::new()
                    .name("klight-midi".into())
                    .spawn(move || {
                        guarded("midi scanner", &stop, || {
                            midi_scan_loop(&*host, &tx, &shared, &stop)
                        });
                    })?,
            );
        }
        {
            let (stop, shared, tx, host) = (stop.clone(), shared.clone(), tx.clone(), host.clone());
            threads.push(
                std::thread::Builder::new()
                    .name("klight-osc".into())
                    .spawn(move || {
                        guarded("osc", &stop, || osc_supervisor(&*host, &tx, &shared, &stop));
                    })?,
            );
        }
        Ok(Self {
            stop,
            tx,
            shared,
            threads,
        })
    }

    /// The next message from any device is reported to `reply`.
    pub fn learn(&self, reply: LearnReply) {
        if let Ok(mut l) = self.shared.learn.lock() {
            *l = Some(reply);
        }
    }

    /// Raw MIDI forwarded by a remote client acting as a bridge.
    pub fn bridge_input(&self, client: ClientId, device: &str, bytes: &[u8]) {
        let _ = self.tx.try_send(Raw::Bridge {
            client,
            device: device.to_owned(),
            bytes: bytes.to_vec(),
        });
    }

    pub fn statuses(&self) -> Vec<InputDeviceStatus> {
        self.shared
            .statuses
            .lock()
            .map(|s| s.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for InputManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Runs `f` and restarts it after a panic (last line of defence).
fn guarded(name: &str, stop: &AtomicBool, mut f: impl FnMut()) {
    while !stop.load(Ordering::Relaxed) {
        if catch_unwind(AssertUnwindSafe(&mut f)).is_ok() {
            return;
        }
        tracing::error!("{name} thread panicked; restarting");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn set_status(shared: &Shared, name: &str, kind: &str, connected: bool) {
    if let Ok(mut s) = shared.statuses.lock() {
        s.insert(
            name.to_owned(),
            InputDeviceStatus {
                name: name.to_owned(),
                kind: kind.to_owned(),
                connected,
            },
        );
    }
}

fn mapper_loop<H: InputHost + ?Sized>(
    host: &H,
    rx: &Receiver<Raw>,
    shared: &Shared,
    stop: &AtomicBool,
) {
    let mut mapper = Mapper::new();
    let mut feedback: BTreeMap<String, FeedbackTracker> = BTreeMap::new();
    let mut last_feedback = Instant::now();
    let mut last_show: Option<Arc<Show>> = None;
    let start = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(raw) => {
                let show = host.show();
                let rt = host.runtime();
                let (device, events, source, at_us) = match raw {
                    Raw::Midi {
                        device,
                        bytes,
                        at_us,
                    } => {
                        let src = CommandSource::Midi {
                            device: device.clone(),
                        };
                        (
                            device,
                            midi::parse(&bytes)
                                .into_iter()
                                .map(InputEvent::Midi)
                                .collect::<Vec<_>>(),
                            src,
                            at_us,
                        )
                    }
                    Raw::Osc { device, bytes } => {
                        let src = CommandSource::Osc {
                            device: device.clone(),
                        };
                        (device, crate::osc::decode(&bytes), src, host.now_us())
                    }
                    Raw::Bridge {
                        client,
                        device,
                        bytes,
                    } => {
                        if let Ok(mut b) = shared.bridges.lock() {
                            b.insert(device.clone(), client);
                        }
                        let src = CommandSource::Remote { client };
                        (
                            device,
                            midi::parse(&bytes)
                                .into_iter()
                                .map(InputEvent::Midi)
                                .collect::<Vec<_>>(),
                            src,
                            host.now_us(),
                        )
                    }
                    Raw::Connected { device } => {
                        mapper.reset_device(&device);
                        if let Some(f) = feedback.get_mut(&device) {
                            f.resync();
                        }
                        continue;
                    }
                };
                for ev in events {
                    if let InputEvent::Midi(MidiEvent::Clock) = ev {
                        host.submit(
                            CommandSource::TimeSource {
                                name: format!("midi:{device}"),
                            },
                            Command::ClockPulse { at_us },
                        );
                        continue;
                    }
                    if let Some((message, _)) = ev.key_value() {
                        let reply = shared.learn.lock().ok().and_then(|mut l| l.take());
                        if let Some(reply) = reply {
                            reply(Learned {
                                device: device.clone(),
                                message,
                            });
                            continue;
                        }
                    }
                    for cmd in mapper.translate(&show, &rt, &device, &ev) {
                        host.submit(source.clone(), cmd);
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }

        // Feedback every 20 ms, only differences.
        if last_feedback.elapsed() >= Duration::from_millis(20) {
            last_feedback = Instant::now();
            let show = host.show();
            let rt = host.runtime();
            let blink_on = (start.elapsed().as_millis() / 250).is_multiple_of(2);
            let show_changed = last_show.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &show));
            if show_changed {
                for f in feedback.values_mut() {
                    f.resync();
                }
                last_show = Some(show.clone());
            }
            let profiles = shared
                .profiles
                .lock()
                .map(|p| p.clone())
                .unwrap_or_default();
            for dev in &show.input_devices {
                let Some(profile) = dev.profile.as_ref().and_then(|p| profiles.get(p)) else {
                    continue;
                };
                let limit = usize::try_from(profile.max_messages_per_second / 50)
                    .unwrap_or(10)
                    .max(1);
                let tracker = feedback.entry(dev.name.clone()).or_default();
                let msgs = tracker.update(&show, &rt, &dev.name, profile, blink_on, limit);
                for bytes in msgs {
                    send_feedback(host, shared, &dev.name, bytes);
                }
            }
        }
    }
}

fn send_feedback<H: InputHost + ?Sized>(host: &H, shared: &Shared, device: &str, bytes: Vec<u8>) {
    let bridge = shared
        .bridges
        .lock()
        .ok()
        .and_then(|b| b.get(device).copied());
    if let Some(client) = bridge {
        host.send_bridge(client, device, bytes);
        return;
    }
    #[cfg(feature = "midi")]
    if let Ok(mut outs) = shared.outputs.lock() {
        if let Some(conn) = outs.get_mut(device) {
            if let Err(e) = conn.send(&bytes) {
                tracing::warn!(device, "feedback send failed: {e}");
            }
        }
    }
}

#[cfg(feature = "midi")]
fn midi_scan_loop<H: InputHost + ?Sized>(
    host: &H,
    tx: &Sender<Raw>,
    shared: &Shared,
    stop: &AtomicBool,
) {
    use midir::{MidiInput, MidiInputConnection, MidiOutput};

    let mut inputs: BTreeMap<String, MidiInputConnection<()>> = BTreeMap::new();
    while !stop.load(Ordering::Relaxed) {
        // Inputs: connect every port (learn works with any controller);
        // devices are recognised by name after hot-plug.
        if let Ok(probe) = MidiInput::new("klight-probe") {
            let ports = probe.ports();
            let names: Vec<(String, usize)> = ports
                .iter()
                .enumerate()
                .filter_map(|(i, p)| probe.port_name(p).ok().map(|n| (n, i)))
                .collect();
            // Disconnected devices.
            let gone: Vec<String> = inputs
                .keys()
                .filter(|k| !names.iter().any(|(n, _)| n == *k))
                .cloned()
                .collect();
            for g in gone {
                inputs.remove(&g);
                if let Ok(mut o) = shared.outputs.lock() {
                    o.remove(&g);
                }
                set_status(shared, &g, "midi", false);
                tracing::warn!(device = g, "MIDI device disconnected");
            }
            for (name, idx) in names {
                if inputs.contains_key(&name) || name.contains("klight") {
                    continue;
                }
                let Ok(input) = MidiInput::new("klight") else {
                    continue;
                };
                let Some(port) = input.ports().get(idx).cloned() else {
                    continue;
                };
                let tx2 = tx.clone();
                let dev = name.clone();
                let clock_origin = host.now_us();
                let origin = Instant::now();
                match input.connect(
                    &port,
                    "klight-in",
                    move |_stamp, bytes, _| {
                        let at_us =
                            clock_origin + u64::try_from(origin.elapsed().as_micros()).unwrap_or(0);
                        let _ = tx2.try_send(Raw::Midi {
                            device: dev.clone(),
                            bytes: bytes.to_vec(),
                            at_us,
                        });
                    },
                    (),
                ) {
                    Ok(conn) => {
                        tracing::info!(device = name, "MIDI device connected");
                        inputs.insert(name.clone(), conn);
                        set_status(shared, &name, "midi", true);
                        let _ = tx.try_send(Raw::Connected {
                            device: name.clone(),
                        });
                        // Matching output for feedback.
                        if let Ok(out) = MidiOutput::new("klight") {
                            let port = out
                                .ports()
                                .into_iter()
                                .find(|p| out.port_name(p).ok().as_deref() == Some(name.as_str()));
                            if let Some(p) = port {
                                if let Ok(c) = out.connect(&p, "klight-out") {
                                    if let Ok(mut o) = shared.outputs.lock() {
                                        o.insert(name.clone(), c);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!(device = name, "cannot open MIDI input: {e}"),
                }
            }
        }
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Keeps one listener thread per configured OSC device.
fn osc_supervisor<H: InputHost + ?Sized>(
    host: &H,
    tx: &Sender<Raw>,
    shared: &Shared,
    stop: &AtomicBool,
) {
    let mut sockets: BTreeMap<String, (u16, UdpSocket)> = BTreeMap::new();
    let mut buf = vec![0u8; 65_536];
    let mut last_check = Instant::now() - Duration::from_secs(10);
    while !stop.load(Ordering::Relaxed) {
        if last_check.elapsed() >= Duration::from_secs(1) {
            last_check = Instant::now();
            let show = host.show();
            let wanted: BTreeMap<String, u16> = show
                .input_devices
                .iter()
                .filter_map(|d| match d.kind {
                    InputDeviceKind::Osc { port } => Some((d.name.clone(), port)),
                    InputDeviceKind::Midi => None,
                })
                .collect();
            sockets.retain(|name, (port, _)| wanted.get(name) == Some(port));
            for (name, port) in wanted {
                if sockets.contains_key(&name) {
                    continue;
                }
                match UdpSocket::bind(("0.0.0.0", port)).and_then(|s| {
                    s.set_nonblocking(true)?;
                    Ok(s)
                }) {
                    Ok(s) => {
                        set_status(shared, &name, "osc", true);
                        sockets.insert(name, (port, s));
                    }
                    Err(e) => {
                        set_status(shared, &name, "osc", false);
                        tracing::warn!(device = name, port, "cannot bind OSC port: {e}");
                    }
                }
            }
        }
        let mut any = false;
        for (name, (_, sock)) in &sockets {
            while let Ok((n, _)) = sock.recv_from(&mut buf) {
                any = true;
                let _ = tx.try_send(Raw::Osc {
                    device: name.clone(),
                    bytes: buf.get(..n).map(<[u8]>::to_vec).unwrap_or_default(),
                });
            }
        }
        if !any {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
