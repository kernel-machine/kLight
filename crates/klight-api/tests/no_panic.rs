use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn client_messages_never_panic(s in r#"[\{\}\[\]":,0-9a-z_ .-]{0,300}"#) {
        let _ = klight_api::parse_client_message(&s);
    }
}
