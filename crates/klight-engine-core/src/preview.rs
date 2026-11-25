//! What a function does, per fixture and channel, computed by the same
//! compilation the engine runs (groups, palettes and fans resolved).

use std::collections::{BTreeMap, BTreeSet};

use klight_model::inspect::{ChannelOrigin, SceneChannel, SceneFixtureChannels};
use klight_model::{Attribute, ChannelRef, FunctionId, Show};

use crate::compile::{
    CompiledFixture, CompiledFunction, CompiledShow, SceneEntry, COARSE_BYTE, FINE_BYTE, NO_ADDR,
    NO_CHANNEL,
};

/// All the channels of the fixtures a scene drives, with their target values,
/// sorted by universe and address. `None` if the function is not a compiled
/// scene.
pub fn scene_channels(
    show: &Show,
    compiled: &CompiledShow,
    id: FunctionId,
) -> Option<Vec<SceneFixtureChannels>> {
    let CompiledFunction::Scene(scene) = compiled.function(id)? else {
        return None;
    };
    let entries: BTreeMap<u32, &SceneEntry> =
        scene.entries.iter().map(|e| (e.channel, e)).collect();
    let fixtures: BTreeSet<u32> = entries
        .keys()
        .filter_map(|&c| compiled.channels.get(c as usize).map(|lc| lc.fixture))
        .collect();
    let mut out: Vec<SceneFixtureChannels> = fixtures
        .into_iter()
        .filter_map(|f| {
            let cf = compiled.fixtures.get(f as usize)?;
            let pf = show.fixture(cf.id)?;
            Some(SceneFixtureChannels {
                fixture: cf.id,
                fixture_name: pf.name.clone(),
                universe: pf.universe,
                address: pf.address,
                channels: fixture_channels(show, compiled, cf, &pf.mode, &entries),
            })
        })
        .collect();
    out.sort_by_key(|f| (f.universe, f.address));
    Some(out)
}

fn fixture_channels(
    show: &Show,
    compiled: &CompiledShow,
    cf: &CompiledFixture,
    mode: &str,
    entries: &BTreeMap<u32, &SceneEntry>,
) -> Vec<SceneChannel> {
    let def = show.definitions.get(cf.definition);
    let mode = def.and_then(|d| d.mode(mode));
    let name_at = |offset: u16| {
        let index = *mode?.channels.get(usize::from(offset))?;
        let channel = def?.channels.get(usize::from(index))?;
        Some(channel.name.trim().to_owned())
    };
    let channel = |logical: u32, channel: ChannelRef, name: String, fine: bool| {
        let lc = compiled.channels.get(logical as usize)?;
        let entry = entries.get(&logical);
        let byte = if fine { FINE_BYTE } else { COARSE_BYTE };
        let [hi, lo] = entry.map_or(lc.default, |e| e.value).to_be_bytes();
        let address = if fine { lc.fine } else { lc.coarse };
        Some(SceneChannel {
            channel,
            address: (address != NO_ADDR).then(|| address.saturating_add(1)),
            channel_name: name,
            attribute: lc.attribute,
            value: if fine { lo } else { hi },
            origin: match entry {
                Some(e) if e.manual & byte != 0 => ChannelOrigin::Manual,
                Some(e) if e.set & byte != 0 => ChannelOrigin::Value,
                _ => ChannelOrigin::Unset,
            },
            locked: lc.attribute == Attribute::Maintenance,
        })
    };

    let mut out = Vec::new();
    for (head, h) in cf.heads.iter().enumerate() {
        let Ok(head) = u16::try_from(head) else {
            continue;
        };
        if h.vdim == NO_CHANNEL {
            continue;
        }
        let name = if cf.heads.len() > 1 {
            format!("Intensity (virtual dimmer, head {})", u32::from(head) + 1)
        } else {
            "Intensity (virtual dimmer)".to_owned()
        };
        out.extend(channel(
            h.vdim,
            ChannelRef::VirtualDimmer { head },
            name,
            false,
        ));
    }
    let virtual_dimmers = out.len();
    // How many channels of each attribute came before (`ChannelRef::index`).
    let mut seen: Vec<(Attribute, u16)> = Vec::new();
    for (slot, logical) in cf.profile.slots.iter().zip(cf.first_channel..) {
        let attribute = slot.attribute;
        let index = match seen.iter_mut().find(|(a, _)| *a == attribute) {
            Some((_, n)) => {
                *n = n.saturating_add(1);
                *n
            }
            None => {
                seen.push((attribute, 0));
                0
            }
        };
        let bytes = [(false, Some(slot.offset)), (true, slot.fine)];
        for (fine, offset) in bytes {
            let Some(offset) = offset else {
                continue;
            };
            let name = name_at(offset).unwrap_or_else(|| format!("Channel {}", offset + 1));
            let reference = ChannelRef::Dmx {
                attribute,
                index,
                fine,
            };
            out.extend(channel(logical, reference, name, fine));
        }
    }
    // A fine byte does not have to follow its coarse one.
    if let Some(dmx) = out.get_mut(virtual_dimmers..) {
        dmx.sort_by_key(|c| c.address);
    }
    out
}
