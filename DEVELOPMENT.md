# Developing kLight

Everything needed to build, run, test and change kLight. The design of the
system is in [`SPEC.md`](SPEC.md), which is the source of truth: read it before
writing code, and if an implementation choice contradicts it, open an issue
instead of deviating. The screens are described in the
[user guide](docs/user-guide.md).

> kLight is a work in progress: interfaces, the protocol and the show file
> format can still change between commits.

## Architecture in short

- **`klight-engine`**: a headless Rust process that owns the show and sends DMX
  from a fixed-rate tick loop (44 Hz). It is the only part that touches the
  hardware.
- **`klight-supervisor`**: a watchdog that starts the engine, restarts it if it
  dies and restores its state.
- **The UI** (Svelte + TypeScript, in `ui/`): served by the engine to any
  browser, and wrapped by the Tauri desktop app in `src-tauri/`. It talks to the
  engine over a WebSocket and never drives the output itself.

The engine core is pure and deterministic: no I/O, no allocation in the steady
tick, explicit errors, and every long-running thread restarted if it panics.
The same pure functions are compiled to WebAssembly for the UI (see
[UI and core logic](#ui-and-core-logic)).

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
| `fixtures/qlcplus/`, `profiles/qlcplus/` | QLC+ fixture library and controller (`.qxi`) profiles, downloaded by `scripts/update-qlcplus-fixtures.sh` (not versioned) |
| `fixtures/`, `profiles/controllers/`, `tests/golden/` | Test fixtures, controller profiles, golden frames |
| `fuzz/` | `cargo-fuzz` targets |
| `docs/` | User guide and images |

Dependency rule: `klight-model` depends on nothing in the project;
`klight-engine-core` only on `klight-model` and `klight-fixtures`. I/O lives
only in `io`, `input`, `persist`, `api` and the binaries.

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

## Fixture library

The QLC+ fixture definitions (about 1800 `.qxf` files) and controller profiles
(`.qxi`, used for LED feedback; Apache License 2.0) are not stored in this
repository. Download them once after cloning, and again whenever you want to
update them:

```
scripts/update-qlcplus-fixtures.sh
```

They go to `fixtures/qlcplus/` (one folder per manufacturer, with the QLC+
license and the upstream commit); the controller profiles go to
`profiles/qlcplus/` (`--qlcplus-profiles`). In Devices, pick the profile of a
controller and choose what its LEDs do when a widget is on or off. The engine
loads every `.qxf` under `--fixtures` recursively and the UI lists them by
manufacturer. Without them kLight still works with the few test definitions in
`fixtures/` and with the definitions embedded in each show. CI and the desktop
build download them automatically (`--if-missing`). On Windows run the script
from Git Bash.

## UI and core logic

The UI never re-implements core logic. Movement and matrix previews, group
layouts, patch checks (first free address, conflicts, channel map), color
conversions, the values a scene gives each light and the times of a chaser are
the engine's own Rust functions compiled to WebAssembly (`crates/klight-wasm`),
so a preview matches the output by construction. When the UI needs something
the engine computes, add a pure function to the core and expose it there.
`npm run build`, `test`, `check` and `dev` rebuild the module automatically.

The TypeScript types of the protocol and the model are generated from the Rust
types: after changing them, run `cd ui && npm run bindings` (CI checks that the
generated files are up to date).

## Tests

```
cargo test --workspace                                   # unit, property, golden, chaos, alloc
cargo run -p klight-sim --release --bin klight-soak -- --minutes 30
cargo run -p klight-sim --release --bin klight-soak -- --minutes 1 --inject-slow-tick-ms 1  # must fail
cd ui && npm test && npm run check                       # Vitest + type check
cd ui && npm run build && PW_CHANNEL=chrome npm run e2e  # Playwright (engine must be built)
cd fuzz && cargo +nightly fuzz run qxf                   # also: show_file, api_message, osc, midi
cargo test -p klight-io --features hardware-tests        # with real Enttec interfaces
```

What they cover:

- **Golden tests** (`klight-sim`): reference shows simulated frame by frame
  with a virtual clock, compared with the frames in `tests/golden/`. After an
  intended output change: `KLIGHT_UPDATE_GOLDEN=1 cargo test -p klight-sim --test golden`,
  then review the diff.
- **Allocation test**: the tick loop does not allocate after warm-up
  (`crates/klight-sim/tests/allocation.rs`).
- **Chaos tests**: drivers that disconnect, slow down or fail must never stop
  the tick loop or the other universes.
- **Soak test**: a realistic show with the real clock for 30 minutes (24 hours
  nightly). The engine is judged on what the machine cannot change: the CPU
  time of every tick and any blocking inside it (a lock, a sleep, I/O). Wall
  time only says whether the machine is fit for the test. A self-check run
  injects faults first and must fail, so the soak can never go blind.
- **Fuzzing**: the `.qxf` parser, show files, API messages, OSC and MIDI.
- **End-to-end tests** (Playwright): the main flows of the UI against a real
  engine.

## Code rules

The full list is in [`CLAUDE.md`](CLAUDE.md) and [SPEC §16](SPEC.md#16-code-rules-to-be-copied-into-claudemd). In short:

- In the core (`klight-model`, `klight-engine-core`, `klight-fixtures`):
  no `unwrap`, `expect`, `panic!`, unchecked indexing or `unsafe` (enforced by
  clippy lints). `unsafe` only in drivers, isolated and justified.
- Errors use explicit types; none is silently ignored.
- No I/O, `sleep` or blocking lock inside the tick loop, and no allocation in the
  steady-state tick.
- Every bug first gets a test that reproduces it, then the fix.
- English everywhere: code, identifiers, comments, documentation.
- New dependencies in the core must be justified, and every dependency must be
  compatible with GPL-3.0-or-later (`cargo deny check licenses`, see `deny.toml`).

## Before every commit

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm run check && npm test && npm run build   # also rebuilds klight-wasm
```

Keep commits small, one milestone at a time; don't start the next one without
green CI.

## Continuous integration

GitHub Actions (`.github/workflows/ci.yml`) runs on every pull request and
nightly:

| Job | What |
|---|---|
| `rust` | fmt, clippy and tests on Linux, Windows and macOS |
| `ui` | generated bindings up to date, type check, unit tests, build, end-to-end tests |
| `licenses` | `cargo deny check licenses` |
| `soak` | soak self-check, then 30 minutes (24 hours nightly) |
| `fuzz` | nightly only, every fuzz target for 10 minutes |

## Releases

There are no releases yet. Version numbers and a changelog will come with the
first one.
