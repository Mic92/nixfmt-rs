#![no_main]
//! Formatting must be idempotent: `format(format(x)) == format(x)`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(f1) = nixfmt_rs::format(src) else {
        return; // invalid input is fine
    };

    // Directive regions cutting through string interpolations follow upstream
    // semantics we have not ported yet (test/diff/directive_interp).
    if src.contains("nixfmt:") && src.contains("${") {
        return;
    }

    let f2 = match nixfmt_rs::format(&f1) {
        Ok(s) => s,
        Err(e) => panic!(
            "format output failed to re-parse: {e:?}\n--- input ---\n{src}\n--- f1 ---\n{f1}"
        ),
    };

    if f1 != f2 {
        panic!("format not idempotent\n--- input ---\n{src}\n--- f1 ---\n{f1}\n--- f2 ---\n{f2}");
    }
});
