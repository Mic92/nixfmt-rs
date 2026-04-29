#![no_main]
//! Formatting must converge: `format` applied twice must be a fixed point.
//!
//! Ideally `format(format(x)) == format(x)`, but upstream Haskell nixfmt
//! (which this project mirrors) has a few inputs that only stabilise on
//! the second pass (e.g. a trailing line comment immediately after a
//! multi-line string literal). We therefore assert `f² == f³`, which still
//! catches oscillation and unbounded growth while tolerating those known
//! one-step instabilities.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(f1) = nixfmt_rs::format(src) else {
        return; // invalid input is fine
    };

    let f2 = match nixfmt_rs::format(&f1) {
        Ok(s) => s,
        Err(e) => panic!(
            "format output failed to re-parse: {e:?}\n--- input ---\n{src}\n--- f1 ---\n{f1}"
        ),
    };

    if f1 == f2 {
        return;
    }

    let f3 = match nixfmt_rs::format(&f2) {
        Ok(s) => s,
        Err(e) => panic!(
            "format output failed to re-parse: {e:?}\n--- input ---\n{src}\n--- f2 ---\n{f2}"
        ),
    };

    if f2 != f3 {
        panic!(
            "format does not converge\n--- input ---\n{src}\n--- f1 ---\n{f1}\n--- f2 ---\n{f2}\n--- f3 ---\n{f3}"
        );
    }
});
