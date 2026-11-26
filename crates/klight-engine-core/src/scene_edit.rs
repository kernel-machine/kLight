//! Edits of the lights and values of a scene, used by editors: which older
//! values a new one replaces depends on the channels the engine resolves them
//! to.

use std::collections::{BTreeMap, BTreeSet};

use klight_model::inspect::{target_fixtures, CoveredValue};
use klight_model::{ChannelRef, FixtureId, Scene, SceneValue, Show, Target, ValueSource};

use crate::compile::{CompiledShow, COARSE_BYTE, FINE_BYTE, NO_ADDR};

/// `values` with one channel of a fixture set by hand to `value`, or released
/// (back to what the other values set) when `value` is `None`.
pub fn set_scene_channel(
    values: &[SceneValue],
    fixture: FixtureId,
    channel: ChannelRef,
    value: Option<u8>,
) -> Vec<SceneValue> {
    let target = Target::Fixture { id: fixture };
    let mut out: Vec<SceneValue> = values
        .iter()
        .filter(|v| {
            !(v.target == target
                && matches!(v.value, ValueSource::Channel { channel: c, .. } if c == channel))
        })
        .cloned()
        .collect();
    // Last, so that it wins over the intentions on the same channel.
    if let Some(value) = value {
        out.push(SceneValue {
            target,
            value: ValueSource::Channel { channel, value },
        });
    }
    out
}

/// Same target and same thing set: the newer value replaces the older one.
fn same_slot(a: &SceneValue, b: &SceneValue) -> bool {
    a.target == b.target
        && match (&a.value, &b.value) {
            (ValueSource::Explicit { value: a }, ValueSource::Explicit { value: b }) => {
                a.same_slot(b)
            }
            (ValueSource::Palette { id: a }, ValueSource::Palette { id: b }) => a == b,
            (ValueSource::Channel { channel: a, .. }, ValueSource::Channel { channel: b, .. }) => {
                a == b
            }
            _ => false,
        }
}

/// `scene` after applying `incoming` (intentions or palettes): the lights it
/// names join the scene, values in the same slot are replaced, and channels set
/// by hand that `incoming` writes are released, so that the new values show on
/// them.
pub fn add_scene_values(
    show: &Show,
    compiled: &CompiledShow,
    scene: &Scene,
    incoming: &[SceneValue],
) -> Scene {
    let named: Vec<Target> = incoming.iter().map(|v| v.target).collect();
    let mut out = add_scene_members(show, scene, &named);
    let (entries, _) = compiled.resolve_values(show, incoming, &out.members);
    let written: BTreeSet<u32> = entries.iter().map(|e| e.channel).collect();
    let overwritten = |v: &SceneValue| {
        let (ValueSource::Channel { channel, .. }, Target::Fixture { id }) = (&v.value, v.target)
        else {
            return false;
        };
        compiled
            .fixture_index
            .get(&id)
            .and_then(|&f| compiled.locate_channel(f, *channel))
            .is_some_and(|(logical, _)| written.contains(&logical))
    };
    out.values
        .retain(|v| !incoming.iter().any(|i| same_slot(v, i)) && !overwritten(v));
    out.values.extend_from_slice(incoming);
    out
}

/// `scene` with more lights: groups, which stay linked to the group, and
/// fixtures (a head stands for its fixture). Lights already in it are left out.
/// Unless the scene says that new lights follow it, the values of the whole
/// scene stay with the lights it had.
pub fn add_scene_members(show: &Show, scene: &Scene, lights: &[Target]) -> Scene {
    let mut out = scene.clone();
    // Groups first: a fixture of one of them is then already in.
    let groups = lights
        .iter()
        .copied()
        .filter(|t| matches!(t, Target::Group { .. }));
    let fixtures = lights.iter().filter_map(|t| match *t {
        Target::Fixture { id } | Target::Head { id, .. } => Some(Target::Fixture { id }),
        Target::Group { .. } | Target::Scene => None,
    });
    for light in groups.chain(fixtures) {
        let covered = match light {
            Target::Fixture { id } => out
                .members
                .iter()
                .any(|m| target_fixtures(show, m).contains(&id)),
            _ => out.members.contains(&light),
        };
        if !covered {
            out.members.push(light);
        }
    }
    if !out.new_members_follow && out.members.len() > scene.members.len() {
        out.values = keep_with(&scene.values, &scene.members);
    }
    out
}

