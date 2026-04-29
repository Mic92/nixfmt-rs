#![no_main]
//! parse → format → parse round-trip.
//!
//! For any input the parser accepts, formatting it must produce output
//! that (a) parses again and (b) yields the same AST modulo trivia/spans.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(ast1) = nixfmt_rs::parse_normalized(src) else {
        return; // invalid input is fine
    };

    let formatted = nixfmt_rs::format(src).expect("format must succeed when parse succeeds");

    let ast2 = match nixfmt_rs::parse_normalized(&formatted) {
        Ok(a) => a,
        Err(e) => panic!(
            "re-parse of formatted output failed: {e:?}\n--- input ---\n{src}\n--- formatted ---\n{formatted}"
        ),
    };

    if ast1 != ast2 {
        panic!(
            "AST changed after format\n--- input ---\n{src}\n--- formatted ---\n{formatted}\n--- ast1 ---\n{ast1:#?}\n--- ast2 ---\n{ast2:#?}"
        );
    }
});
