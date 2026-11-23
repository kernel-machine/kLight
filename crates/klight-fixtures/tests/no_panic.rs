//! Stable-toolchain complement to `fuzz/`: hostile input never panics.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use proptest::prelude::*;

const VALID: &str = include_str!("../../../fixtures/Test-Spot-Moving-Head.qxf");

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn arbitrary_text(s in ".{0,400}") {
        let _ = klight_fixtures::parse_qxf(&s);
    }

    #[test]
    fn mutated_valid_file(cut in 0usize..VALID.len(), insert in "[<>/=\"a-zA-Z0-9 #&;]{0,12}", drop in 0usize..40) {
        let mut s = String::from(VALID);
        let cut = (0..=cut).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
        let end = (cut + drop).min(s.len());
        let end = (end..=s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
        s.replace_range(cut..end, &insert);
        let _ = klight_fixtures::parse_qxf(&s);
    }
}

#[test]
fn every_test_fixture_parses() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "qxf") {
            klight_fixtures::parse_qxf(&std::fs::read_to_string(&p).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        }
    }
}

#[test]
fn deep_nesting_is_rejected() {
    let s = "<a>".repeat(10_000);
    assert!(klight_fixtures::parse_qxf(&s).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn arbitrary_qxi_text(s in ".{0,400}") {
        let _ = klight_fixtures::parse_qxi(&s);
    }

    #[test]
    fn mangled_qxi(
        n in "[0-9a-z-]{0,6}",
        rgb in "[#0-9a-zA-Z]{0,9}",
        v in "[0-9a-z-]{0,5}",
    ) {
        let s = format!(
            "<InputProfile><Channel Number=\"{n}\"><Feedback LowerValue=\"{v}\" MidiChannel=\"{v}\"/></Channel>\
             <ColorTable><Color Value=\"{v}\" RGB=\"{rgb}\"/></ColorTable>\
             <MidiChannelTable><Channel Value=\"{v}\"/></MidiChannelTable></InputProfile>"
        );
        let _ = klight_fixtures::parse_qxi(&s);
    }
}
