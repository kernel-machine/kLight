//! Updating a cue from the programmer: same rules as adding values to a scene.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::CompiledShow;
use klight_engine_core::scene_edit::update_cue_values;
use klight_model::*;

#[test]
fn an_update_replaces_the_same_slot_and_keeps_the_rest() {
    let show = test_show();
    let compiled = CompiledShow::build(&show);
    let red = AttributeValue::Color {
        color: Color::new(1.0, 0.0, 0.0),
    };
    let values = vec![
        explicit(fixture(RGB), intensity(0.5)),
        explicit(fixture(RGB), red.clone()),
        explicit(fixture(DIM1), intensity(1.0)),
    ];
    let incoming = vec![
        explicit(fixture(RGB), intensity(0.8)),
        explicit(fixture(DIM2), intensity(0.3)),
    ];
    let out = update_cue_values(&show, &compiled, &values, &incoming);
    assert_eq!(
        out,
        vec![
            explicit(fixture(RGB), red),
            explicit(fixture(DIM1), intensity(1.0)),
            explicit(fixture(RGB), intensity(0.8)),
            explicit(fixture(DIM2), intensity(0.3)),
        ]
    );
}

#[test]
fn an_update_releases_channels_set_by_hand_that_it_writes() {
    let show = test_show();
    let compiled = CompiledShow::build(&show);
    let by_hand = SceneValue {
        target: fixture(DIM1),
        value: ValueSource::Channel {
            channel: ChannelRef::Dmx {
                attribute: Attribute::Dimmer,
                index: 0,
                fine: false,
            },
            value: 12,
        },
    };
    let out = update_cue_values(
        &show,
        &compiled,
        &[by_hand],
        &[explicit(fixture(DIM1), intensity(1.0))],
    );
    assert_eq!(out, vec![explicit(fixture(DIM1), intensity(1.0))]);
}
