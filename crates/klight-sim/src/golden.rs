//! Golden tests: a scenario (show + timed commands + virtual clock) produces
//! DMX frames that are compared with reference files in `tests/golden/`.
//!
//! Set `KLIGHT_UPDATE_GOLDEN=1` to (re)generate the reference files.

use std::collections::BTreeMap;
use std::path::PathBuf;

use klight_engine_core::{Engine, EngineConfig};
use klight_model::{Command, CommandEnvelope, CommandSource, Show, UniverseId};
use serde::{Deserialize, Serialize};

/// Non-zero channels of one universe at one instant (1-based address -> value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenFrame {
    pub t_us: u64,
    pub universe: u32,
    pub channels: BTreeMap<u16, u8>,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    pub show: Show,
    /// Commands applied at the given times (applied before the tick at that time).
    pub commands: Vec<(u64, Command)>,
    /// Instants at which frames are sampled.
    pub samples: Vec<u64>,
    /// Tick period of the virtual clock.
    pub tick_us: u64,
}

/// Runs a scenario deterministically and returns the sampled frames.
pub fn run(s: &Scenario) -> Result<Vec<GoldenFrame>, String> {
    let mut engine = Engine::new(s.show.clone(), EngineConfig::default());
    let end = s.samples.iter().copied().max().unwrap_or(0);
    let mut commands = s.commands.clone();
    commands.sort_by_key(|(t, _)| *t);
    let mut next_cmd = 0usize;
    let mut samples: Vec<u64> = s.samples.clone();
    samples.sort_unstable();
    let mut next_sample = 0usize;
    let mut out = Vec::new();
    let mut t = 0u64;
    let universes: Vec<UniverseId> = s.show.universes.iter().map(|u| u.id).collect();
    loop {
        while let Some((at, cmd)) = commands.get(next_cmd) {
            if *at > t {
                break;
            }
            let r = engine.apply(
                t,
                CommandEnvelope {
                    id: next_cmd as u64,
                    source: CommandSource::Internal,
                    command: cmd.clone(),
                },
            );
            if let Err(e) = r.result {
                return Err(format!("command {next_cmd} at {at} failed: {e}"));
            }
            next_cmd += 1;
        }
        engine.tick(t);
        while samples.get(next_sample).is_some_and(|&st| st <= t) {
            for u in &universes {
                let frame = engine.frame(*u).ok_or("missing universe")?;
                let channels = frame
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v != 0)
                    .map(|(i, v)| (u16::try_from(i + 1).unwrap_or(u16::MAX), *v))
                    .collect();
                out.push(GoldenFrame {
                    t_us: t,
                    universe: u.0,
                    channels,
                });
            }
            next_sample += 1;
        }
        if t >= end {
            break;
        }
        // Land exactly on sample instants as well as on ticks.
        let next_tick = t + s.tick_us.max(1);
        t = samples
            .get(next_sample)
            .map_or(next_tick, |&st| st.min(next_tick));
    }
    Ok(out)
}

pub fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// Compares the scenario output with its golden file.
pub fn check(s: &Scenario) -> Result<(), String> {
    let frames = run(s)?;
    let path = golden_dir().join(format!("{}.json", s.name));
    let update = std::env::var_os("KLIGHT_UPDATE_GOLDEN").is_some();
    if update || !path.exists() {
        let json = serde_json::to_string_pretty(&frames).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(golden_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&path, json + "\n").map_err(|e| e.to_string())?;
        if !update {
            return Err(format!(
                "golden file {} was missing and has been created; review and commit it",
                path.display()
            ));
        }
        return Ok(());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let expected: Vec<GoldenFrame> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if expected != frames {
        for (e, a) in expected.iter().zip(frames.iter()) {
            if e != a {
                return Err(format!(
                    "{}: first difference at t={} us universe {}:\n expected {:?}\n actual   {:?}",
                    s.name, e.t_us, e.universe, e.channels, a.channels
                ));
            }
        }
        return Err(format!(
            "{}: frame count differs ({} vs {})",
            s.name,
            expected.len(),
            frames.len()
        ));
    }
    Ok(())
}
