//! Bridge between the input subsystem and the engine.

use std::sync::{Arc, OnceLock};

use crossbeam_channel::{Sender, TrySendError};
use klight_api::{ClientLink, ServerMessage};
use klight_engine_core::{Clock, EngineShared};
use klight_input::InputHost;
use klight_model::{ClientId, Command, CommandEnvelope, CommandSource, RuntimeSnapshot, Show};

pub struct EngineHost {
    pub shared: Arc<EngineShared>,
    pub commands: Sender<CommandEnvelope>,
    pub clock: Arc<dyn Clock>,
    pub link: Arc<OnceLock<ClientLink>>,
}

impl InputHost for EngineHost {
    fn show(&self) -> Arc<Show> {
        self.shared.show.load_full()
    }

    fn runtime(&self) -> Arc<RuntimeSnapshot> {
        self.shared.snapshot.load_full()
    }

    fn beat(&self) -> f64 {
        self.shared.beat.load(std::sync::atomic::Ordering::Relaxed) as f64
            / klight_engine_core::music::BEAT as f64
    }

    fn submit(&self, source: CommandSource, command: Command) {
        // Input results are not routed to anyone (id 0); a full queue drops
        // the event rather than blocking the input thread.
        if let Err(TrySendError::Full(_)) = self.commands.try_send(CommandEnvelope {
            id: 0,
            source,
            command,
        }) {
            tracing::warn!("command queue full: input event dropped");
        }
    }

    fn now_us(&self) -> u64 {
        self.clock.now_us()
    }

    fn send_bridge(&self, client: ClientId, device: &str, bytes: Vec<u8>) {
        if let Some(l) = self.link.get() {
            l.send(
                client,
                ServerMessage::MidiOut {
                    seq: 0,
                    device: device.to_owned(),
                    bytes,
                },
            );
        }
    }
}
