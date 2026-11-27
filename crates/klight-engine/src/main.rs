//! kLight headless engine.
//!
//! Runs a show without any UI attached. Prints `HEARTBEAT <ticks>` on stdout
//! every 250 ms for the supervisor; logs go to stderr and to rotating files.

mod args;
mod host;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use klight_api::{
    ApiConfig, ApiHooks, ClientLink, InputInfo, JitterInfo, OutputInfo, ServerMessage, SystemStatus,
};
use klight_engine_core::{Clock, Engine, EngineConfig, LoopHooks, MonotonicClock, TickLoop};
use klight_io::{DeviceManager, ManagerConfig, SystemFactory};
use klight_model::{CommandEnvelope, CommandResult, Show, Universe, UniverseId};
use klight_persist::{AutosaveConfig, Autosaver};

use crate::args::Args;
use crate::host::EngineHost;

fn main() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("klight-engine: {e}\n\n{}", args::USAGE);
            std::process::exit(2);
        }
    };
    let _log_guard = init_logging(args.log_dir.as_deref());
    if let Err(e) = run(args) {
        tracing::error!("fatal: {e}");
        std::process::exit(1);
    }
}

fn init_logging(dir: Option<&Path>) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let stderr = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    match dir {
        Some(d) => {
            let appender = tracing_appender::rolling::daily(d, "klight-engine.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let file = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(stderr)
                .with(file)
                .try_init();
            Some(guard)
        }
        None => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(stderr)
                .try_init();
            None
        }
    }
}

/// Collects `.qxf` files recursively (QLC+ keeps one folder per manufacturer).
fn qxf_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        if depth == 0 {
            tracing::warn!("fixture directory {} not readable", dir.display());
        }
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() && depth < 4 {
            qxf_files(&path, depth + 1, out);
        } else if path.extension().is_some_and(|x| x == "qxf") {
            out.push(path);
        }
    }
}

fn load_library(dirs: &[PathBuf]) -> Vec<klight_model::FixtureDefinition> {
    let mut lib = klight_fixtures::FixtureLibrary::new();
    let mut files = Vec::new();
    for dir in dirs {
        qxf_files(dir, 0, &mut files);
    }
    let mut rejected = 0usize;
    for path in files {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if let Err(r) = lib.add_qxf(&path.display().to_string(), &text) {
                    rejected += 1;
                    tracing::warn!("fixture {} rejected: {}", r.source, r.error);
                }
            }
            Err(e) => tracing::warn!("cannot read {}: {e}", path.display()),
        }
    }
    tracing::info!(
        "fixture library: {} definitions ({rejected} rejected)",
        lib.len()
    );
    let mut defs: Vec<_> = lib.iter().cloned().collect();
    klight_api::sort_library(&mut defs);
    defs
}

fn initial_show(path: &Path) -> Show {
    if path.exists() {
        match klight_persist::load_show(path) {
            Ok(loaded) => {
                if let Some(b) = &loaded.from_backup {
                    tracing::warn!("show file damaged; loaded backup {}", b.display());
                    for p in &loaded.problems {
                        tracing::warn!("{p}");
                    }
                }
                return loaded.show;
            }
            Err(e) => {
                // Never start dark because of a broken file: start empty and say so.
                tracing::error!("cannot load show ({e}); starting with an empty show");
            }
        }
    }
    let mut show = Show::new("New show");
    show.universes.push(Universe {
        id: UniverseId(1),
        name: "Universe 1".into(),
        output: None,
    });
    show
}

