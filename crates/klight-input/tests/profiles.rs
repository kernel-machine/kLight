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
