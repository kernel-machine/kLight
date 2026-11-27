//! What a function does, per fixture and channel, computed by the same
//! compilation the engine runs (groups, palettes and fans resolved).

use std::collections::{BTreeMap, BTreeSet};

use klight_model::inspect::{
    scene_lights, ChannelOrigin, MovementHead, MovementReach, SceneChannel, SceneFixtureChannels,
};
use klight_model::{
    Attribute, ChannelRef, FunctionId, FunctionKind, MovementEffect, MovementMirror, Show,
};

use crate::compile::{
    CompiledFixture, CompiledFunction, CompiledShow, SceneEntry, COARSE_BYTE, FINE_BYTE, NO_ADDR,
    NO_CHANNEL,
};
use crate::effects::movement_path_point;

/// All the channels of the fixtures of a scene (its lights, and any other its
/// values drive), with their target values,
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
    let FunctionKind::Scene(model) = &show.function(id)?.kind else {
        return None;
    };
    let lights = scene_lights(show, model).fixtures;
    let fixtures: BTreeSet<u32> = entries
        .keys()
        .filter_map(|&c| compiled.channels.get(c as usize).map(|lc| lc.fixture))
        .chain(
            lights
                .iter()
                .filter_map(|f| compiled.fixture_index.get(f).copied()),
        )
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
                Some(_) => ChannelOrigin::Paired,
                None => ChannelOrigin::Unset,
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

/// Samples of a cycle used to find how far a shape goes.
const REACH_SAMPLES: u32 = 720;

/// Where the heads of movement effect `m` can go, and whether its shape goes
/// beyond: what the editor draws around the shape. The heads and their ranges
/// are those the engine plays ([`CompiledShow::movement_members`]); a head
/// beyond its range stops at its limit, as [`degrees_to_dmx16`] clamps.
///
/// [`degrees_to_dmx16`]: klight_fixtures::convert::degrees_to_dmx16
pub fn movement_reach(show: &Show, compiled: &CompiledShow, m: &MovementEffect) -> MovementReach {
    // How far the shape goes from 0 on each axis, at any phase, mirrored or
    // not. A relative shape is measured as if the scenes left the heads at
    // the center of their range.
    let (mut far_pan, mut far_tilt) = (0.0_f32, 0.0_f32);
    for mirrored in [false, m.mirror != MovementMirror::None] {
        for k in 0..=REACH_SAMPLES {
            let (p, t) = movement_path_point(m, k as f32 / REACH_SAMPLES as f32, mirrored);
            far_pan = far_pan.max(p.abs());
            far_tilt = far_tilt.max(t.abs());
        }
    }
    // Rounding of the drawing, not a real overshoot.
    const SLACK: f32 = 0.01;

    let mut reach = MovementReach {
        heads: Vec::new(),
        pan_limit: None,
        tilt_limit: None,
        clipped: false,
    };
    let narrowest = |limit: &mut Option<f32>, half: f32| {
        *limit = Some(limit.map_or(half, |l| l.min(half)));
    };
    for (fi, head, moving) in compiled.movement_members(show, m) {
        let Some(cf) = compiled.fixtures.get(fi as usize) else {
            continue;
        };
        let fixture_name = show
            .fixture(cf.id)
            .map_or_else(String::new, |f| f.name.clone());
        let name = if cf.heads.len() > 1 {
            format!("{fixture_name} · {}", u32::from(head) + 1)
        } else {
            fixture_name
        };
        reach.heads.push(MovementHead {
            fixture: cf.id,
            head,
            name,
            moves: moving.is_some(),
            mirrored: moving.is_some_and(|h| h.mirrored),
        });
        let Some(h) = moving else { continue };
        // The effect's pan goes to the tilt channel when the axes are swapped.
        let (pan_channel, pan_range, tilt_channel, tilt_range) = if h.orientation.swap_pan_tilt {
            (h.tilt, h.tilt_range, h.pan, h.pan_range)
        } else {
            (h.pan, h.pan_range, h.tilt, h.tilt_range)
        };
        if pan_channel != NO_CHANNEL && pan_range > 0.0 {
            narrowest(&mut reach.pan_limit, pan_range / 2.0);
            reach.clipped |= far_pan > pan_range / 2.0 + SLACK;
        }
        if tilt_channel != NO_CHANNEL && tilt_range > 0.0 {
            narrowest(&mut reach.tilt_limit, tilt_range / 2.0);
            reach.clipped |= far_tilt > tilt_range / 2.0 + SLACK;
        }
    }
    reach
}
