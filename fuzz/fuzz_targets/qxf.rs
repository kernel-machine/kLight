#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = klight_fixtures::parse_qxf(s);
    }
});
