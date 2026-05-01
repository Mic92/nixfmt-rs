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
/// Throws a JS exception with the parse error message on invalid Nix.
#[wasm_bindgen]
pub fn format(source: &str) -> Result<String, JsError> {
    nixfmt_rs::format(source).map_err(|e| JsError::new(&e.to_string()))
}

/// Format a Nix expression with explicit `width` (line width) and `indent`
/// (spaces per level).
///
/// # Errors
/// See [`format`].
#[wasm_bindgen]
pub fn format_with(source: &str, width: usize, indent: usize) -> Result<String, JsError> {
    nixfmt_rs::format_with(source, width, indent).map_err(|e| JsError::new(&e.to_string()))
}
