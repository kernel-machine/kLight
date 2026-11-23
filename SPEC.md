# kLight — project specification

DMX lighting control software for live use.

> Product name: **kLight**. In code it is always lowercase: `klight`
> (prefix of crates, binaries, folders, identifiers).
> This document is the source of truth for architecture and development plan.
> Read all of it before writing code. If an implementation choice contradicts
> this document, stop and ask instead of deviating.

---

## 1. Goal

Cross-platform software (Linux, Windows, macOS) for controlling DMX lights,
functionally similar to QLC+, designed for **live shows**.

**Absolute priority: robustness.** During a show the software must never
stop sending DMX. A UI crash, a disconnected remote client, an unplugged USB
interface or a corrupted file must never leave the stage dark or in an
unexpected state.

Order of priorities when they conflict:
1. Continuity of the DMX output
2. Correctness of the output values
3. Integrity of the show data
4. UI responsiveness
5. Features and convenience

## 2. Non-goals (v1)

- Network output (Art-Net, sACN): the architecture must allow for it, but v1
  implements only the Enttec interfaces.
- Show/timeline with audio track, audio triggers.
- 3D visualizer.
- Tracking cue lists, hot backup, Ableton Link, automatic music
  synchronisation, 3D pointing: see §17 (the architecture must allow for them).
- User scripts (possibly a WASM sandbox in the future, never JS in the engine).
- Import of QLC+ `.qxw` workspaces (after v1).

---

## 3. Architectural principles

1. **The engine is headless and independent of the UI.** It runs as a
   separate process. The UI is just a client.
2. **Single source of truth: the core.** The show (patch, functions, virtual
   console, input mappings) and its runtime state live in the core. Clients
   draw and send commands; they have no state of their own that affects the output.
3. **Everything enters through a command queue.** UI, remote clients, MIDI,
   OSC and keyboard produce typed `Command`s, validated before being applied.
   No component modifies the state directly.
4. **The update loop is sacred.** Fixed rate, no allocation in the hot path,
   no blocking I/O, no contended locks.
5. **Fault isolation.** Every input/output driver has its own thread. A
   driver failure does not affect the others or the engine.
6. **Determinism.** With the same show, the same command sequence and the
   same clock, the output is identical bit for bit. It is the basis of the tests.
7. **Controlled degradation.** On error the last valid frame is kept, the
   problem is reported and recovery is attempted; never panic, never silence.

---

## 4. Process architecture

```
┌───────────────┐   ┌───────────────┐   ┌──────────────┐
│ Desktop UI    │   │ Remote client │   │ Supervisor   │
│ (Tauri)       │   │ (browser)     │   │ (watchdog)   │
└──────┬────────┘   └──────┬────────┘   └──────┬───────┘
       │  WebSocket API    │                   │ heartbeat / restart
       ▼                   ▼                   ▼
┌──────────────────────────────────────────────────────┐
│ HEADLESS ENGINE (klight-engine)                      │
│                                                      │
│  API server ──► Command queue ──► Show state         │
│  Input drivers ┘                   │   (immutable    │
│  (MIDI, OSC)                       │    snapshots)   │
│                                    ▼                 │
│                     44 Hz tick loop ──► Universe     │
│                                    │      merge      │
│  Persistence ◄── autosave          ▼                 │
│                      Output drivers (1 thread each)  │
└──────────────────────────────────────────────────────┘
                                     │
                       Enttec DMX USB Pro / Open DMX
```

- **klight-engine**: headless process. Contains everything needed to run a
  show. Must be able to run without any UI attached.
- **klight-supervisor**: minimal process. Starts the engine, checks a
  heartbeat, restarts it if it dies or hangs, restoring the last saved state
  (including the runtime state: active cues, fader values).
- **Desktop UI**: Tauri app that starts supervisor + engine as child
  processes and connects through the API like any other client.
- **Remote clients**: the same web UI served by the engine, opened from a
  browser (tablet, phone, other PC).

---

## 5. Technology stack

