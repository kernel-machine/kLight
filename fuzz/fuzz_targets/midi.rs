#![no_main]
// MIDI (including SysEx) as received from controllers.
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = klight_input::midi::parse(data);
});
