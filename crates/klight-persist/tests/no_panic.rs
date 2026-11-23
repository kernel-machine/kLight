use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn show_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
        let _ = klight_persist::parse_show_bytes(&bytes);
    }

    #[test]
    fn json_like_never_panics(s in r#"[\{\}\[\]":,0-9a-z_ ]{0,200}"#) {
        let _ = klight_persist::parse_show_bytes(s.as_bytes());
    }
}