fn run(args: Args) -> Result<(), String> {
    let clock: Arc<dyn Clock> = Arc::new(MonotonicClock::new());
    let show = initial_show(&args.show);
    let library = Arc::new(load_library(&args.fixture_dirs));

    let mut engine = Engine::new(show, EngineConfig::default());
    if args.restore {
        let rp = klight_persist::runtime_path(&args.show);
        match klight_persist::load_runtime(&rp) {
            Ok(state) => {
                for w in engine.restore(clock.now_us(), &state) {
                    tracing::warn!("restore: {w}");
                }
                tracing::info!("runtime state restored from {}", rp.display());
            }
            Err(e) => tracing::warn!("no runtime state restored: {e}"),
        }
    }
    let shared = engine.shared();

    let (cmd_tx, cmd_rx) = crossbeam_channel::bounded::<CommandEnvelope>(4096);
    let (res_tx, res_rx) = crossbeam_channel::bounded::<CommandResult>(4096);
    let hooks = LoopHooks {
        on_start: Some(Box::new(|| raise_priority("tick"))),
        on_tick: None,
        on_timing: None,
        probe: None,
    };
    let tick =
        TickLoop::spawn(engine, clock.clone(), cmd_rx, res_tx, hooks).map_err(|e| e.to_string())?;

    // Outputs.
    let devices = Arc::new(Mutex::new(DeviceManager::new(
        Arc::new(SystemFactory::default()),
        shared.clone(),
        ManagerConfig::default(),
    )));
    if let Ok(mut d) = devices.lock() {
        d.sync(&shared.show.load());
    }

    // Persistence.
    let autosave = Arc::new(
        Autosaver::spawn(shared.clone(), AutosaveConfig::new(args.show.clone()))
            .map_err(|e| e.to_string())?,
    );

    // Inputs.
    let link: Arc<OnceLock<ClientLink>> = Arc::new(OnceLock::new());
    let host = Arc::new(EngineHost {
        shared: shared.clone(),
        commands: cmd_tx.clone(),
        clock: clock.clone(),
        link: link.clone(),
    });
    // QLC+ input profiles first: a hand-written profile with the same id wins.
    let mut profiles = klight_input::load_qxi_profiles(&args.qlcplus_profiles_dir);
    profiles.extend(klight_input::load_profiles(&args.profiles_dir));
    let inputs = Arc::new(
        klight_input::InputManager::start(host.clone(), profiles).map_err(|e| e.to_string())?,
    );

    // Housekeeping: resync outputs when the show changes; heartbeat.
    let stop = Arc::new(AtomicBool::new(false));
    let housekeeping = {
        let (stop, shared, devices) = (stop.clone(), shared.clone(), devices.clone());
        std::thread::Builder::new()
            .name("klight-housekeeping".into())
            .spawn(move || {
                let mut seen = shared.show_seq.load(Ordering::Relaxed);
                let mut last_ticks = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let seq = shared.show_seq.load(Ordering::Relaxed);
                    if seq != seen {
                        seen = seq;
                        if let Ok(mut d) = devices.lock() {
                            d.sync(&shared.show.load());
                        }
                    }
                    // Heartbeat only while the tick loop makes progress:
                    // a hung engine stops beating and gets restarted.
                    let ticks = shared.stats.ticks.load(Ordering::Relaxed);
                    if ticks != last_ticks {
                        last_ticks = ticks;
                        println!("HEARTBEAT {ticks}");
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
            .map_err(|e| e.to_string())?
    };

    // API server (Tokio lives only here, never in the tick loop).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("klight-api")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    let status_fn = {
        let (shared, devices, inputs, autosave, show_path) = (
            shared.clone(),
            devices.clone(),
            inputs.clone(),
            autosave.clone(),
            args.show.display().to_string(),
        );
        Arc::new(move || {
            let s = shared.stats.snapshot();
            let outputs = devices
                .lock()
                .map(|d| d.statuses())
                .unwrap_or_default()
                .into_iter()
                .map(|o| OutputInfo {
                    universe: o.universe,
                    kind: format!("{:?}", o.kind),
                    state: format!("{:?}", o.driver.state).to_lowercase(),
                    port: o.driver.port,
                    serial_number: o.driver.serial_number,
                    hardware_buffer: o.driver.hardware_buffer,
                    frames_sent: o.frames_sent,
                    errors: o.errors,
                    last_error: o.last_error,
                })
                .collect();
            let a = autosave.status();
            SystemStatus {
                jitter: JitterInfo {
                    min_us: s.jitter_min_us,
                    avg_us: s.jitter_avg_us,
                    max_us: s.jitter_max_us,
                    p99_us: s.jitter_p99_us,
                    work_us: s.work_us,
                    ticks: s.ticks,
                    overruns: s.overruns,
                },
                outputs,
                inputs: inputs
                    .statuses()
                    .into_iter()
                    .map(|i| InputInfo {
                        name: i.name,
                        kind: i.kind,
                        connected: i.connected,
                        events: i.events,
                    })
                    .collect(),
                last_autosave_ms: a.last_show_save_ms,
                autosave_error: a.last_error,
                clients: 0,
                beat: shared.beat.load(Ordering::Relaxed) as f64
                    / klight_engine_core::music::BEAT as f64,
                show_path: Some(show_path.clone()),
            }
        })
    };
    let api_hooks = ApiHooks {
        status: status_fn,
        midi_in: Some({
            let inputs = inputs.clone();
            Arc::new(move |client, device: &str, bytes: &[u8]| {
                inputs.bridge_input(client, device, bytes)
            })
        }),
        learn: Some({
            let (inputs, link) = (inputs.clone(), link.clone());
            Arc::new(move |client| {
                let link = link.clone();
                inputs.learn(Box::new(move |learned| {
                    if let Some(l) = link.get() {
                        l.send(
                            client,
                            ServerMessage::Learned {
                                seq: 0,
                                device: learned.device,
                                message: learned.message,
                            },
                        );
                    }
                }));
            })
        }),
        ports: None,
        input_profiles: None,
    };
    let api_hooks = ApiHooks {
        input_profiles: Some({
            let inputs = inputs.clone();
            Arc::new(move || {
                inputs
                    .profiles()
                    .into_iter()
                    .map(|(id, p)| klight_api::InputProfileInfo {
                        id,
                        name: p.name,
                        default_active: p.default_active,
                        behaviors: p
                            .behaviors
                            .into_iter()
                            .map(|b| klight_api::LedBehaviorInfo {
                                channel: b.channel,
                                label: b.label,
                            })
                            .collect(),
                    })
                    .collect()
            })
        }),
        ports: Some(Arc::new(|| {
            klight_io::discovery::list_ports()
                .into_iter()
                .map(|p| klight_api::SerialPortInfo {
                    is_ftdi: p.is_ftdi(),
                    port: p.port,
                    serial_number: p.serial_number,
                    product: p.product,
                })
                .collect()
        })),
        ..api_hooks
    };
    match &args.ui_dir {
        None => tracing::warn!(
            "no --ui directory given: the web UI will not be served (HTTP 404 on /). \
             Build it with `cd ui && npm run build` and pass `--ui ui/dist`"
        ),
        Some(dir) if !dir.join("index.html").is_file() => tracing::warn!(
            "--ui {} has no index.html: the web UI will not be served. \
             Build it with `cd ui && npm run build`",
            dir.display()
        ),
        Some(_) => {}
    }
    let api_config = ApiConfig {
        bind: args.bind,
        static_dir: args.ui_dir.clone(),
        tokens: args.tokens.clone(),
        local_permission: klight_api::Permission::Full,
        library,
    };
    let server = runtime
        .block_on(klight_api::serve(
            api_config,
            shared.clone(),
            cmd_tx.clone(),
            res_rx,
            api_hooks,
        ))
        .map_err(|e| format!("cannot start API on {}: {e}", args.bind))?;
    let _ = link.set(server.link());
    tracing::info!("kLight engine running; show {}", args.show.display());

    // Supervisor gone (stdin closed) => clean shutdown with final save.
    let stdin_closed = Arc::new(tokio::sync::Notify::new());
    if args.exit_on_stdin_close {
        let n = stdin_closed.clone();
        let _ = std::thread::Builder::new()
            .name("klight-stdin".into())
            .spawn(move || {
                let mut sink = Vec::new();
                let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut sink);
                n.notify_one();
            });
    }

    // Wait for Ctrl-C / SIGTERM / stdin close.
    runtime.block_on(async {
        let closed = stdin_closed.notified();
        tokio::pin!(closed);
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            match signal(SignalKind::terminate()) {
                Ok(mut term) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {},
                        _ = term.recv() => {},
                        _ = &mut closed => {},
                    }
                }
                Err(_) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {},
                        _ = &mut closed => {},
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = &mut closed => {},
            }
        }
    });
    tracing::info!("shutting down");

    runtime.block_on(server.shutdown());
    stop.store(true, Ordering::SeqCst);
    let _ = housekeeping.join();
    drop(host);
    if let Ok(inputs) = Arc::try_unwrap(inputs) {
        inputs.stop();
    }
    // Final flush of show and runtime state.
    if let Ok(a) = Arc::try_unwrap(autosave) {
        a.stop();
    }
    if let Ok(mut d) = devices.lock() {
        d.shutdown();
    }
    tick.stop();
    runtime.shutdown_timeout(Duration::from_secs(1));
    Ok(())
}

pub fn raise_priority(what: &str) {
    match thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max) {
        Ok(()) => tracing::info!("{what} thread priority raised"),
        Err(e) => tracing::warn!(
            "cannot raise {what} thread priority ({e:?}); running at normal priority"
        ),
    }
}
