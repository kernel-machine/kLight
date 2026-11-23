//! WebSocket server. Talks to the engine only through the command queue and
//! the published snapshots, so it can never block the tick loop.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use futures_util::{SinkExt, StreamExt};
use klight_engine_core::EngineShared;
use klight_model::{ClientId, CommandEnvelope, CommandResult, CommandSource, UniverseId};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::*;

#[derive(Debug, Clone)]
pub struct TokenGrant {
    pub token: String,
    pub permission: Permission,
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub bind: SocketAddr,
    /// Directory with the built web UI, served to remote clients.
    pub static_dir: Option<PathBuf>,
    pub tokens: Vec<TokenGrant>,
    /// Permission for loopback clients (the local desktop UI).
    pub local_permission: Permission,
    /// Fixture definitions available for patching.
    pub library: Arc<Library>,
}

pub type StatusFn = Arc<dyn Fn() -> SystemStatus + Send + Sync>;
pub type MidiInFn = Arc<dyn Fn(ClientId, &str, &[u8]) + Send + Sync>;
pub type LearnFn = Arc<dyn Fn(ClientId) + Send + Sync>;
pub type PortsFn = Arc<dyn Fn() -> Vec<SerialPortInfo> + Send + Sync>;
pub type InputProfilesFn = Arc<dyn Fn() -> Vec<InputProfileInfo> + Send + Sync>;

#[derive(Clone)]
pub struct ApiHooks {
    pub status: StatusFn,
    pub midi_in: Option<MidiInFn>,
    pub learn: Option<LearnFn>,
    pub ports: Option<PortsFn>,
    pub input_profiles: Option<InputProfilesFn>,
}

const CLIENT_QUEUE: usize = 1024;

struct ClientHandle {
    tx: mpsc::Sender<ServerMessage>,
    seq: AtomicU64,
    dropped: AtomicU64,
    dmx: Mutex<Vec<UniverseId>>,
}

#[derive(Default)]
struct Hub {
    clients: Mutex<BTreeMap<ClientId, Arc<ClientHandle>>>,
    pending: Mutex<HashMap<u64, (ClientId, u64)>>,
    next_cmd: AtomicU64,
    next_client: AtomicU32,
}

impl Hub {
    fn client(&self, id: ClientId) -> Option<Arc<ClientHandle>> {
        self.clients.lock().ok().and_then(|c| c.get(&id).cloned())
    }

    fn all(&self) -> Vec<(ClientId, Arc<ClientHandle>)> {
        self.clients
            .lock()
            .map(|c| c.iter().map(|(k, v)| (*k, v.clone())).collect())
            .unwrap_or_default()
    }

    fn count(&self) -> u32 {
        self.clients
            .lock()
            .map(|c| u32::try_from(c.len()).unwrap_or(u32::MAX))
            .unwrap_or(0)
    }

    /// Never blocks: a full client queue drops the message (the gap in
    /// `seq` tells the client to resync).
    fn send(&self, handle: &ClientHandle, mut msg: ServerMessage) {
        let seq = handle.seq.fetch_add(1, Ordering::Relaxed) + 1;
        msg.set_seq(seq);
        if let Err(mpsc::error::TrySendError::Full(_)) = handle.tx.try_send(msg) {
            handle.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Clone)]
struct AppState {
    hub: Arc<Hub>,
    shared: Arc<EngineShared>,
    commands: Sender<CommandEnvelope>,
    config: Arc<ApiConfig>,
    hooks: ApiHooks,
}

pub struct ApiServer {
    pub local_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    hub: Arc<Hub>,
}

impl ApiServer {
    pub fn client_count(&self) -> u32 {
        self.hub.count()
    }

    /// Sends controller feedback to a remote MIDI bridge client.
    pub fn send_midi(&self, client: ClientId, device: &str, bytes: Vec<u8>) {
        if let Some(h) = self.hub.client(client) {
            self.hub.send(
                &h,
                ServerMessage::MidiOut {
                    seq: 0,
                    device: device.to_owned(),
                    bytes,
                },
            );
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
    }
}

/// Handle usable from any thread to reach connected clients.
#[derive(Clone)]
pub struct ClientLink(Arc<Hub>);

impl ClientLink {
    pub fn count(&self) -> u32 {
        self.0.count()
    }

