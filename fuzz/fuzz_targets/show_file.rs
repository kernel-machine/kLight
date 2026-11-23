#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = klight_persist::parse_show_bytes(data);
});
