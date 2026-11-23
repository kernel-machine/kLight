#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::{Merge, Role, NO_ADDR, NO_CHANNEL};
use klight_engine_core::engine::merge;
use klight_engine_core::music::{MusicClock, BEAT};
use klight_engine_core::playback::{lerp_u16, Layer};
use klight_fixtures::convert::{degrees_to_dmx16, dmx16_to_degrees, unit_to16};
use klight_model::*;
use proptest::prelude::*;

fn layer_strategy(n: usize) -> impl Strategy<Value = Layer> {
    (
        prop::collection::vec(any::<u16>(), n),
        prop::collection::vec(any::<bool>(), n),
    )
        .prop_map(|(values, set)| Layer { values, set })
}

fn channel_count() -> usize {
    channels().len()
}

fn channels() -> Vec<klight_engine_core::compile::LogicalChannel> {
    engine(test_show()).compiled().channels.clone()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn htp_never_below_any_input(
        a in layer_strategy(channel_count()),
        b in layer_strategy(channel_count()),
        c in layer_strategy(channel_count()),
    ) {
        let chans = channels();
        let n = chans.len();
        let mut merged = vec![0u16; n];
        let mut frames = vec![[0u8; 512]];
        merge(&chans, [&a, &b, &c], u16::MAX, false, &mut merged, &mut frames);
        for (ch, lc) in chans.iter().enumerate() {
            if lc.merge == Merge::Htp {
                for l in [&a, &b, &c] {
                    if let Some(v) = l.get(ch) {
                        prop_assert!(merged[ch] >= v);
                    }
                }
            }
            if lc.merge == Merge::Ltp {
                // Topmost set layer wins.
                let expect = [&c, &b, &a].iter().find_map(|l| l.get(ch)).unwrap_or(lc.default);
                prop_assert_eq!(merged[ch], expect);
            }
        }
    }

    #[test]
    fn blackout_and_grand_master_zero_intensity(
        gm in any::<u16>(),
        values in prop::collection::vec(any::<u16>(), 64),
    ) {
        let chans = channels();
        let n = chans.len();
        let full = Layer { values: (0..n).map(|i| values[i % values.len()]).collect(), set: vec![true; n] };
        let empty = Layer { values: vec![0; n], set: vec![false; n] };
        let mut merged = vec![0u16; n];
        let mut f_black = vec![[0u8; 512]];
        merge(&chans, [&full, &empty, &empty], gm, true, &mut merged, &mut f_black);
        let mut f_gm0 = vec![[0u8; 512]];
        merge(&chans, [&full, &empty, &empty], 0, false, &mut merged, &mut f_gm0);
        let mut f_gm1 = vec![[0u8; 512]];
        merge(&chans, [&full, &empty, &empty], u16::MAX, false, &mut merged, &mut f_gm1);
        for lc in &chans {
            if lc.coarse == NO_ADDR { continue; }
            let a = usize::from(lc.coarse);
            let lit = lc.role == Role::Dimmer || matches!(lc.role, Role::Color { vdim } if vdim != NO_CHANNEL);
            if lit {
                prop_assert_eq!(f_black[0][a], 0);
                prop_assert_eq!(f_gm0[0][a], 0);
            }
            if matches!(lc.attribute, Attribute::Pan | Attribute::Tilt) {
                prop_assert_eq!(f_gm0[0][a], f_gm1[0][a], "GM must not touch pan/tilt");
            }
        }
    }

    #[test]
    fn fade_is_bounded_monotonic_and_exact(from in any::<u16>(), to in any::<u16>(), den in 1u64..10_000_000) {
        let mut prev = from;
        let steps = 50u64;
        for k in 0..=steps {
            let num = den * k / steps;
            let v = lerp_u16(from, to, num, den);
            prop_assert!(v >= from.min(to) && v <= from.max(to));
            if to >= from { prop_assert!(v >= prev) } else { prop_assert!(v <= prev) }
            prev = v;
        }
        prop_assert_eq!(lerp_u16(from, to, den, den), to);
        prop_assert_eq!(lerp_u16(from, to, den + 1, den), to);
        prop_assert_eq!(lerp_u16(from, to, 0, den), from);
    }

    #[test]
    fn degrees_roundtrip_within_resolution(deg in -270.0f32..270.0, range in prop::sample::select(vec![180.0f32, 270.0, 360.0, 540.0, 630.0])) {
        let deg = deg.clamp(-range / 2.0, range / 2.0);
        let v = degrees_to_dmx16(deg, range);
        let back = dmx16_to_degrees(v, range);
        // 16-bit resolution (plus float slack)
        prop_assert!((back - deg).abs() <= range / 65535.0 + 1e-3, "{deg} -> {v} -> {back}");
        // 8-bit coarse only
        let coarse = (v >> 8) * 257;
        let back8 = dmx16_to_degrees(coarse, range);
        prop_assert!((back8 - deg).abs() <= range / 255.0 + 1e-3);
    }

    #[test]
    fn rgb_color_roundtrip(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        // A color reachable by an 8-bit RGB fixture comes back identical.
        let c = Color::new(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
        let mut e = engine(test_show());
        ok(&mut e, 0, Command::ProgrammerSet { target: fixture(RGB), value: AttributeValue::Color { color: c } });
        e.tick(0);
        prop_assert_eq!((dmx(&e, 10), dmx(&e, 11), dmx(&e, 12)), (r, g, b));
        prop_assert_eq!(unit_to16(f32::from(r) / 255.0) >> 8, u16::from(r));
    }

    #[test]
    fn music_clock_monotonic_and_continuous(
        bpms in prop::collection::vec(30.0f64..300.0, 1..10),
        gaps in prop::collection::vec(1u64..5_000_000, 1..10),
    ) {
        let mut c = MusicClock::new(120.0);
        let mut t = 0u64;
        let mut last = 0u64;
        for (bpm, gap) in bpms.iter().zip(gaps.iter()) {
            t += gap;
            let before = c.beat_at(t);
            prop_assert!(before >= last);
            c.set_manual_bpm(t, *bpm);
            prop_assert_eq!(c.beat_at(t), before, "no phase jump on BPM change");
            last = before;
        }
        // One beat later the position has advanced by one beat (within 1 us).
        let one = c.beats_to_us(BEAT);
        let b0 = c.beat_at(t);
        let b1 = c.beat_at(t + one);
        prop_assert!(b1.abs_diff(b0 + BEAT) < BEAT / 10_000);
    }

    #[test]
    fn patch_conflicts_match_brute_force(addrs in prop::collection::vec(1u16..=500, 1..12)) {
        let mut show = test_show();
        let template = show.fixtures[3].clone(); // 6-channel par
        show.fixtures.clear();
        for (i, a) in addrs.iter().enumerate() {
            let mut f = template.clone();
            f.id = FixtureId(i as u32 + 1);
            f.address = *a;
            show.fixtures.push(f);
        }
        let conflicts = klight_model::validate::address_conflicts(&show);
        let mut brute = 0;
        for i in 0..addrs.len() {
            for j in i + 1..addrs.len() {
                if addrs[i].abs_diff(addrs[j]) < 6 { brute += 1; }
            }
        }
        prop_assert_eq!(conflicts.len(), brute);
    }
}

#[test]
fn group_fan_symmetric_from_center() {
    // A fan from the center: heads on opposite sides get opposite offsets.
    let mut show = test_show();
    show.fixtures[5].orientation.invert_pan = false;
    let mut e = engine(show);
    for (i, f) in [SPOT1, SPOT2].iter().enumerate() {
        let pan = if i == 0 { -30.0 } else { 30.0 };
        ok(
            &mut e,
            0,
            Command::ProgrammerSet {
                target: fixture(*f),
                value: AttributeValue::Position { pan, tilt: 0.0 },
            },
        );
    }
    e.tick(0);
    let a = u16::from_be_bytes([dmx(&e, 40), dmx(&e, 41)]);
    let b = u16::from_be_bytes([dmx(&e, 60), dmx(&e, 61)]);
    assert!((i32::from(a) + i32::from(b) - 65536).abs() <= 2, "{a} {b}");
}
