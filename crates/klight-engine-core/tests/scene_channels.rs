//! Channels of a scene set by hand, on top of intentions.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::CompiledShow;
use klight_engine_core::preview::scene_channels;
use klight_engine_core::scene_edit::{add_scene_values, set_scene_channel};
use klight_model::inspect::ChannelOrigin;
use klight_model::*;

fn pan(fine: bool) -> ChannelRef {
    ChannelRef::Dmx {
        attribute: Attribute::Pan,
        index: 0,
        fine,
    }
}

fn red() -> ChannelRef {
    ChannelRef::Dmx {
        attribute: Attribute::Red,
        index: 0,
        fine: false,
    }
}

fn manual(id: FixtureId, channel: ChannelRef, value: u8) -> SceneValue {
    SceneValue {
        target: fixture(id),
        value: ValueSource::Channel { channel, value },
    }
}

fn position(pan: f32) -> AttributeValue {
    AttributeValue::Position { pan, tilt: 0.0 }
}

/// Runs a scene with `values` and returns the engine.
fn run_scene(values: Vec<SceneValue>) -> klight_engine_core::Engine {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, values, 0),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(10);
    e
}

fn channel_of(values: Vec<SceneValue>, id: FixtureId, channel: ChannelRef) -> (u8, ChannelOrigin) {
    let mut show = test_show();
    show.functions.push(scene(1, values, 0));
    let compiled = CompiledShow::build(&show);
    let fixtures = scene_channels(&show, &compiled, FunctionId(1)).unwrap();
    let c = fixtures
        .iter()
        .find(|f| f.fixture == id)
        .unwrap()
        .channels
        .iter()
        .find(|c| c.channel == channel)
        .unwrap();
    (c.value, c.origin)
}

#[test]
fn a_channel_set_by_hand_overrides_one_byte_of_an_intention() {
    let base = explicit(fixture(SPOT1), position(30.0));
    let [hi, lo] = klight_fixtures::convert::degrees_to_dmx16(30.0, 540.0).to_be_bytes();
    assert_ne!(lo, 7);
    let values = vec![base, manual(SPOT1, pan(true), 7)];

    let e = run_scene(values.clone());
    assert_eq!((dmx(&e, 40), dmx(&e, 41)), (hi, 7));
    assert_eq!(
        channel_of(values.clone(), SPOT1, pan(false)),
        (hi, ChannelOrigin::Value)
    );
    assert_eq!(
        channel_of(values, SPOT1, pan(true)),
        (7, ChannelOrigin::Manual)
    );
}

#[test]
fn a_byte_set_by_hand_alone_takes_the_whole_16_bit_channel() {
    // Another scene drives both bytes of the pan.
    let other = vec![explicit(fixture(SPOT1), position(30.0))];
    let [_, other_fine] = klight_fixtures::convert::degrees_to_dmx16(30.0, 540.0).to_be_bytes();
    let values = vec![manual(SPOT1, pan(false), 200)];
    let mut e = run_scene(other);
    ok(
        &mut e,
        20,
        Command::SetFunction {
            function: scene(2, values.clone(), 0),
        },
    );
    ok(&mut e, 20, Command::StartFunction { id: FunctionId(2) });
    e.tick(30);
    let default_fine = channel_of(values.clone(), SPOT1, pan(true)).0;
    assert_ne!(other_fine, default_fine);
    // Channels merge as a whole (LTP): the newer scene takes both bytes...
    assert_eq!((dmx(&e, 40), dmx(&e, 41)), (200, default_fine));
    // ...so the editor must not report the other one as left alone.
    assert_eq!(
        channel_of(values.clone(), SPOT1, pan(false)),
        (200, ChannelOrigin::Manual)
    );
    assert_eq!(
        channel_of(values, SPOT1, pan(true)),
        (default_fine, ChannelOrigin::Paired)
    );
}

#[test]
fn releasing_one_of_two_bytes_set_by_hand_leaves_it_paired() {
    let both = vec![manual(SPOT1, pan(false), 200), manual(SPOT1, pan(true), 7)];
    let released = set_scene_channel(&both, SPOT1, pan(true), None);
    assert_eq!(
        channel_of(released, SPOT1, pan(true)).1,
        ChannelOrigin::Paired
    );
}

#[test]
fn the_virtual_dimmer_can_be_set_by_hand() {
    let dimmer = ChannelRef::VirtualDimmer { head: 0 };
    let values = vec![
        explicit(fixture(RGB), AttributeValue::Color { color: Color::RED }),
        manual(RGB, dimmer, 51),
    ];
    let e = run_scene(values.clone());
    assert_eq!(dmx(&e, 10), 51, "red scaled by the virtual dimmer");
    assert_eq!(channel_of(values, RGB, dimmer), (51, ChannelOrigin::Manual));
}

#[test]
fn setting_and_releasing_a_channel() {
    let base = vec![explicit(
        fixture(RGB),
        AttributeValue::Color { color: Color::RED },
    )];
    let once = set_scene_channel(&base, RGB, red(), Some(10));
    let twice = set_scene_channel(&once, RGB, red(), Some(20));
    assert_eq!(twice, vec![base[0].clone(), manual(RGB, red(), 20)]);
    assert_eq!(set_scene_channel(&twice, RGB, red(), None), base);
}

#[test]
fn a_new_value_releases_the_channels_it_writes() {
    let show = test_show();
    let compiled = CompiledShow::build(&show);
    let values = vec![
        explicit(group(G_SPOTS), position(0.0)),
        manual(SPOT1, pan(true), 7),
        manual(SPOT2, pan(false), 9),
        manual(RGB, red(), 100),
    ];
    // Replacing the position of one spot releases only its pan.
    let one = explicit(fixture(SPOT1), position(10.0));
    let after = add_scene_values(&show, &compiled, &values, std::slice::from_ref(&one));
    assert_eq!(
        after,
        vec![
            values[0].clone(),
            values[2].clone(),
            values[3].clone(),
            one.clone()
        ]
    );
    // Same slot (a fan replaces a position on the same target): replaced.
    let fan = explicit(
        fixture(SPOT1),
        AttributeValue::PositionFan {
            pan: 0.0,
            tilt: 0.0,
            pan_spread: 0.0,
            tilt_spread: 0.0,
            mode: FanMode::Linear,
        },
    );
    let again = add_scene_values(&show, &compiled, &after, std::slice::from_ref(&fan));
    assert_eq!(again.last(), Some(&fan));
    assert!(!again.contains(&one));
}

#[test]
fn channel_values_are_validated() {
    let mut e = engine(test_show());
    let on_group = SceneValue {
        target: group(G_SPOTS),
        value: ValueSource::Channel {
            channel: pan(false),
            value: 1,
        },
    };
    let service = manual(
        SPOT1,
        ChannelRef::Dmx {
            attribute: Attribute::Maintenance,
            index: 0,
            fine: false,
        },
        255,
    );
    for bad in [on_group, service] {
        assert!(matches!(
            err(
                &mut e,
                0,
                Command::SetFunction {
                    function: scene(1, vec![bad], 0)
                }
            ),
            CommandError::InvalidValue { .. }
        ));
    }
    // A channel the fixture does not have is ignored with a warning.
    let missing = manual(DIM1, pan(false), 1);
    let outcome = ok(
        &mut e,
        0,
        Command::SetFunction {
            function: scene(1, vec![missing], 0),
        },
    );
    assert!(outcome.warnings.iter().any(|w| w.contains("no channel")));
}
