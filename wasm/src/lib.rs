//! WebAssembly bindings for `nixfmt-rs`.
//!
//! ```js
//! import init, { format, version } from "nixfmt-wasm";
//! await init();
//! format("{a=1;}");                    // "{ a = 1; }\n"
//! format(src, { width: 80, indent: 4, filename: "foo.nix" });
//! ```

use js_sys::{Array, Error, Reflect};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS: &str = r#"
export interface FormatOptions {
  /** Target line width (soft). Default 100. */
  width?: number;
  /** Spaces per indentation level. Default 2. */
  indent?: number;
  /** Name shown in the diagnostic on parse failure. */
  filename?: string;
}

/** Thrown by {@link format} when `source` is not valid Nix. */
export interface ParseError extends Error {
  /** Multi-line rustc-style diagnostic with source snippet and caret. */
  diagnostic: string;
  /** Byte offsets `[start, end)` of the primary error span in `source`. */
  range: [number, number];
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "FormatOptions")]
    pub type FormatOptions;
    #[wasm_bindgen(method, getter, structural)]
    fn width(this: &FormatOptions) -> Option<u32>;
    #[wasm_bindgen(method, getter, structural)]
    fn indent(this: &FormatOptions) -> Option<u32>;
    #[wasm_bindgen(method, getter, structural)]
    fn filename(this: &FormatOptions) -> Option<String>;
}

/// Format a Nix expression. Throws {@link ParseError} if `source` is invalid.
#[wasm_bindgen(unchecked_return_type = "string")]
pub fn format(source: &str, options: Option<FormatOptions>) -> Result<String, JsValue> {
    let mut opts = nixfmt_rs::Options::default();
    let mut filename = None;
    if let Some(o) = options.as_ref() {
        if let Some(w) = o.width() {
            opts.width = w as usize;
        }
        if let Some(i) = o.indent() {
            opts.indent = i as usize;
        }
        filename = o.filename();
    }

    nixfmt_rs::format_with(source, &opts).map_err(|e| {
        let r = e.byte_range();
        let diagnostic = nixfmt_rs::format_error(source, filename.as_deref(), &e);
        let err = Error::new(&e.message());
        err.set_name("ParseError");
        let obj: &JsValue = err.as_ref();
        let _ = Reflect::set(obj, &"diagnostic".into(), &diagnostic.into());
        let range = Array::of2(&(r.start as u32).into(), &(r.end as u32).into());
        let _ = Reflect::set(obj, &"range".into(), &range);
        err.into()
    })
}

/// Crate version string, e.g. `"0.1.2"`.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    nixfmt_rs::VERSION.into()
}
