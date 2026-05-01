#![no_main]
//! Feed arbitrary bytes to the parser. It must never panic, hang or OOM;
//! returning `Err` is fine. Any crash here is a bug.
//!
//! On `Err` the diagnostic is also rendered via `format_error`, so the
//! error-formatting / source-context code (which the other targets never
//! reach) is fuzzed against arbitrary spans and source shapes too.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Err(e) = nixfmt_rs::parse(s) {
        let _ = nixfmt_rs::format_error(s, Some("<fuzz>"), &e);
        let _ = nixfmt_rs::format_error(s, None, &e);
        let _ = e.to_string();
    }
});
