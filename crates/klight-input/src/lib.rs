//! Global inputs (MIDI, OSC): mappings stored in the show, soft takeover,
//! MIDI learn, controller profiles and colored feedback.

pub mod feedback;
pub mod manager;
pub mod mapping;
pub mod midi;
pub mod osc;

use std::collections::BTreeMap;
use std::path::Path;

pub use feedback::{
    Behavior, ControllerProfile, FeedbackTime, FeedbackTracker, Look, ProfileKind, WidgetState,
};
pub use manager::{InputDeviceStatus, InputHost, InputManager, Learned};
pub use mapping::{InputEvent, Mapper, SoftTakeover};

/// Loads every QLC+ input profile (`*.qxi`) of a directory, keyed by file
/// name without extension. Controllers without any feedback, and files that
/// cannot be read or parsed, are skipped.
pub fn load_qxi_profiles(dir: &Path) -> BTreeMap<String, ControllerProfile> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "qxi") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| klight_fixtures::parse_qxi(&t).map_err(|e| e.to_string()))
        {
            Ok(def) => {
                if let Some(p) = ControllerProfile::from_qxi(&def) {
                    out.insert(stem, p);
                }
            }
            Err(e) => tracing::warn!("input profile {} rejected: {e}", path.display()),
        }
    }
    out
}

/// Loads every `*.toml` profile of a directory (invalid files are logged and skipped).
pub fn load_profiles(dir: &Path) -> BTreeMap<String, ControllerProfile> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "toml") {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| ControllerProfile::from_toml(&t).map_err(|e| e.to_string()))
            {
                Ok(p) => {
                    out.insert(stem, p);
                }
                Err(e) => tracing::warn!("controller profile {} rejected: {e}", path.display()),
            }
        }
    }
    out
}
