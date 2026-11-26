//! Edits of the lights and values of a scene, used by editors: which older
//! values a new one replaces depends on the channels the engine resolves them
//! to.

use std::collections::BTreeSet;

use klight_model::inspect::target_fixtures;
use klight_model::{ChannelRef, FixtureId, Scene, SceneValue, Show, Target, ValueSource};

use crate::compile::CompiledShow;

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
