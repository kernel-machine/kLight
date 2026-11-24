# Rules for working on kLight

`SPEC.md` is the source of truth. Read it before writing code. If an
implementation choice contradicts it, stop and ask instead of deviating.

## Code rules (SPEC §16)

- In the core (`klight-model`, `klight-engine-core`, `klight-fixtures`):
  `unwrap`, `expect`, `panic!`, unchecked indexing and `unsafe` are forbidden.
  The clippy lints `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`
  are `deny`, and `unsafe_code` is `forbid` (tests may use unwrap via `clippy.toml`).
- `unsafe` is allowed only in drivers, isolated, commented and justified.
- Errors use explicit types (`thiserror`); no error is silently ignored.
- Every long-running thread is wrapped by a handler that catches panics,
  logs them and restarts the component (last line of defence only).
- No I/O, `sleep` or blocking lock inside the tick loop. No allocation in the
  steady-state tick (checked by `crates/klight-sim/tests/allocation.rs`).
- Every bug first gets a test that reproduces it, then the fix.
- Small commits, one milestone at a time; do not start the next one without green CI.
- English is the language of the whole project: code, identifiers, comments and all documentation.
- Do not add dependencies to the core without justifying them in the commit.
  kLight is GPL-3.0-or-later: dependencies must be GPLv3-compatible
  (`deny.toml`; GPL-2.0-only is not).

## Dependency rule

`model` depends on nothing in the project; `engine-core` only on `model` and
`fixtures`. I/O lives only in `io`, `input`, `persist`, `api` and the binaries.

## No duplicated logic

The UI must not re-implement core logic. If the UI needs to compute something
the engine computes (previews, layouts, patch checks, conversions), add a pure
function to the core and expose it through `crates/klight-wasm`. Behaviour of
the show (e.g. fans) is a core intention, never computed by a client.

## Checks before every commit

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm run check && npm test && npm run build   # also rebuilds klight-wasm
```

After changing protocol or model types: `cd ui && npm run bindings`.
After an intended output change: `KLIGHT_UPDATE_GOLDEN=1 cargo test -p klight-sim --test golden`
and review the diff in `tests/golden/`.