/// `values` with each value of the whole scene turned into one per member.
fn keep_with(values: &[SceneValue], members: &[Target]) -> Vec<SceneValue> {
    let split: Vec<SceneValue> = values
        .iter()
        .flat_map(|v| match v.target {
            Target::Scene => members
                .iter()
                .map(|&target| SceneValue {
                    target,
                    value: v.value.clone(),
                })
                .collect(),
            _ => vec![v.clone()],
        })
        .collect();
    // A later value in the same slot hides an earlier one.
    let mut out: Vec<SceneValue> = Vec::with_capacity(split.len());
    for v in split.into_iter().rev() {
        if !out.iter().any(|later| same_slot(&v, later)) {
            out.push(v);
        }
    }
    out.reverse();
    out
}

/// `scene` without one of its lights, and without the values that were its own.
/// A fixture still in the scene through another member keeps them.
pub fn remove_scene_member(show: &Show, scene: &Scene, light: Target) -> Scene {
    let mut out = scene.clone();
    if !out.members.contains(&light) {
        return out;
    }
    out.members.retain(|&m| m != light);
    let kept: Vec<FixtureId> = out
        .members
        .iter()
        .flat_map(|m| target_fixtures(show, m))
        .collect();
    let gone: Vec<FixtureId> = target_fixtures(show, &light)
        .into_iter()
        .filter(|f| !kept.contains(f))
        .collect();
    out.values.retain(|v| match v.target {
        Target::Fixture { id } | Target::Head { id, .. } => !gone.contains(&id),
        Target::Group { .. } => v.target != light,
        Target::Scene => true,
    });
    out
}

/// The bytes each value of `scene` writes on its own, per logical channel. A
/// channel without a fine byte counts as whole whichever byte is set.
fn written_bytes(
    show: &Show,
    compiled: &CompiledShow,
    scene: &Scene,
    value: &SceneValue,
) -> BTreeMap<u32, u8> {
    let (entries, _) = compiled.resolve_values(show, std::slice::from_ref(value), &scene.members);
    entries
        .iter()
        .map(|e| {
            let single = compiled
                .channels
                .get(e.channel as usize)
                .is_some_and(|c| c.fine == NO_ADDR);
            let bytes = if single {
                COARSE_BYTE | FINE_BYTE
            } else {
                e.set
            };
            (e.channel, bytes)
        })
        .collect()
}

/// The values of `scene` that later values override, as the engine resolves
/// them (the last value wins on every channel byte). Values that write nothing
/// (e.g. a gobo the fixtures lack) are not listed.
pub fn covered_values(show: &Show, compiled: &CompiledShow, scene: &Scene) -> Vec<CoveredValue> {
    let written: Vec<BTreeMap<u32, u8>> = scene
        .values
        .iter()
        .map(|v| written_bytes(show, compiled, scene, v))
        .collect();
    let mut out = Vec::new();
    for (i, mine) in written.iter().enumerate() {
        if mine.is_empty() {
            continue;
        }
        let mut by = Vec::new();
        let mut covered: BTreeMap<u32, u8> = BTreeMap::new();
        for (j, later) in written.iter().enumerate().skip(i + 1) {
            let mut overlaps = false;
            for (channel, &bytes) in mine {
                let common = later.get(channel).map_or(0, |&b| b & bytes);
                if common != 0 {
                    overlaps = true;
                    *covered.entry(*channel).or_insert(0) |= common;
                }
            }
            if overlaps {
                by.push(u32::try_from(j).unwrap_or(u32::MAX));
            }
        }
        if by.is_empty() {
            continue;
        }
        let full = mine
            .iter()
            .all(|(channel, &bytes)| covered.get(channel).is_some_and(|&c| c & bytes == bytes));
        out.push(CoveredValue {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            by,
            full,
        });
    }
    out
}

/// `scene` without the values that later values override entirely.
pub fn remove_covered_values(show: &Show, compiled: &CompiledShow, scene: &Scene) -> Scene {
    let gone: BTreeSet<usize> = covered_values(show, compiled, scene)
        .into_iter()
        .filter(|c| c.full)
        .map(|c| c.index as usize)
        .collect();
    let mut out = scene.clone();
    out.values = scene
        .values
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone.contains(i))
        .map(|(_, v)| v.clone())
        .collect();
    out
}
