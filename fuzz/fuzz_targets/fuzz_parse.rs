#![no_main]
//! Feed arbitrary bytes to the parser. It must never panic, hang or OOM;
//! returning `Err` is fine. Any crash here is a bug.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = nixfmt_rs::parse(s);
    }
});
