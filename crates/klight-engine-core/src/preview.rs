//! What a function does, per fixture and channel, computed by the same
//! compilation the engine runs (groups, palettes and fans resolved).

use klight_model::inspect::{FixturePreview, PreviewChannel};
use klight_model::{FunctionId, Show};

use crate::compile::{CompiledFunction, CompiledShow, Role, NO_ADDR};

/// Target values of a scene, grouped by fixture (sorted by universe and
/// address). `None` if the function is not a compiled scene.
pub fn scene_preview(
    show: &Show,
    compiled: &CompiledShow,
    id: FunctionId,
) -> Option<Vec<FixturePreview>> {
    let CompiledFunction::Scene(scene) = compiled.function(id)? else {
        return None;
    };
    let mut out: Vec<FixturePreview> = Vec::new();
    for e in &scene.entries {
        let Some(lc) = compiled.channels.get(e.channel as usize) else {
            continue;
        };
        let Some(cf) = compiled.fixtures.get(lc.fixture as usize) else {
            continue;
        };
        let Some(pf) = show.fixture(cf.id) else {
            continue;
        };
        let def = show.definitions.get(cf.definition);
        let slot = e
            .channel
            .checked_sub(cf.first_channel)
            .and_then(|s| cf.profile.slots.get(s as usize));
        let channel_name = match (lc.role, slot) {
            (Role::VirtualDimmer, _) => "Intensity (virtual dimmer)".to_owned(),
            (_, Some(slot)) => def
                .and_then(|d| d.channels.get(usize::from(slot.channel)))
                .map_or_else(
                    || format!("Channel {}", slot.offset + 1),
                    |c| c.name.trim().to_owned(),
                ),
            _ => "Channel".to_owned(),
        };
        let [hi, lo] = e.value.to_be_bytes();
        let addr = |a: u16| (a != NO_ADDR).then(|| a.saturating_add(1));
        let channel = PreviewChannel {
            address: addr(lc.coarse),
            channel_name,
            attribute: lc.attribute,
            value: hi,
            fine_address: addr(lc.fine),
            fine_value: (lc.fine != NO_ADDR).then_some(lo),
        };
        match out.iter_mut().find(|f| f.fixture == cf.id) {
            Some(f) => f.channels.push(channel),
            None => out.push(FixturePreview {
                fixture: cf.id,
                fixture_name: pf.name.clone(),
                universe: pf.universe,
                address: pf.address,
                channels: vec![channel],
            }),
        }
    }
    for f in &mut out {
        // Virtual dimmers (no address) first, then by address.
        f.channels.sort_by_key(|c| c.address.unwrap_or(0));
    }
    out.sort_by_key(|f| (f.universe, f.address));
    Some(out)
}
