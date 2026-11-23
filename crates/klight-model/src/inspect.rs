//! Read-only views of the patch used by clients (through `klight-wasm`) and
//! by the core. One implementation, so the UI never re-implements them.

use serde::{Deserialize, Serialize};

use crate::fixture_def::Attribute;
use crate::ids::{FixtureId, UniverseId};
use crate::patch::{PatchedFixture, UNIVERSE_SIZE};
use crate::show::Show;
use crate::validate;

/// What is patched on one DMX address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChannelInfo {
    pub fixture: FixtureId,
    pub fixture_name: String,
    /// Position of the fixture in the universe, by address (0-based).
    pub fixture_index: u16,
    /// Offset of the channel inside the fixture (0 = first channel).
    pub offset: u16,
    pub channel_name: String,
    pub attribute: Attribute,
}

/// Summary of a patched fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixtureInfo {
    pub id: FixtureId,
    /// Number of DMX channels (0 if the definition or mode is missing).
    pub footprint: u16,
    pub heads: u16,
    pub attributes: Vec<Attribute>,
    /// Has pan or tilt (moving head / scanner).
    pub has_position: bool,
    /// Has additive color mixing.
    pub has_color_mixing: bool,
}

pub fn fixture_info(show: &Show, f: &PatchedFixture) -> FixtureInfo {
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut heads = 1;
    let mut footprint = 0;
    if let Some((def, mode)) = show.definition_and_mode(f) {
        footprint = mode.footprint();
        heads = u16::try_from(mode.heads.len().max(1)).unwrap_or(u16::MAX);
        for &ci in &mode.channels {
            if let Some(ch) = def.channels.get(usize::from(ci)) {
                if !attributes.contains(&ch.attribute) {
                    attributes.push(ch.attribute);
                }
            }
        }
    }
    FixtureInfo {
        id: f.id,
        footprint,
        heads,
        has_position: attributes
            .iter()
            .any(|a| matches!(a, Attribute::Pan | Attribute::Tilt)),
        has_color_mixing: attributes.iter().any(|a| a.is_additive_color()),
        attributes,
    }
}

/// The 512 addresses of a universe (`None` = not patched).
pub fn channel_map(show: &Show, universe: UniverseId) -> Vec<Option<ChannelInfo>> {
    let mut map: Vec<Option<ChannelInfo>> = vec![None; UNIVERSE_SIZE];
    let mut fixtures: Vec<&PatchedFixture> = show
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .collect();
    fixtures.sort_by_key(|f| (f.address, f.id));
    for (index, f) in fixtures.into_iter().enumerate() {
        let Some((def, mode)) = show.definition_and_mode(f) else {
            continue;
        };
        for (offset, &ci) in mode.channels.iter().enumerate() {
            let slot = usize::from(f.address.saturating_sub(1)) + offset;
            let Some(cell) = map.get_mut(slot) else {
                continue;
            };
            if cell.is_some() {
                continue; // conflicting patch: the first fixture keeps the address
            }
            let ch = def.channels.get(usize::from(ci));
            *cell = Some(ChannelInfo {
                fixture: f.id,
                fixture_name: f.name.clone(),
                fixture_index: u16::try_from(index).unwrap_or(u16::MAX),
                offset: u16::try_from(offset).unwrap_or(u16::MAX),
                channel_name: ch.map_or_else(
                    || format!("Channel {}", offset + 1),
                    |c| c.name.trim().to_owned(),
                ),
                attribute: ch.map_or(Attribute::Other, |c| c.attribute),
            });
        }
    }
    map
}

/// First address (1-based) from `from` where `size` channels fit without
/// overlapping any fixture of the universe.
pub fn next_free_address(show: &Show, universe: UniverseId, size: u16, from: u16) -> Option<u16> {
    if size == 0 {
        return None;
    }
    let mut spans: Vec<(u16, u16)> = show
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .filter_map(|f| {
            let fp = validate::footprint(show, f)?;
            (fp > 0).then(|| (f.address, f.address.saturating_add(fp - 1)))
        })
        .collect();
    spans.sort_unstable();
    let mut candidate = from.max(1);
    for (start, end) in spans {
        if u32::from(candidate) + u32::from(size) - 1 < u32::from(start) {
            break;
        }
        if end >= candidate {
            candidate = end.saturating_add(1);
        }
    }
    (u32::from(candidate) + u32::from(size) - 1 <= UNIVERSE_SIZE as u32).then_some(candidate)
}

