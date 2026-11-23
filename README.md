# kLight

DMX lighting control for live shows: a headless Rust engine (`klight-engine`),
a watchdog (`klight-supervisor`), and a Svelte web UI used both by the Tauri
desktop app and by remote browsers. See [`SPEC.md`](SPEC.md) for the
architecture and [`docs/user-guide.md`](docs/user-guide.md) for the user guide.

## Repository layout

| Path | Content |
|---|---|
| `crates/klight-model` | Pure data types: show, fixtures, functions, widgets, commands |
| `crates/klight-fixtures` | `.qxf` parser, fixture profiles, units ↔ DMX conversions |
| `crates/klight-engine-core` | Tick loop, layers, HTP/LTP merge, fades, chasers, cue lists, effects, musical clock |
| `crates/klight-io` | Output drivers (Enttec Pro, Open DMX, fake) and device manager |
| `crates/klight-input` | MIDI/OSC, mappings, soft takeover, controller profiles, feedback |
| `crates/klight-persist` | Show files, migrations, atomic writes, backups, autosave |
| `crates/klight-api` | WebSocket protocol and server |
| `crates/klight-engine` | Engine binary |
| `crates/klight-supervisor` | Watchdog binary |
| `crates/klight-sim` | Scenarios, golden, chaos, allocation and soak tests |
| `crates/klight-wasm` | Pure core functions compiled to WebAssembly for the UI |
| `ui/` | Svelte + TypeScript UI (protocol types generated from Rust) |
| `src-tauri/` | Desktop shell (built separately) |
| `fixtures/qlcplus/` | QLC+ fixture library, downloaded by `scripts/update-qlcplus-fixtures.sh` (not versioned) |
| `fixtures/`, `profiles/controllers/`, `tests/golden/` | Test fixtures, controller profiles, golden frames |
| `fuzz/` | `cargo-fuzz` targets |

## System dependencies

The Rust toolchain is pinned in `rust-toolchain.toml` (rustup installs it,
including the `wasm32-unknown-unknown` target). Node.js ≥ 20 is needed for the UI,
plus the WebAssembly bindings generator, same version as the crate:

```
cargo install wasm-bindgen-cli --version 0.2.130 --locked
```

**Linux (Debian/Ubuntu)**

```
sudo apt install build-essential pkg-config libudev-dev libasound2-dev
# Desktop app only (Tauri):
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev
# Access to USB-serial interfaces without root:
sudo usermod -aG dialout $USER   # then log out and back in
```

For the Enttec Open DMX the `ftdi_sio` kernel driver must be loaded (default).

**Windows**: Visual Studio Build Tools (C++ workload) and the FTDI VCP driver
(usually installed automatically by Windows Update). WebView2 is preinstalled on
Windows 10/11.

**macOS**: Xcode Command Line Tools (`xcode-select --install`). Recent macOS
versions include the FTDI driver.

## Build and run

```
cargo build --release -p klight-engine -p klight-supervisor
cd ui && npm ci && npm run build && cd ..
./target/release/klight-supervisor -- --show my-show.klight.json --ui ui/dist
```

Open `http://<this-machine>:9400/` in a browser. Clients on localhost have full
access; remote clients need a token: `--token secret123:playback`
(`full`, `playback`, `readonly`), then open `http://host:9400/?token=secret123`.

UI development: run the engine, then `cd ui && npm run dev` (Vite proxies the API).

Desktop app: `cd src-tauri && cargo tauri build` (requires `cargo install tauri-cli`
and the Tauri system libraries above; icons for Windows/macOS can be generated
with `cargo tauri icon src-tauri/icons/icon.png`).

## Tests

```
cargo test --workspace                                   # unit, property, golden, chaos, alloc
cargo run -p klight-sim --release --bin klight-soak -- --minutes 30
cd ui && npm test && npm run check                       # Vitest + type check
cd ui && npm run build && PW_CHANNEL=chrome npm run e2e  # Playwright (engine must be built)
cd fuzz && cargo +nightly fuzz run qxf                   # also: show_file, api_message, osc, midi
cargo test -p klight-io --features hardware-tests        # with real Enttec interfaces
```

## UI and core logic

The UI never re-implements core logic. Movement and matrix previews, group
layouts, patch checks (first free address, conflicts, channel map) and color
conversions are the engine's own Rust functions compiled to WebAssembly
(`crates/klight-wasm`), so a preview matches the output by construction.
`npm run build`, `test`, `check` and `dev` rebuild the module automatically.

## Fixture library

The QLC+ fixture definitions (about 1800 `.qxf` files, Apache License 2.0)
are not stored in this repository. Download them once after cloning, and
again whenever you want to update them:

```
scripts/update-qlcplus-fixtures.sh
```

They go to `fixtures/qlcplus/` (one folder per manufacturer, with the QLC+
license and the upstream commit). The engine loads every `.qxf` under
`--fixtures` recursively and the UI lists them by manufacturer. Without them
kLight still works with the few test definitions in `fixtures/` and with the
definitions embedded in each show. CI and the desktop build download them
automatically (`--if-missing`). On Windows run the script from Git Bash.

## License

Not decided yet (SPEC §18). Until then only permissive dependencies are
allowed; `cargo deny check licenses` enforces it (`deny.toml`).
