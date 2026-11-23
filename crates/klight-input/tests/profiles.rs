#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Every shipped profile loads, and "green widget active" yields exact bytes.

use klight_input::*;
use klight_model::input_map::InputMessage;
use klight_model::Rgb8;

#[test]
fn shipped_profiles_render_green_active() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/controllers");
    let profiles = load_profiles(&dir);
    assert_eq!(
        profiles.len(),
        std::fs::read_dir(&dir).unwrap().count(),
        "a profile failed to load"
    );
    let note = InputMessage::MidiNote {
        channel: 0,
        note: 11,
    };
    let green = Rgb8::new(0, 255, 0);
    let expected: &[(&str, Vec<u8>)] = &[
        ("generic-on-off", vec![0x90, 11, 127]),
        ("akai-apc-mini", vec![0x90, 11, 1]),
        (
            "novation-launchpad-x",
            vec![
                0xF0, 0x00, 0x20, 0x29, 0x02, 0x0C, 0x03, 0x03, 11, 0, 127, 0, 0xF7,
            ],
        ),
    ];
    for (name, bytes) in expected {
        let p = profiles
            .get(*name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            p.render(&note, WidgetState::Active, green, true).as_ref(),
            Some(bytes),
            "{name}"
        );
    }
}

/// The downloaded QLC+ profile of the Akai APC mini mk2: a green pad that is
/// on goes out on MIDI channel 6 (100% brightness) with the green color value.
#[test]
fn downloaded_apc_mini_mk2_profile() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/qlcplus");
    if !dir.join("LICENSE").exists() {
        eprintln!(
            "SKIPPED: profiles/qlcplus not downloaded (run scripts/update-qlcplus-fixtures.sh)"
        );
        return;
    }
    let profiles = load_qxi_profiles(&dir);
    assert!(!profiles.is_empty(), "no QLC+ profile with feedback loaded");
    let p = profiles.get("Akai-APCMini-mk2").unwrap();
    assert_eq!(p.default_active, Some(6));
    assert!(p.behaviors.iter().any(|b| b.label.contains("Pulsing")));
    let note = InputMessage::MidiNote {
        channel: 0,
        note: 11,
    };
    let on = p
        .render(&note, WidgetState::Active, Rgb8::new(0, 255, 0), true)
        .unwrap();
    // 21 is green on the APC mini mk2 (the .qxi lists 42: QLC+ doubles values).
    assert_eq!(on, vec![0x96, 11, 21]);
    let red = p
        .render(&note, WidgetState::Active, Rgb8::new(255, 0, 0), true)
        .unwrap();
    assert_eq!(red, vec![0x96, 11, 5]); // 5 is red on the APC mini mk2
                                        // The Drum button (note 117) has a single-color LED: on is plain value 1.
    let drum = InputMessage::MidiNote {
        channel: 0,
        note: 117,
    };
    assert_eq!(
        p.render(&drum, WidgetState::Active, Rgb8::new(0, 255, 0), true),
        Some(vec![0x90, 117, 1])
    );
    assert_eq!(
        p.render(&drum, WidgetState::Inactive, Rgb8::new(0, 255, 0), true),
        Some(vec![0x90, 117, 0])
    );
    let off = p
        .render(&note, WidgetState::Inactive, Rgb8::new(0, 255, 0), true)
        .unwrap();
    assert_eq!(off, vec![0x90, 11, 0]);
}