/// Fixtures that a span of `size` channels at `address` would overlap.
pub fn overlapping(
    show: &Show,
    universe: UniverseId,
    address: u16,
    size: u16,
    exclude: Option<FixtureId>,
) -> Vec<FixtureId> {
    let probe = PatchedFixture {
        id: exclude.unwrap_or(FixtureId(u32::MAX)),
        name: String::new(),
        definition: crate::ids::DefinitionKey(String::new()),
        mode: String::new(),
        universe,
        address,
        orientation: Default::default(),
        calibration: None,
    };
    validate::conflicts_for(show, &probe, size)
        .into_iter()
        .map(|c| c.first)
        .collect()
}

/// One channel set by a function, as the engine computes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PreviewChannel {
    /// DMX address (1-based); `None` for a virtual dimmer (fixture without
    /// intensity channel: it scales the color channels).
    pub address: Option<u16>,
    pub channel_name: String,
    pub attribute: Attribute,
    /// 8-bit value of the coarse channel.
    pub value: u8,
    /// Address and value of the fine byte for 16-bit channels.
    pub fine_address: Option<u16>,
    pub fine_value: Option<u8>,
}

/// What a function does to one fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixturePreview {
    pub fixture: FixtureId,
    pub fixture_name: String,
    pub universe: UniverseId,
    pub address: u16,
    pub channels: Vec<PreviewChannel>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_def::{
        ChannelByte, ChannelDef, FixtureDefinition, FixtureKind, ModeDef, Physical,
    };
    use crate::patch::{Universe, UNIVERSE_SIZE};

    fn show(addresses: &[u16]) -> Show {
        let mut s = Show::new("t");
        s.universes.push(Universe {
            id: UniverseId(1),
            name: "U".into(),
            output: None,
        });
        let ch = |name: &str, attribute| ChannelDef {
            name: name.into(),
            attribute,
            byte: ChannelByte::Coarse,
            default: 0,
            capabilities: vec![],
        };
        s.definitions.push(FixtureDefinition {
            manufacturer: "G".into(),
            model: "P".into(),
            kind: FixtureKind::Other,
            channels: vec![
                ch("Pan", Attribute::Pan),
                ch("Red ", Attribute::Red),
                ch("D", Attribute::Dimmer),
            ],
            modes: vec![ModeDef {
                name: "m".into(),
                channels: vec![0, 1, 2, 2, 2, 2],
                heads: vec![],
                physical: None,
            }],
            physical: Physical::default(),
        });
        for (i, a) in addresses.iter().enumerate() {
            s.fixtures.push(PatchedFixture {
                id: FixtureId(i as u32 + 1),
                name: format!("f{i}"),
                definition: crate::ids::DefinitionKey::new("G", "P"),
                mode: "m".into(),
                universe: UniverseId(1),
                address: *a,
                orientation: Default::default(),
                calibration: None,
            });
        }
        s
    }

    #[test]
    fn next_free_address_cases() {
        let u = UniverseId(1);
        assert_eq!(next_free_address(&show(&[]), u, 6, 1), Some(1));
        assert_eq!(next_free_address(&show(&[1]), u, 6, 1), Some(7));
        assert_eq!(next_free_address(&show(&[1, 13]), u, 6, 1), Some(7));
        assert_eq!(next_free_address(&show(&[1, 10]), u, 6, 1), Some(16));
        assert_eq!(next_free_address(&show(&[]), u, 6, 508), None);
        assert_eq!(next_free_address(&show(&[]), u, 0, 1), None);
    }

    #[test]
    fn overlaps_and_channel_map() {
        let s = show(&[1, 20]);
        let u = UniverseId(1);
        assert_eq!(overlapping(&s, u, 5, 6, None), vec![FixtureId(1)]);
        assert!(overlapping(&s, u, 7, 6, None).is_empty());
        assert!(overlapping(&s, u, 1, 6, Some(FixtureId(1))).is_empty());
        let map = channel_map(&s, u);
        assert_eq!(map.len(), UNIVERSE_SIZE);
        let c = map[1].as_ref().unwrap();
        assert_eq!(
            (
                c.fixture,
                c.offset,
                c.channel_name.as_str(),
                c.fixture_index
            ),
            (FixtureId(1), 1, "Red", 0)
        );
        assert_eq!(map[19].as_ref().unwrap().fixture_index, 1);
        assert!(map[6].is_none());
        let info = fixture_info(&s, &s.fixtures[0]);
        assert!(info.has_position && info.has_color_mixing);
        assert_eq!(info.footprint, 6);
    }
}
