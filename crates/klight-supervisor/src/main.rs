//! kLight supervisor: starts the engine, watches its heartbeat and restarts
//! it (with `--restore`, so cues and faders come back) if it dies or hangs.
//!
//! Deliberately tiny and dependency-free: it must be the most reliable
//! process of the system.
//!
//! Usage: `klight-supervisor [--engine <path>] [--timeout-ms <n>] -- <engine args>`

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Config {
    engine: PathBuf,
    heartbeat_timeout: Duration,
    startup_grace: Duration,
    engine_args: Vec<String>,
}

fn default_engine() -> PathBuf {
    let name = if cfg!(windows) {
        "klight-engine.exe"
    } else {
        "klight-engine"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn parse_args() -> Result<Config, String> {
    let mut c = Config {
        engine: default_engine(),
        heartbeat_timeout: Duration::from_secs(3),
        startup_grace: Duration::from_secs(15),
        engine_args: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--engine" => c.engine = PathBuf::from(it.next().ok_or("--engine needs a value")?),
            "--timeout-ms" => {
                let v: u64 = it
                    .next()
                    .ok_or("--timeout-ms needs a value")?
                    .parse()
                    .map_err(|e| format!("{e}"))?;
                c.heartbeat_timeout = Duration::from_millis(v.max(500));
            }
            "--" => {
                c.engine_args = it.collect();
                break;
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(c)
}

fn log(msg: &str) {
    eprintln!("[klight-supervisor] {msg}");
}

fn now_ms(origin: Instant) -> u64 {
    u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn spawn(
    cfg: &Config,
    restore: bool,
    last_beat: &Arc<AtomicU64>,
    origin: Instant,
) -> std::io::Result<Child> {
    let mut cmd = Command::new(&cfg.engine);
    cmd.args(&cfg.engine_args);
    cmd.arg("--exit-on-stdin-close");
    if restore && !cfg.engine_args.iter().any(|a| a == "--restore") {
        cmd.arg("--restore");
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn()?;
    last_beat.store(now_ms(origin), Ordering::SeqCst);
    if let Some(out) = child.stdout.take() {
        let beat = last_beat.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                match line {
                    Ok(l) if l.starts_with("HEARTBEAT") => {
                        beat.store(now_ms(origin), Ordering::SeqCst)
                    }
                    Ok(l) => println!("{l}"),
                    Err(_) => break,
                }
            }
        });
    }
    log(&format!("engine started, ENGINE_PID={}", child.id()));
    Ok(child)
}

fn main() {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            log(&e);
            std::process::exit(2);
        }
    };
    let origin = Instant::now();
    let last_beat = Arc::new(AtomicU64::new(0));
    let mut restore = cfg.engine_args.iter().any(|a| a == "--restore");
    let mut backoff = Duration::from_millis(200);

    loop {
        let started = Instant::now();
        let mut child = match spawn(&cfg, restore, &last_beat, origin) {
            Ok(c) => c,
            Err(e) => {
                log(&format!(
                    "cannot start engine {}: {e}",
                    cfg.engine.display()
                ));
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(5));
                continue;
            }
        };
        let mut seen_beat = false;
        let reason = loop {
            std::thread::sleep(Duration::from_millis(100));
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        log("engine exited normally");
                        return;
                    }
                    break format!("engine exited with {status}");
                }
                Ok(None) => {}
                Err(e) => break format!("cannot query engine: {e}"),
            }
            let last = last_beat.load(Ordering::SeqCst);
            let since = Duration::from_millis(now_ms(origin).saturating_sub(last));
            if last > 0
                && started.elapsed() > Duration::from_millis(500)
                && since < cfg.heartbeat_timeout
            {
                seen_beat = true;
            }
            let limit = if seen_beat {
                cfg.heartbeat_timeout
            } else {
                cfg.startup_grace
            };
            if since > limit {
                let _ = child.kill();
                let _ = child.wait();
                break format!("no heartbeat for {} ms: engine killed", since.as_millis());
            }
        };
        log(&format!("{reason}; restarting with state restore"));
        // Quick crash loops back off; a long healthy run resets the backoff.
        if started.elapsed() > Duration::from_secs(30) {
            backoff = Duration::from_millis(200);
        } else {
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
        restore = true;
    }
}