    /// Sends a message to one client (never blocks; dropped if the client is slow).
    pub fn send(&self, client: ClientId, msg: ServerMessage) {
        if let Some(h) = self.0.client(client) {
            self.0.send(&h, msg);
        }
    }
}

impl ApiServer {
    pub fn link(&self) -> ClientLink {
        ClientLink(self.hub.clone())
    }
}

/// Starts the server. Must be called inside a Tokio runtime.
pub async fn serve(
    config: ApiConfig,
    shared: Arc<EngineShared>,
    commands: Sender<CommandEnvelope>,
    results: Receiver<CommandResult>,
    hooks: ApiHooks,
) -> std::io::Result<ApiServer> {
    let hub = Arc::new(Hub::default());
    let state = AppState {
        hub: hub.clone(),
        shared: shared.clone(),
        commands,
        config: Arc::new(config.clone()),
        hooks,
    };

    // Command results → clients (blocking receiver on its own thread).
    {
        let hub = hub.clone();
        std::thread::Builder::new()
            .name("klight-api-results".into())
            .spawn(move || {
                while let Ok(r) = results.recv() {
                    let target = hub.pending.lock().ok().and_then(|mut p| p.remove(&r.id));
                    let Some((client, id)) = target else { continue };
                    let Some(h) = hub.client(client) else {
                        continue;
                    };
                    let msg = match r.result {
                        Ok(o) => ServerMessage::Ack {
                            seq: 0,
                            id,
                            warnings: o.warnings,
                            show_seq: o.show_seq,
                        },
                        Err(error) => ServerMessage::Error {
                            seq: 0,
                            id: Some(id),
                            error: ApiError::Command { error },
                        },
                    };
                    hub.send(&h, msg);
                }
            })?;
    }

    tokio::spawn(broadcast_loop(state.clone()));

    let mut app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/health", get(health))
        .route("/api/library", get(library))
        .route("/api/library/definition", get(library_definition))
        .route("/api/runtime", get(runtime_state))
        .route("/api/ports", get(ports))
        .route("/api/input-profiles", get(input_profiles));
    if let Some(dir) = &config.static_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(
            tower_http::services::ServeDir::new(dir)
                .fallback(tower_http::services::ServeFile::new(index)),
        );
    }
    // Static files (UI, WebAssembly module, JSON) are compressed for remote clients.
    let app = app
        .with_state(state)
        .layer(tower_http::compression::CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let local_addr = listener.local_addr()?;
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = server.await {
            tracing::error!("API server error: {e}");
        }
    });
    tracing::info!("API listening on {local_addr}");
    Ok(ApiServer {
        local_addr,
        shutdown: Some(tx),
        task: Some(task),
        hub,
    })
}

async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let stats = s.shared.stats.snapshot();
    Json(serde_json::json!({ "ok": true, "ticks": stats.ticks, "clients": s.hub.count() }))
}

async fn ports(State(s): State<AppState>) -> impl IntoResponse {
    // Enumeration may touch the OS: keep it off the async workers.
    let f = s.hooks.ports.clone();
    let list = tokio::task::spawn_blocking(move || f.map(|f| f()).unwrap_or_default())
        .await
        .unwrap_or_default();
    Json(list)
}

async fn input_profiles(State(s): State<AppState>) -> impl IntoResponse {
    let list: Vec<InputProfileInfo> = s
        .hooks
        .input_profiles
        .as_ref()
        .map(|f| f())
        .unwrap_or_default();
    Json(list)
}

async fn runtime_state(State(s): State<AppState>) -> impl IntoResponse {
    Json(s.shared.snapshot.load().as_ref().clone())
}

