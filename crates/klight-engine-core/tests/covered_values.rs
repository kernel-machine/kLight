//! Values of a scene that later values override: the last value wins on every
//! channel, whatever its target.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::CompiledShow;
use klight_engine_core::scene_edit::{covered_values, remove_covered_values};
use klight_model::inspect::CoveredValue;
use klight_model::*;

const WHOLE: Target = Target::Scene;

fn color(target: Target, color: Color) -> SceneValue {
    explicit(target, AttributeValue::Color { color })
}

fn position(target: Target, pan: f32) -> SceneValue {
    explicit(target, AttributeValue::Position { pan, tilt: 0.0 })
}

fn pan_by_hand(id: FixtureId, fine: bool, value: u8) -> SceneValue {
    SceneValue {
        target: fixture(id),
        value: ValueSource::Channel {
            channel: ChannelRef::Dmx {
                attribute: Attribute::Pan,
                index: 0,
                fine,
            },
            value,
        },
    }
}

fn scene_of(members: Vec<Target>, values: Vec<SceneValue>) -> Scene {
    Scene {
        members,
        values,
        ..Scene::default()
    }
}

fn covered(scene: &Scene) -> Vec<CoveredValue> {
    let show = test_show();
    covered_values(&show, &CompiledShow::build(&show), scene)
}

fn cover(index: u32, by: &[u32], full: bool) -> CoveredValue {
    CoveredValue {
        index,
        by: by.to_vec(),
        full,
    }
}

/// DMX output of a running scene, channels 1..=120.
fn output(scene: Scene) -> Vec<u8> {
    let mut e = engine(test_show());
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: Function {
                id: FunctionId(1),
                folder: None,
                name: "s".into(),
                kind: FunctionKind::Scene(scene),
            },
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(1_000);
    (1..=120).map(|a| dmx(&e, a)).collect()
}

#[test]
fn a_later_value_for_all_lights_covers_a_fixture_value() {
    let lights = vec![fixture(RGB), fixture(RGBW)];
    let s = scene_of(
        lights,
        vec![color(fixture(RGB), Color::RED), color(WHOLE, Color::BLUE)],
    );
    assert_eq!(covered(&s), vec![cover(0, &[1], true)]);
}

#[test]
fn a_later_fixture_value_overrides_a_group_value_on_that_fixture_only() {
    let s = scene_of(
        vec![group(G_SPOTS)],
        vec![
            position(group(G_SPOTS), 10.0),
            position(fixture(SPOT1), 40.0),
        ],
    );
    assert_eq!(
        covered(&s),
        vec![cover(0, &[1], false)],
        "spot 2 keeps the group value"
    );
}

#[test]
fn a_later_group_value_covers_a_fixture_value_of_the_group() {
    let s = scene_of(
        vec![group(G_SPOTS)],
        vec![
            position(fixture(SPOT1), 40.0),
            position(group(G_SPOTS), 10.0),
        ],
    );
    assert_eq!(covered(&s), vec![cover(0, &[1], true)]);
}

#[test]
fn other_attributes_do_not_cover() {
    let s = scene_of(
        vec![fixture(RGB)],
        vec![
            color(fixture(RGB), Color::RED),
            explicit(WHOLE, intensity(1.0)),
        ],
    );
    assert!(covered(&s).is_empty());
}

#[test]
fn one_byte_set_by_hand_covers_a_16_bit_channel_only_in_part() {
    let s = scene_of(
        vec![fixture(SPOT1)],
        vec![position(fixture(SPOT1), 10.0), pan_by_hand(SPOT1, false, 7)],
    );
    assert_eq!(
        covered(&s),
        vec![cover(0, &[1], false)],
        "tilt and the fine pan byte stay"
    );
    let both = scene_of(
        vec![fixture(SPOT1)],
        vec![
            pan_by_hand(SPOT1, false, 7),
            pan_by_hand(SPOT1, true, 9),
            explicit(
                fixture(SPOT1),
                AttributeValue::Position {
                    pan: 0.0,
                    tilt: 0.0,
                },
            ),
        ],
    );
    assert_eq!(
        covered(&both),
        vec![cover(0, &[2], true), cover(1, &[2], true)]
    );
}

#[test]
fn every_value_that_overrides_is_listed_in_order() {
    let lights = vec![fixture(RGB), fixture(RGBW)];
    let s = scene_of(
        lights,
        vec![
            color(fixture(RGB), Color::RED),
            color(fixture(RGB), Color::GREEN),
            color(WHOLE, Color::BLUE),
        ],
    );
    assert_eq!(
        covered(&s),
        vec![cover(0, &[1, 2], true), cover(1, &[2], true)]
    );
}

#[test]
fn values_that_write_nothing_are_not_listed() {
    let s = scene_of(
        vec![fixture(DIM1)],
        vec![color(fixture(DIM1), Color::RED), color(WHOLE, Color::BLUE)],
    );
    assert!(covered(&s).is_empty(), "a dimmer has no color");
}

#[test]
fn removing_covered_values_keeps_the_output_identical() {
    let lights = vec![group(G_SPOTS), fixture(RGB), fixture(RGBW)];
    let s = scene_of(
        lights,
        vec![
            color(fixture(RGB), Color::RED),
            position(fixture(SPOT1), 40.0),
            position(group(G_SPOTS), 10.0),
            color(WHOLE, Color::BLUE),
            position(fixture(SPOT2), -20.0),
            explicit(WHOLE, intensity(1.0)),
        ],
    );
    let show = test_show();
    let cleaned = remove_covered_values(&show, &CompiledShow::build(&show), &s);
    assert_eq!(
        cleaned.values,
        vec![
            position(group(G_SPOTS), 10.0),
            color(WHOLE, Color::BLUE),
            position(fixture(SPOT2), -20.0),
            explicit(WHOLE, intensity(1.0)),
        ],
        "the partly covered group position stays"
    );
    assert_eq!(output(cleaned), output(s));
}
