//! WebAssembly bindings for `nixfmt-rs`.
//!
//! Build via `nix build .#wasm`, or manually:
//!
//! ```sh
//! cargo build -p nixfmt-wasm --release --target wasm32-unknown-unknown
//! wasm-bindgen --target web --out-dir pkg \
//!     target/wasm32-unknown-unknown/release/nixfmt_wasm.wasm
//! ```

use wasm_bindgen::prelude::*;

/// Format a Nix expression with default layout (width 100, indent 2).
///
/// # Errors
/// Throws a JS exception whose message is the same multi-line diagnostic
/// the CLI prints (snippet + caret + hint). ANSI escapes are stripped
/// since browser DOM nodes don't render them.
#[wasm_bindgen]
pub fn format(source: &str) -> Result<String, JsError> {
    format_with(source, 100, 2)
}

/// Format a Nix expression with explicit `width` (line width) and `indent`
/// (spaces per level).
///
/// # Errors
/// See [`format`].
#[wasm_bindgen]
pub fn format_with(source: &str, width: usize, indent: usize) -> Result<String, JsError> {
    let mut opts = nixfmt_rs::Options::default();
    opts.width = width;
    opts.indent = indent;
    nixfmt_rs::format_with(source, &opts).map_err(|e| {
        let pretty = nixfmt_rs::format_error(source, None, &e);
        JsError::new(&strip_ansi(&pretty))
    })
}

/// Drop ANSI SGR escape sequences (`\x1b[...m`). The CLI formatter colours
/// its output for terminals; the playground renders plain text in a `<pre>`.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for term in chars.by_ref() {
                if term.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
