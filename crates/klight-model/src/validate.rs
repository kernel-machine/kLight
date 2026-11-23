//! Show validation: patch conflicts and dangling references.

use serde::{Deserialize, Serialize};

use crate::ids::{FixtureId, UniverseId};
use crate::patch::{PatchedFixture, UNIVERSE_SIZE};
use crate::show::Show;

/// Two fixtures share at least one DMX address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AddressConflict {
    pub universe: UniverseId,
    pub first: FixtureId,
    pub second: FixtureId,
    /// First overlapping address (1-based).
    pub address: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchError {
    #[error("unknown universe {universe}")]
    UnknownUniverse { universe: UniverseId },
    #[error("unknown fixture definition '{definition}'")]
    UnknownDefinition { definition: String },
    #[error("unknown mode '{mode}' for definition '{definition}'")]
    UnknownMode { definition: String, mode: String },
    #[error("address {address} with footprint {footprint} does not fit in a universe")]
    OutOfRange { address: u16, footprint: u16 },
    #[error("address conflict between {first} and {second} at address {address}")]
    Conflict {
        first: FixtureId,
        second: FixtureId,
        address: u16,
    },
}

/// Number of DMX channels a patched fixture occupies, if its definition and mode are known.
pub fn footprint(show: &Show, fixture: &PatchedFixture) -> Option<u16> {
    show.definition_and_mode(fixture)
        .map(|(_, m)| m.footprint())
}

/// Checks that a single fixture is patchable (references and range).
pub fn check_fixture(show: &Show, fixture: &PatchedFixture) -> Result<u16, PatchError> {
    if show.universe(fixture.universe).is_none() {
        return Err(PatchError::UnknownUniverse {
            universe: fixture.universe,
        });
    }
    let def =
        show.definition(&fixture.definition)
            .ok_or_else(|| PatchError::UnknownDefinition {
                definition: fixture.definition.0.clone(),
            })?;
    let mode = def
        .mode(&fixture.mode)
        .ok_or_else(|| PatchError::UnknownMode {
            definition: fixture.definition.0.clone(),
            mode: fixture.mode.clone(),
        })?;
    let fp = mode.footprint();
    let end = usize::from(fixture.address) + usize::from(fp);
    if fixture.address == 0 || fp == 0 || end > UNIVERSE_SIZE + 1 {
        return Err(PatchError::OutOfRange {
            address: fixture.address,
            footprint: fp,
        });
    }
    Ok(fp)
}

/// All address conflicts in the show (each pair reported once).
pub fn address_conflicts(show: &Show) -> Vec<AddressConflict> {
    let mut spans: Vec<(UniverseId, u16, u16, FixtureId)> = show
        .fixtures
        .iter()
        .filter_map(|f| {
            let fp = footprint(show, f)?;
            if fp == 0 {
                return None;
            }
            Some((
                f.universe,
                f.address,
                f.address.saturating_add(fp - 1),
                f.id,
            ))
        })
        .collect();
    spans.sort();
    let mut out = Vec::new();
    for (i, a) in spans.iter().enumerate() {
        for b in spans.iter().skip(i + 1) {
            if b.0 != a.0 || b.1 > a.2 {
                break;
            }
            out.push(AddressConflict {
                universe: a.0,
                first: a.3,
                second: b.3,
                address: b.1.max(a.1),
            });
        }
    }
    out
}

/// Conflicts that a specific fixture would have with the rest of the patch.
pub fn conflicts_for(
    show: &Show,
    fixture: &PatchedFixture,
    footprint_len: u16,
) -> Vec<AddressConflict> {
    let start = fixture.address;
    let end = fixture
        .address
        .saturating_add(footprint_len.saturating_sub(1));
    show.fixtures
        .iter()
        .filter(|o| o.id != fixture.id && o.universe == fixture.universe)
        .filter_map(|o| {
            let fp = footprint(show, o)?;
            let o_end = o.address.saturating_add(fp.saturating_sub(1));
            (o.address <= end && start <= o_end).then(|| AddressConflict {
                universe: fixture.universe,
                first: o.id,
                second: fixture.id,
                address: start.max(o.address),
            })
        })
        .collect()
}

/// Human-readable warnings about dangling references (never fatal: the
/// engine skips what it cannot resolve).
pub fn reference_warnings(show: &Show) -> Vec<String> {
    let mut w = Vec::new();
    for f in &show.fixtures {
        if let Err(e) = check_fixture(show, f) {
            w.push(format!("fixture {}: {e}", f.id));
        }
    }
    for g in &show.groups {
        for m in &g.members {
            if show.fixture(m.fixture).is_none() {
                w.push(format!("group {}: unknown fixture {}", g.id, m.fixture));
            }
        }
    }
    for c in address_conflicts(show) {
        w.push(format!(
            "address conflict in {} at {}: {} and {}",
            c.universe, c.address, c.first, c.second
        ));
    }
    w
}

/// Name for fixture `id`: `desired` if no other fixture uses it, otherwise
/// the next free number (`Spot` → `Spot 1`, `Spot 2`…; a taken `Spot 3`
/// continues with `Spot 4`). Empty names fall back to `fallback`.
pub fn unique_fixture_name(show: &Show, desired: &str, id: FixtureId, fallback: &str) -> String {
    let desired = match desired.trim() {
        "" => fallback.trim(),
        d => d,
    };
    let taken = |n: &str| {
        show.fixtures
            .iter()
            .any(|f| f.id != id && f.name.trim() == n)
    };
    if !taken(desired) {
        return desired.to_owned();
    }
    let (stem, start) = match desired.rsplit_once(' ') {
        Some((stem, num)) if !stem.trim().is_empty() => match num.parse::<u32>() {
            Ok(n) => (stem.trim_end(), n.saturating_add(1)),
            Err(_) => (desired, 1),
        },
        _ => (desired, 1),
    };
    // At most `fixtures.len() + 1` candidates can be taken.
    (start..=start.saturating_add(u32::try_from(show.fixtures.len()).unwrap_or(u32::MAX)))
        .map(|n| format!("{stem} {n}"))
        .find(|c| !taken(c))
        .unwrap_or_else(|| format!("{stem} {}", id.0))
}
