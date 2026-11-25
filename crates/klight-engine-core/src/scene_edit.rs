//! Edits of the values of a scene, used by editors: which older values a new
//! one replaces depends on the channels the engine resolves them to.

use std::collections::BTreeSet;

use klight_model::{ChannelRef, FixtureId, SceneValue, Show, Target, ValueSource};

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

/// `values` after applying `incoming` (intentions or palettes): values in the
/// same slot are replaced, and channels set by hand that `incoming` writes are
/// released, so that the new values show on them.
pub fn add_scene_values(
    show: &Show,
    compiled: &CompiledShow,
    values: &[SceneValue],
    incoming: &[SceneValue],
) -> Vec<SceneValue> {
    let (entries, _) = compiled.resolve_values(show, incoming);
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
    values
        .iter()
        .filter(|v| !incoming.iter().any(|i| same_slot(v, i)) && !overwritten(v))
        .chain(incoming)
        .cloned()
        .collect()
}