| Area | Choice | Reason |
|---|---|---|
| Core, drivers, supervisor | **Rust** (stable, edition 2021+) | Memory safety, no GC, cross-platform |
| Internal communication | `crossbeam-channel`, `arc-swap` | Lock-free queues, atomic snapshots |
| API server | `tokio` + `axum` (WebSocket) | Only outside the tick loop |
| Serialisation | `serde`, `serde_json` | Show file and protocol |
| `.qxf` parsing | `quick-xml` | QLC+ fixture format |
| Serial / FTDI | `serialport` (break supported); consider `libftdi1`/D2XX only if needed for Open DMX | Enttec |
| MIDI | `midir` | Cross-platform |
| OSC | `rosc` | |
| Tests | `proptest`, `insta` (snapshots), `cargo-fuzz` | Property, golden, fuzzing |
| UI | **Tauri 2** + **TypeScript** + **Svelte** | Same UI for desktop and remote |
| Core logic in the UI | `klight-wasm` (`wasm-bindgen`) | The UI calls the engine's own pure functions; no duplicated logic |
| Logging | `tracing` + rotating files | Post-show diagnostics |

Do not add dependencies to the core without justifying it in the PR/commit.

---

## 6. Repository structure

```
klight/
├── SPEC.md                    # this document
├── CLAUDE.md                  # working rules for Claude Code (see §16)
├── Cargo.toml                 # workspace
├── crates/
│   ├── klight-model/         # pure types: show, fixtures, functions, widgets, commands. No I/O.
│   ├── klight-fixtures/      # .qxf parser, fixture library, physical units ↔ DMX conversions
│   ├── klight-engine-core/   # tick loop, layers, merge, function execution. No I/O, injected clock.
│   ├── klight-io/            # driver trait + Enttec Pro, Open DMX, fake driver
│   ├── klight-input/         # MIDI, OSC, controller profiles, mappings, feedback
│   ├── klight-persist/       # show file, schema versions, migrations, atomic writes, autosave
│   ├── klight-api/           # protocol (message types) + WebSocket server
│   ├── klight-engine/        # binary: assembles everything
│   ├── klight-supervisor/    # binary: watchdog
│   ├── klight-sim/           # deterministic simulator + harness for chaos and soak tests
│   └── klight-wasm/          # pure core functions compiled to WebAssembly for the UI
├── ui/                        # Svelte + TypeScript
├── src-tauri/                 # Tauri shell
├── profiles/controllers/      # MIDI controller profiles (TOML)
├── fixtures/                  # test .qxf files
└── tests/golden/              # reference DMX frames
```

Dependency rule: `model` depends on nothing in the project; `engine-core`
depends only on `model` and `fixtures`; I/O lives only in `io`, `input`,
`persist`, `api` and the binaries.

**No duplicated logic in the UI.** Anything the UI needs to compute the way
the engine does (effect previews, group layouts, patch checks, color
conversions) comes from `klight-wasm`, which wraps the core's pure functions.
Behaviour that belongs to the show (e.g. fans over a group) is a core
intention, never computed by a client.

---

## 7. Data model (klight-model)

### 7.1 Show
A `Show` contains: universes, patch (instantiated fixtures), fixture groups,
functions, virtual console, configured I/O devices, input mappings, project
settings. It has a `schema_version` field.

### 7.2 Universes and patch
- `Universe { id, name, output: Option<OutputAssignment> }` — 512 channels.
- `PatchedFixture { id, name, definition_ref, mode, universe, address, ... }`.
- **Address conflicts** are detected during validation and returned to the
  client as explicit errors; a patch command that creates a conflict is
  rejected (or accepted with a warning if the user explicitly forces it).
- **Orientation** set at patch time for each fixture: inverted pan, inverted
  tilt, swapped pan/tilt, upside-down mounting. From then on "up", "left" and
  degrees mean the same for every head.
- **Fixture groups** with **order** and **2D grid** (position of each head
  in the grid): the basis for matrices, fans and effect phase. Scenes,
  palettes and effects refer to the group, not to individual channels: a
  fixture added to the group automatically inherits what is applied to the group.

