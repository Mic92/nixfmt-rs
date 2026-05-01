#![no_main]
//! Exercise the debug renderers (`--ast` / `--ir`). They share none of the
//! pretty-printer code that `fuzz_roundtrip` covers, so without this target
//! `pretty_simple/*` and `colored_writer` see zero fuzz coverage. The only
//! property asserted is "does not panic"; output is intentionally discarded.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    if nixfmt_rs::parse(src).is_err() {
        return;
    }
    let _ = nixfmt_rs::format_ast(src).expect("format_ast must succeed when parse succeeds");
    let _ = nixfmt_rs::format_ir(src).expect("format_ir must succeed when parse succeeds");
});
