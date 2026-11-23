use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    #[test]
    fn midi_and_osc_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
        let _ = klight_input::midi::parse(&bytes);
        let _ = klight_input::osc::decode(&bytes);
    }
}