async fn library(State(s): State<AppState>) -> impl IntoResponse {
    let entries: Vec<LibraryEntry> = s.config.library.iter().map(LibraryEntry::of).collect();
    Json(entries)
}

async fn library_definition(
    State(s): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> axum::response::Response {
    let key = q.get("key").map(String::as_str).unwrap_or("");
    match s.config.library.iter().find(|d| d.key().0 == key) {
        Some(d) => Json(d.clone()).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            "unknown fixture definition",
        )
            .into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(s): State<AppState>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(socket, addr, s))
}

fn full_snapshot(s: &AppState) -> ServerMessage {
    ServerMessage::Snapshot {
        seq: 0,
        show_seq: s.shared.show_seq.load(Ordering::Relaxed),
        show: Box::new(s.shared.show.load().as_ref().clone()),
        runtime: Box::new(s.shared.snapshot.load().as_ref().clone()),
        status: status_with_clients(s),
    }
}

fn status_with_clients(s: &AppState) -> SystemStatus {
    let mut st = (s.hooks.status)();
    st.clients = s.hub.count();
    st
}

async fn send_direct(socket: &mut WebSocket, msg: &ServerMessage) -> bool {
    match serde_json::to_string(msg) {
        Ok(text) => socket.send(Message::Text(text.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn connection(mut socket: WebSocket, addr: SocketAddr, s: AppState) {
    let local = addr.ip().is_loopback();

    // Handshake: first message must be `hello`.
    let hello = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    let token = match hello {
        Ok(Some(Ok(Message::Text(t)))) => match parse_client_message(t.as_str()) {
            Ok(ClientMessage::Hello { token, .. }) => token,
            _ => {
                let _ = send_direct(
                    &mut socket,
                    &ServerMessage::Error {
                        seq: 0,
                        id: None,
                        error: ApiError::Malformed {
                            reason: "expected hello".into(),
                        },
                    },
                )
                .await;
                return;
            }
        },
        _ => return,
    };
    let permission = if local {
        Some(s.config.local_permission)
    } else {
        token.and_then(|t| {
            s.config
                .tokens
                .iter()
                .find(|g| g.token == t)
                .map(|g| g.permission)
        })
    };
    let Some(permission) = permission else {
        let _ = send_direct(
            &mut socket,
            &ServerMessage::Error {
                seq: 0,
                id: None,
                error: ApiError::Unauthorized,
            },
        )
        .await;
        return;
    };

    let id = ClientId(s.hub.next_client.fetch_add(1, Ordering::Relaxed) + 1);
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(CLIENT_QUEUE);
    let handle = Arc::new(ClientHandle {
        tx,
        seq: AtomicU64::new(0),
        dropped: AtomicU64::new(0),
        dmx: Mutex::new(Vec::new()),
    });
    if let Ok(mut c) = s.hub.clients.lock() {
        c.insert(id, handle.clone());
    }
    tracing::info!(client = id.0, %addr, ?permission, "client connected");

    let welcome = ServerMessage::Welcome {
        client: id,
        permission,
        protocol: PROTOCOL_VERSION,
        server: format!("klight-engine {}", env!("CARGO_PKG_VERSION")),
    };
    if !send_direct(&mut socket, &welcome).await {
        unregister(&s, id);
        return;
    }
    s.hub.send(&handle, full_snapshot(&s));

    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&msg) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let source = if local {
        CommandSource::LocalUi
    } else {
        CommandSource::Remote { client: id }
    };
    while let Some(Ok(msg)) = stream.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        match parse_client_message(text.as_str()) {
            Err(error) => s.hub.send(
                &handle,
                ServerMessage::Error {
                    seq: 0,
                    id: None,
                    error,
                },
            ),
            Ok(ClientMessage::Hello { .. }) => {}
            Ok(ClientMessage::Command {
                id: cmd_id,
                command,
            }) => {
                if !permission.allows(&command) {
                    s.hub.send(
                        &handle,
                        ServerMessage::Error {
                            seq: 0,
                            id: Some(cmd_id),
                            error: ApiError::Forbidden,
                        },
                    );
                    continue;
                }
                let global = s.hub.next_cmd.fetch_add(1, Ordering::Relaxed) + 1;
                if let Ok(mut p) = s.hub.pending.lock() {
                    p.insert(global, (id, cmd_id));
                }
                let env = CommandEnvelope {
                    id: global,
                    source: source.clone(),
                    command,
                };
                if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
                    s.commands.try_send(env)
                {
                    if let Ok(mut p) = s.hub.pending.lock() {
                        p.remove(&global);
                    }
                    s.hub.send(
                        &handle,
                        ServerMessage::Error {
                            seq: 0,
                            id: Some(cmd_id),
                            error: ApiError::Busy,
                        },
                    );
                }
            }
            Ok(ClientMessage::RequestSnapshot) => s.hub.send(&handle, full_snapshot(&s)),
            Ok(ClientMessage::SubscribeDmx { universes }) => {
                if let Ok(mut d) = handle.dmx.lock() {
                    *d = universes.into_iter().take(64).collect();
                }
            }
            Ok(ClientMessage::MidiIn { device, bytes }) => {
                if permission == Permission::ReadOnly {
                    continue;
                }
                if let Some(f) = &s.hooks.midi_in {
                    f(id, &device, &bytes);
                }
            }
            Ok(ClientMessage::MidiLearn) => {
                if permission != Permission::Full {
                    s.hub.send(
                        &handle,
                        ServerMessage::Error {
                            seq: 0,
                            id: None,
                            error: ApiError::Forbidden,
                        },
                    );
                } else if let Some(f) = &s.hooks.learn {
                    f(id);
                }
            }
            Ok(ClientMessage::Ping) => s.hub.send(&handle, ServerMessage::Pong { seq: 0 }),
        }
    }
    unregister(&s, id);
    writer.abort();
    tracing::info!(client = id.0, "client disconnected");
}

fn unregister(s: &AppState, id: ClientId) {
    if let Ok(mut c) = s.hub.clients.lock() {
        c.remove(&id);
    }
    if let Ok(mut p) = s.hub.pending.lock() {
        p.retain(|_, (c, _)| *c != id);
    }
}

async fn broadcast_loop(s: AppState) {
    let mut show_seq = s.shared.show_seq.load(Ordering::Relaxed);
    let mut rt_seq = s.shared.snapshot.load().seq;
    let mut tick: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        tick += 1;
        let clients = s.hub.all();
        let ss = s.shared.show_seq.load(Ordering::Relaxed);
        if ss != show_seq {
            show_seq = ss;
            let show = s.shared.show.load_full();
            for (_, h) in &clients {
                s.hub.send(
                    h,
                    ServerMessage::ShowChanged {
                        seq: 0,
                        show_seq: ss,
                        show: Box::new(show.as_ref().clone()),
                    },
                );
            }
        }
        let snap = s.shared.snapshot.load_full();
        if snap.seq != rt_seq {
            rt_seq = snap.seq;
            for (_, h) in &clients {
                s.hub.send(
                    h,
                    ServerMessage::Runtime {
                        seq: 0,
                        runtime: snap.as_ref().clone(),
                    },
                );
            }
        }
        if tick.is_multiple_of(20) && !clients.is_empty() {
            let status = status_with_clients(&s);
            for (_, h) in &clients {
                s.hub.send(
                    h,
                    ServerMessage::Status {
                        seq: 0,
                        status: status.clone(),
                    },
                );
            }
        }
        if tick.is_multiple_of(2) {
            let mut frame = [0u8; 512];
            for (_, h) in &clients {
                let subs = h.dmx.lock().map(|d| d.clone()).unwrap_or_default();
                for u in subs {
                    if let Some(slot) = s.shared.output(u) {
                        slot.read(&mut frame);
                        s.hub.send(
                            h,
                            ServerMessage::Dmx {
                                seq: 0,
                                universe: u,
                                data: frame.to_vec(),
                            },
                        );
                    }
                }
            }
        }
    }
}
