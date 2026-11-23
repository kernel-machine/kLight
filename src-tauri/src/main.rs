// kLight desktop shell.
//
// Starts `klight-supervisor`, which starts and watches `klight-engine`; the
// window then loads the UI served by the local engine, exactly like a
// remote client. Closing the window stops the supervisor; the engine sees
// its stdin close, saves and exits.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use tauri::Manager;

struct Supervisor(Mutex<Option<Child>>);

fn sidecar(name: &str) -> PathBuf {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&exe)))
        .unwrap_or_else(|| PathBuf::from(exe))
}

fn start_supervisor(app: &tauri::AppHandle) -> std::io::Result<Child> {
    let data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resources = app.path().resource_dir().unwrap_or_else(|_| PathBuf::from("."));
    std::fs::create_dir_all(&data)?;
    let logs = data.join("logs");
    std::fs::create_dir_all(&logs)?;
    let show = data.join("show.klight.json");
    Command::new(sidecar("klight-supervisor"))
        .arg("--engine")
        .arg(sidecar("klight-engine"))
        .arg("--")
        .arg("--show")
        .arg(&show)
        .arg("--restore")
        .args(["--bind", "0.0.0.0:9400"])
        .arg("--ui")
        .arg(resources.join("ui"))
        .arg("--fixtures")
        .arg(resources.join("fixtures"))
        .arg("--profiles")
        .arg(resources.join("profiles"))
        .arg("--log-dir")
        .arg(&logs)
        .stdin(Stdio::null())
        .spawn()
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let child = start_supervisor(app.handle())?;
            app.manage(Supervisor(Mutex::new(Some(child))));
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(a) => a,
        Err(e) => {
            eprintln!("kLight failed to start: {e}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<Supervisor>() {
                if let Ok(mut guard) = state.0.lock() {
                    if let Some(mut child) = guard.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        }
    });
}
