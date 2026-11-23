//! Resolved view of a definition + mode: logical channels (coarse + optional
//! fine byte) grouped by head.

use klight_model::{Attribute, ChannelByte, ChannelDef, FixtureDefinition, ModeDef, Physical};

/// A logical channel of a fixture: a coarse DMX channel plus an optional fine one.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    /// Offset of the coarse byte from the fixture address (0-based).
    pub offset: u16,
    /// Offset of the fine byte, if the channel is 16-bit.
    pub fine: Option<u16>,
    pub attribute: Attribute,
    /// Index into `FixtureDefinition::channels`.
    pub channel: u16,
    /// Default value (16-bit scale).
    pub default: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadProfile {
    /// Indices into `FixtureProfile::slots` belonging to this head (and
    /// fixture-wide slots as fallback, after the head's own).
    pub slots: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureProfile {
    pub slots: Vec<Slot>,
    pub heads: Vec<HeadProfile>,
    pub physical: Physical,
    pub footprint: u16,
}

pub fn to16(v: u8) -> u16 {
    u16::from(v) * 257
}

impl FixtureProfile {
    pub fn build(def: &FixtureDefinition, mode: &ModeDef) -> Self {
        let mut slots: Vec<Slot> = Vec::new();
        // Mode position -> slot index, used to build heads.
        let mut pos_to_slot: Vec<Option<u16>> = Vec::with_capacity(mode.channels.len());

        let channel_at = |idx: u16| -> Option<&ChannelDef> { def.channels.get(usize::from(idx)) };

        let head_of = |pos: u16| -> Option<usize> {
            mode.heads.iter().position(|h| h.channels.contains(&pos))
        };

        for (pos, &ch_idx) in mode.channels.iter().enumerate() {
            let pos = u16::try_from(pos).unwrap_or(u16::MAX);
            let Some(ch) = channel_at(ch_idx) else {
                pos_to_slot.push(None);
                continue;
            };
            if ch.byte == ChannelByte::Fine {
                // Pair with the closest preceding coarse slot of the same
                // attribute and head that has no fine byte yet.
                let my_head = head_of(pos);
                let partner = slots.iter_mut().enumerate().rev().find(|(_, s)| {
                    s.attribute == ch.attribute && s.fine.is_none() && head_of(s.offset) == my_head
                });
                if let Some((i, s)) = partner {
                    s.fine = Some(pos);
                    pos_to_slot.push(u16::try_from(i).ok());
                    continue;
                }
            }
            pos_to_slot.push(u16::try_from(slots.len()).ok());
            slots.push(Slot {
                offset: pos,
                fine: None,
                attribute: ch.attribute,
                channel: ch_idx,
                default: to16(ch.default),
            });
        }

        let all: Vec<u16> = (0..slots.len())
            .filter_map(|i| u16::try_from(i).ok())
            .collect();
        let heads = if mode.heads.is_empty() {
            vec![HeadProfile { slots: all }]
        } else {
            let in_any_head = |s: &Slot| head_of(s.offset).is_some();
            let wide: Vec<u16> = all
                .iter()
                .copied()
                .filter(|&i| slots.get(usize::from(i)).is_some_and(|s| !in_any_head(s)))
                .collect();
            mode.heads
                .iter()
                .map(|h| {
                    let mut own: Vec<u16> = Vec::new();
                    for &p in &h.channels {
                        if let Some(Some(si)) = pos_to_slot.get(usize::from(p)) {
                            if !own.contains(si) {
                                own.push(*si);
                            }
                        }
                    }
                    own.extend(wide.iter().copied());
                    HeadProfile { slots: own }
                })
                .collect()
        };

        Self {
            slots,
            heads,
            physical: def.physical_for(mode),
            footprint: mode.footprint(),
        }
    }

    pub fn head_count(&self) -> u16 {
        u16::try_from(self.heads.len()).unwrap_or(u16::MAX)
    }

    /// First slot of `head` with the given attribute (head's own slots first).
    pub fn find(&self, head: u16, attribute: Attribute) -> Option<(u16, &Slot)> {
        let h = self.heads.get(usize::from(head))?;
        h.slots.iter().find_map(|&i| {
            let s = self.slots.get(usize::from(i))?;
            (s.attribute == attribute).then_some((i, s))
        })
    }

    pub fn has(&self, head: u16, attribute: Attribute) -> bool {
        self.find(head, attribute).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klight_model::{FixtureKind, HeadDef};

    fn ch(name: &str, a: Attribute, b: ChannelByte) -> ChannelDef {
        ChannelDef {
            name: name.into(),
            attribute: a,
            byte: b,
            default: 0,
            capabilities: vec![],
        }
    }

    #[test]
    fn pairs_fine_channels_and_heads() {
        let def = FixtureDefinition {
            manufacturer: "T".into(),
            model: "M".into(),
            kind: FixtureKind::MovingHead,
            channels: vec![
                ch("Pan", Attribute::Pan, ChannelByte::Coarse),
                ch("Pan fine", Attribute::Pan, ChannelByte::Fine),
                ch("Tilt", Attribute::Tilt, ChannelByte::Coarse),
                ch("Tilt fine", Attribute::Tilt, ChannelByte::Fine),
                ch("Dimmer", Attribute::Dimmer, ChannelByte::Coarse),
            ],
            modes: vec![],
            physical: Physical::default(),
        };
        let mode = ModeDef {
            name: "m".into(),
            channels: vec![0, 2, 1, 3, 4],
            heads: vec![],
            physical: None,
        };
        let p = FixtureProfile::build(&def, &mode);
        assert_eq!(p.slots.len(), 3);
        assert_eq!(p.slots[0].offset, 0);
        assert_eq!(p.slots[0].fine, Some(2));
        assert_eq!(p.slots[1].fine, Some(3));
        assert_eq!(p.heads.len(), 1);

        // Two heads of RGB plus a fixture-wide dimmer.
        let def2 = FixtureDefinition {
            channels: vec![
                ch("R", Attribute::Red, ChannelByte::Coarse),
                ch("G", Attribute::Green, ChannelByte::Coarse),
                ch("D", Attribute::Dimmer, ChannelByte::Coarse),
            ],
            ..def
        };
        let mode2 = ModeDef {
            name: "m2".into(),
            channels: vec![2, 0, 1, 0, 1],
            heads: vec![
                HeadDef {
                    channels: vec![1, 2],
                },
                HeadDef {
                    channels: vec![3, 4],
                },
            ],
            physical: None,
        };
        let p2 = FixtureProfile::build(&def2, &mode2);
        assert_eq!(p2.heads.len(), 2);
        assert_eq!(p2.find(1, Attribute::Red).unwrap().1.offset, 3);
        assert_eq!(p2.find(1, Attribute::Dimmer).unwrap().1.offset, 0);
    }
}