### 7.3 Fixture definitions
- **QLC+ `.qxf` files** are read.
- A copy of each definition in use is **embedded in the show**: the show must
  open identically on a machine that does not have that `.qxf`.
- The parser is **tolerant**: a malformed file produces a descriptive error
  and is discarded; it must never cause a panic. It must be fuzzed.
- Our own internal model (not the QLC+ format in memory): channels with an
  attribute (intensity, pan, pan fine, tilt, tilt fine, RGB/W/A/UV color,
  color wheel, gobo, shutter/strobe, etc.), modes, multiple heads, physical
  ranges (pan/tilt degrees), named capabilities (color wheel slots with their
  color, gobos, prism, strobe ranges).

### 7.4 Abstract attributes, intentions and palettes
Principle: the user expresses **intentions** ("this group red", "these heads
on the drums"), not DMX values. The core translates the intention for each
fixture according to its definition. It is the working model of professional
consoles (grandMA, MagicQ), with a simple interface.

**Color**
- Abstract color model (desired color in a linear space, e.g. linear RGB or
  CIE xyY) converted per fixture type:
  - RGB / RGBW / RGBA / RGBWAUV: additive mixing (with extra channels
    computed by explicit rules, e.g. white at zero for saturated colors);
  - CMY: subtractive mixing;
  - color wheel: nearest slot in a perceptual space (e.g. OKLab);
  - white only / no color: attribute ignored, the rest of the command applies.
- **Optional per-fixture calibration** (correction matrix), because the same
  red differs between brands.

**Position (moving heads)**
- Pan/tilt in **degrees**, using the ranges of the definition and the patch orientation.
- **Fans** over a group according to its order: linear or symmetric from the
  center. A fan is an intention on the group resolved by the core, so adding a
  head to the group re-spreads it.
- Conversion functions tested with property tests (round trip
  degrees ↔ 16-bit DMX within the channel resolution).

**Beam and more**
- Gobo and prism **by name** (from the definition's capabilities); if the
  fixture does not have that gobo, the attribute is ignored and reported.
- Zoom in **degrees** and strobe in **Hz** when the definition allows it,
  raw value otherwise.
- **Virtual dimmer** for fixtures without an intensity channel (scales RGB).
- **Service channels** (lamp on, reset, etc.) cannot be stored in scenes,
  cues or palettes: they are controlled only through dedicated actions.

**Palettes (in v1)**
- `Palette { id, name, kind: Color | Position | Beam | Intensity, values }`.
- Values are stored **per fixture** (e.g. a position palette remembers the
  pan/tilt of each head) with a generic per-group value as fallback.
- Scenes, cues and effects store **references** to palettes, not copies of
  the values: editing a palette updates every place that uses it.
- When a fixture is replaced with a different model, palettes are
  recomputed from the intention; scenes and cues stay valid.

### 7.5 Functions (FX screen)
- `Scene` — per group or fixture: intentions, palette references or explicit
  values; fade in/out, delays and fades **distributed over the group** (e.g.
  heads lighting up one after the other), different times per attribute.
- `Chaser` — sequence of steps (scene references) with times, direction, mode.
- `CueList` — cues with times, Go/Back/Pause, current cue.
- `MovementEffect` (EFX) — pan/tilt generators: circle, eight, line, square,
  lissajous; size, speed, phase distributed over the group.
- `MatrixEffect` — algorithms over a group with a 2D grid: solid color,
  chase, waves, gradients, text (in Rust, in the core).
- Effects work in **physical units** (degrees, normalised RGB/HSV) and are
  converted to DMX only at the end, using the fixture definition (16-bit
  fine channels included).
- All times (chaser steps, effect duration and speed, fades) can be
  expressed **in milliseconds or in beats**, locked to the musical clock (§8.4).
- **Move in black**: if a head has zero intensity in a cue, the engine moves
  it in advance to the position, color and beam of the next cue in which it
  lights up (can be enabled per cue list and per cue).

### 7.6 Virtual console
A tree of widgets. Each widget has: `id`, `kind`, properties (position,
size, label, **color**), bindings (function, input, feedback) and **runtime
state** managed by the core.

Types for v1 (parity with QLC+):
- Containers: `Frame` (with pages), `SoloFrame` (only one active child).
- Controls: `Button`, `Slider` (level / playback / submaster), `Knob`,
  `XYPad`, `SpeedDial` (tap tempo, multipliers).
- Playback: `CueListWidget`, `AnimationWidget` (matrix control).
- Informational: `Label`, `Clock`.

**Behaviours** (e.g. SoloFrame exclusivity, cue advance) are core rules, not UI rules.

**Design / Show** mode at core level: in Show mode commands that modify the
structure are rejected. Switching to Design from remote clients requires a PIN.

### 7.7 Commands
Exhaustive, serialisable `enum Command`, with a `CommandSource` (local UI,
remote client with id, MIDI device X, OSC, time source, automation,
internal). Every command is validated before being applied; an invalid
command produces a typed error, never a panic. Show editing commands support
undo/redo.

**Source priority**: every `CommandSource` has a priority. The operator (UI,
controller, OSC) always takes precedence over automation: a manual action on
a control overrides what automation is doing on that control until the
operator releases it.

---

## 8. Engine (klight-engine-core)

### 8.1 Tick loop
- Fixed rate **44 Hz** (configurable), on a dedicated high-priority thread.
- **Injected clock** (`trait Clock`): real in production, virtual in tests.
- On every tick:
  1. Drain the command queue (with a per-tick limit to avoid starvation).
  2. Apply the commands and publish a new snapshot if the state changed.
  3. Advance the active functions (fades, chasers, EFX, matrices).
  4. Compute levels and merge.
  5. Publish the frame of each universe to the output drivers (atomically
     replaceable buffer: the driver always takes the latest frame).
  6. Publish state events to the API and input feedback (non-blocking; if a
     consumer is slow, intermediate updates are dropped).
- **No allocation** in the hot path after warm-up (preallocated buffers).
  To be verified by a test using a counting allocator.
- Measure and expose the tick **jitter** (min/avg/max/p99).

### 8.2 Layers (bottom to top)
1. Base cues / scenes / chasers
2. Effects (EFX, matrices)
3. Programmer (Simple Desk, manual changes)
4. Grand master and blackout

### 8.3 Merge
- Intensity: **HTP**. Other attributes: **LTP** with per-layer priority.
- Properties to guarantee with property-based tests, for example:
  - HTP never produces a value lower than any of its inputs.
  - Blackout → all intensity channels at 0, always.
  - Grand master at 0 → no intensity > 0; it does not touch pan/tilt.
  - Fade: value always between start and end, monotonic, reaches the
    target exactly at the end of the time.

### 8.4 Musical clock
- First-class concept in the core: **BPM, beat, bar, phase**.
- Sources, selectable with priority and fallback: tap tempo (v1), MIDI clock
  (v1), Ableton Link (see §17), sync from DJ software (future).
- If the active source is lost, the clock **keeps running at the last known
  BPM** (freewheel) and reports it in the status bar; it never stops.
- Chasers and effects with times in beats stay locked to the phase.
- Test: with a virtual clock, a chaser at 1 step per beat at 120 BPM changes
  step exactly every 500 ms; a BPM change causes no phase jump.

---

## 9. Output drivers (klight-io)

```rust
trait OutputDriver: Send {
    fn open(&mut self) -> Result<(), DriverError>;
    fn send_frame(&mut self, universe: &[u8; 512]) -> Result<(), DriverError>;
    fn close(&mut self);
    fn status(&self) -> DriverStatus;
}
```

- Every driver runs in **its own thread**, managed by a `DeviceManager`.
- On error: `Degraded` state, report, **reconnection with exponential
  backoff** (capped), no impact on the other universes.
- **Hot-plug**: devices are identified by **USB serial number**, not by
  port. If the interface is plugged into another port, its universe returns
  to it automatically.

### 9.1 Enttec DMX USB Pro
- Has a microcontroller: repeats the last frame by itself → preferred for live use.
- Packet protocol: `0x7E`, label, length LSB/MSB, data, `0xE7`.
  Label 6 = *Output Only Send DMX Packet*; data starts with start code `0x00`.
- Send only when the frame changes + a periodic refresh.

### 9.2 Enttec Open DMX
- FTDI chip without intelligence: the PC generates break, MAB and frame.
- 250000 baud, 8N2. Break ≥ 88 µs, MAB ≥ 8 µs, then start code + 512 channels.
- Dedicated high-priority thread; timing measured and exposed.
- If the engine stops, the output stops: show it in the UI
  ("no hardware buffer" badge).

### 9.3 Fake driver
`FakeOutput` that records all frames with timestamps and can simulate errors
(disconnection, latency, partial write). Used in all tests.

Architecture ready for Art-Net/sACN (same trait), not implemented in v1.

---

## 10. Input (klight-input)

- **Global** inputs, not tied to universes (intentional difference from QLC+).
- Every device (MIDI, OSC) has its own thread, translates messages into
  `Command`s through the **mappings stored in the show**, and queues them.
- **MIDI learn**: the client asks the core for learn mode; the core answers
  with the first message received.
- **Hot-plug** with recognition by name/ID and restoration of the mapping.
- **Soft takeover** for faders and knobs: a physical control takes effect
  only after it has "caught" the current value.
- **Time sources** (feeding the musical clock §8.4): tap tempo from any
  mapped input, incoming MIDI clock. Other sources (Ableton Link, DJ
  software) arrive as external processes through the API (§17).

### 10.1 Feedback and colors
- The widget color is reproduced on the controller when supported.
- **Controller profiles** in `profiles/controllers/*.toml`, loaded at runtime:
  - `none` / `on_off`: LED on/off.
  - `palette`: color chosen through velocity or value → the nearest palette
    color in a perceptual space (e.g. OKLab).
  - `rgb_sysex`: SysEx template with exact RGB.
- **QLC+ input profiles** (`.qxi`, Apache 2.0, downloaded by
  `scripts/update-qlcplus-fixtures.sh` into `profiles/qlcplus`, not stored in
  the repository) are read at runtime like `.qxf` fixtures and offered as
  controller profiles. The color table becomes a `palette` profile (nearest
  color) and the MIDI channel table becomes the **LED behaviours** the
  controller can show (brightness levels, pulsing, blinking), selected through
  the MIDI channel of the feedback message. Controllers without a color table
  get on/off from the feedback range; without any feedback they are not listed.
  A TOML profile with the same id takes precedence.
- Per configured controller the user picks the LED behaviour for **widget on**
  (its function is running; default: the profile's own "on" channel) and for
  **widget off** (default: LED off). A third state (armed / next) with its own
  behaviour is future work: for now it blinks between the two.
- Widget **states** → color variants: inactive (dim), active (full),
  next/armed (blinking).
- Send **only differences**, with a per-device rate limit.
- **Full resync** when the device connects and when the engine restarts.

### 10.2 Controllers on a remote client
The remote client acts as a **bridge**: it forwards raw MIDI messages to the
core (labelled with client id + device id) and writes the received feedback
to the device. All mapping logic stays in the core. If the connection drops,
the client shows it locally ("disconnected" LED pattern).
Note: Web MIDI is not available on Safari/iOS.

---

## 11. API (klight-api)

- WebSocket with typed JSON messages (shared types; generate the TypeScript
  types from the Rust types, e.g. `ts-rs` or `specta`).
- Messages: `Command` → `Ack | Error`; subscriptions to state/events; full
  initial snapshot on connection, then incremental updates with a sequence
  number (the client detects gaps and requests a snapshot).
- **Authentication** for non-local clients (PIN/token) and per-client
  **permissions**: full, playback only, read only.
- The API server must never block the tick loop: it communicates only
  through queues and snapshots.
- The engine also serves the static files of the web UI for remote clients.

---

## 12. Persistence (klight-persist)

- **JSON** show file with `schema_version` and explicit **migrations**
  between versions (tested with example files of every version).
- **Atomic write**: temporary file → `fsync` → rename → `fsync` of the
  directory. Never overwrite in place.
- Periodic **autosave** and after changes (with debounce), plus rotating
  backups of the last N saves.
- Separate **runtime state** file (active cues, fader values, mode) saved
  frequently, used by the supervisor to restore after a restart.
- Tolerant loading: a damaged file must not cause a panic; the last valid
  backup is offered.

---

## 13. User interface

Main screens:
1. **Fixtures** — universes, fixture patch from `.qxf`, channels and modes,
   head orientation, groups with order and 2D grid, conflict reporting.
2. **FX** — scenes, chasers, cue lists, movement effects, matrix effects,
   palettes (color, position, beam, intensity).
3. **Virtual Console** — design and use of the widget console.
4. **Simple Desk** — **selection**-based programmer: tap a group (or single
   fixtures), then an intention or a palette ("Red", "Drums", 60° fan). Save
   into a scene or update a palette.
5. **Devices** — outputs (interface for each universe) and global inputs
   (MIDI/OSC, profiles, mappings, MIDI learn).

Always visible:
- **Status bar**: output interface status, connected controllers, tick
  jitter, last autosave, connected remote clients, Design/Show mode, BPM and
  musical clock source.
- **Blackout** and **grand master** always reachable.
- **DMX monitor** (channels per universe) that can be opened from any screen.

Purely local preferences (zoom, theme, current page) stay in the client.

---

## 14. Test strategy

Every feature is complete only with its tests. Levels:

1. **Unit tests** for every module.
2. **Property-based** (`proptest`) on merge, fades, units↔DMX conversions,
   16-bit fine channels, patch validation, abstract color conversion (e.g. a
   color reachable by an RGB fixture comes back identical within the
   quantisation error), orientation and fans, musical clock.
3. **Golden tests**: example show + command sequence + virtual clock → DMX
   frames compared with the reference ones in `tests/golden/`.
4. **Fuzzing** (`cargo-fuzz`) on the `.qxf` parser, show file, API messages,
   OSC, incoming MIDI SysEx. No input must cause a panic.
5. **Chaos tests** (`klight-sim`): fake drivers that disconnect, slow down
   or return random errors; clients that disconnect in the middle of a
   command; killing the engine and checking the restore by the supervisor.
6. **Soak tests**: realistic show for 24–48 h with the real clock; check
   stable memory, jitter within threshold and no lost frames. Nightly in CI
   (short 30-minute version on PRs).
7. **Allocation tests**: the tick loop does not allocate after warm-up.
8. **MIDI feedback tests**: virtual device; for each profile, "green active
   widget" → exact expected bytes.
9. Optional **hardware tests** (feature flag) with real Enttec Pro/Open DMX.

CI on Linux, Windows and macOS: `cargo fmt --check`, `cargo clippy -D warnings`,
tests, UI build, UI tests (Vitest + Playwright for the main flows).

---

## 15. Development plan (milestones)

Proceed in order. Every milestone ends with green tests on all platforms.

- **M0 — Skeleton**: Cargo workspace, empty crates with respected
  boundaries, CI on the three platforms, lint, `CLAUDE.md`,
  `rust-toolchain.toml` with a pinned version, README with the system
  dependencies for Linux, Windows and macOS (no devcontainer).
- **M1 — Model and pure engine**: `klight-model`, `.qxf` parser + fuzzing,
  patch with orientation and validation, groups, **abstract attributes and
  conversions** (color, position, beam), **palettes by reference**, tick
  loop with virtual clock, **musical clock** with tap tempo, layers, merge,
  scenes with fades, `FakeOutput`, golden tests and property tests.
- **M2 — Hardware and reliability**: Enttec Pro and Open DMX drivers,
  `DeviceManager` with hot-plug and backoff, persistence with atomic writes
  and autosave, supervisor with state restore, first chaos tests.
- **M3 — API and basic UI**: WebSocket server, protocol and generated TS
  types, Tauri app, Fixtures and Devices screens (outputs only), status bar,
  DMX monitor, blackout/grand master.
- **M4 — Functions**: chasers, cue lists with move in black, times
  distributed over the group, selection-based Simple Desk, palette editor,
  FX screen (scenes, chasers, cue lists), undo/redo, incoming MIDI clock.
- **M5 — Virtual console and input**: widget tree and behaviours in the
  core, VC editor, Design/Show mode, MIDI with learn, mappings, soft
  takeover, controller profiles and color feedback, OSC.
- **M6 — Effects**: movement effects with distributed phase, fans, matrix
  effects, effects with times in beats, AnimationWidget, XYPad.
- **M7 — Remote and hardening**: remote clients with authentication and
  permissions, remote MIDI bridge, long soak tests, jitter profiling, polish.

---

## 16. Code rules (to be copied into CLAUDE.md)

- In the core (`model`, `engine-core`, `fixtures`): `unwrap`, `expect`,
  `panic!`, unchecked indexing and `unsafe` are **forbidden**. Enable the
  corresponding clippy lints (`unwrap_used`, `expect_used`, `panic`,
  `indexing_slicing`) at `deny` level.
- `unsafe` allowed only in drivers, isolated, commented and justified.
- Errors with explicit types (`thiserror`); no error silently ignored.
- All long-running threads are wrapped by a handler that catches any panic,
  logs it and restarts the component: last line of defence, it does not
  replace the rules above.
- No I/O, `sleep` or blocking lock in the tick loop.
- Every bug found first produces a test that reproduces it, then the fix.
- Small commits, one milestone at a time; do not start the next one without green CI.
- English is the language of the whole project: code, identifiers, comments
  and documentation (including user documentation).

---

## 17. Future developments (after v1)

Not to be implemented now, but the v1 choices must not make them hard.

- **Tracking cue lists**: each cue stores only the differences from the
  previous one; a change "tracks" forward until the cue that changes the same
  attribute. The v1 cue model must be able to evolve in this direction.
- **Hot backup**: a second engine, on another machine, receives the same
  command stream and the same state; if the primary stops responding, it
  takes over the output. Made possible by the core as the single source of
  truth and by determinism.
- **Ableton Link**: the library is GPLv2+ (or Ableton's commercial license).
  If kLight is not GPL, it must be implemented as a **separate process**
  (`klight-link`, GPL) that talks to the core through the API as a time
  source; no direct linking of the library into the core.
- **Automatic music synchronisation** (SoundSwitch style):
  1. track position sources (DJ software/console, audio analysis from line
     input) as external input sources;
  2. offline track analysis (beat grid, sections such as intro, build-up,
     drop, breakdown, energy) in a **separate process**, never in the engine;
  3. automation: "section type → scenes/effects" rules and per-track
     timelines, as a command source with **lower priority than the operator**.
  Check the licenses of audio analysis libraries (some are GPL/AGPL).
- **3D pointing**: with the position of the heads in space, "all point here"
  computes pan/tilt for each one.
- Show/timeline with audio, Art-Net/sACN output, `.qxw` import, 3D visualizer.

---

## 18. Open decisions

To be confirmed with the project owner before the indicated milestone:
- **License** (before M0, because it constrains dependencies and
  contributions): GPLv3 + CLA for contributors (allows integrating GPL
  libraries and selling builds or commercial licenses) or Apache 2.0. Until
  the decision, use only dependencies with permissive licenses (MIT,
  Apache-2.0, BSD, Zlib).
- ~~UI framework~~ — decided: Svelte + TypeScript, with the core's pure
  functions shared through WebAssembly (`klight-wasm`) instead of a Rust UI.
- JSON vs SQLite show file: JSON proposed (before M2).
- Initial list of MIDI controllers to support with a profile (before M5).
