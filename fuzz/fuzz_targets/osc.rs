#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = klight_input::osc::decode(data);
});
