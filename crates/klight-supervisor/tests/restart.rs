//! Chaos test: kill (or freeze) the engine and check that the supervisor
//! restarts it with the saved runtime state.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use klight_model::*;

fn engine_binary() -> PathBuf {
    let sup = PathBuf::from(env!("CARGO_BIN_EXE_klight-supervisor"));
    let engine = sup.with_file_name("klight-engine");
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "klight-engine"])
        .status()
        .expect("cargo build");
    assert!(status.success());
    assert!(engine.exists(), "{}", engine.display());
    engine
}

fn http_get(port: u16, path: &str) -> Option<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = String::new();
    s.read_to_string(&mut buf).ok()?;
    buf.split_once("\r\n\r\n").map(|(_, b)| b.to_owned())
}

fn runtime(port: u16) -> Option<RuntimeSnapshot> {
    serde_json::from_str(&http_get(port, "/api/runtime")?).ok()
}

fn wait<T>(what: &str, timeout: Duration, mut f: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(start.elapsed() < timeout, "timeout waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn supervisor_restarts_killed_and_frozen_engine_with_state() {
    let engine = engine_binary();
    let dir = tempfile::tempdir().unwrap();
    let show_path = dir.path().join("show.json");
    let mut show = Show::new("supervised");
    show.universes.push(Universe {
        id: UniverseId(1),
        name: "U1".into(),
        output: None,
    });
    klight_persist::save_show(&show_path, &show, 2).unwrap();
    let state = RuntimeState {
        grand_master: 0.3,
        mode: OperatingMode::Show,
        ..RuntimeState::default()
    };
    klight_persist::save_runtime(&klight_persist::runtime_path(&show_path), &state).unwrap();

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut sup = Command::new(env!("CARGO_BIN_EXE_klight-supervisor"))
        .args([
            "--engine",
            engine.to_str().unwrap(),
            "--timeout-ms",
            "1500",
            "--",
        ])
        .args([
            "--show",
            show_path.to_str().unwrap(),
            "--bind",
            &format!("127.0.0.1:{port}"),
            "--restore",
        ])
        .args([
            "--fixtures",
            dir.path().to_str().unwrap(),
            "--profiles",
            dir.path().to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel::<u32>();
    let stderr = sup.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(pid) = line.split("ENGINE_PID=").nth(1) {
                let _ = tx.send(pid.trim().parse().unwrap());
            }
        }
    });

    let check_state = || {
        let rt = wait("engine API", Duration::from_secs(20), || runtime(port));
        assert!((rt.state.grand_master - 0.3).abs() < 1e-6, "{rt:?}");
        assert_eq!(rt.state.mode, OperatingMode::Show);
    };

    let pid1 = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    check_state();

    // 1. Crash: SIGKILL.
    Command::new("kill")
        .args(["-9", &pid1.to_string()])
        .status()
        .unwrap();
    let pid2 = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_ne!(pid1, pid2);
    check_state();

    // 2. Hang: SIGSTOP freezes the engine; heartbeat stops.
    Command::new("kill")
        .args(["-STOP", &pid2.to_string()])
        .status()
        .unwrap();
    let pid3 = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_ne!(pid2, pid3);
    check_state();

    sup.kill().unwrap();
    let _ = sup.wait();
    // Engine notices the closed stdin and exits by itself.
    wait("engine exit", Duration::from_secs(10), || {
        let alive = Command::new("kill")
            .args(["-0", &pid3.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        (!alive).then_some(())
    });
}
