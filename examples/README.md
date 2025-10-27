# Examples

This directory contains example programs demonstrating nixfmt-rs functionality.

## error_visualization.rs

Demonstrates what error messages look like for common parsing errors, comparing current output with future goals.

**Run:**
```bash
cargo run --example error_visualization
```

**Output:** Shows 14 different error cases with:
- Current error output (basic)
- Future error output (with source snippets, pointers, and helpful suggestions)

**Examples include:**
- Missing semicolon
- Unclosed delimiters (braces, parentheses, strings)
- Chained comparison operators
- Unexpected tokens
- Invalid syntax patterns
- Mismatched delimiters
- And more...

**Purpose:** Serves as:
- A visual reference for error message improvements
- A test bed for the new error system
- Documentation of common error scenarios
- Motivation for why better error messages matter

**Note:** This example currently shows mock "future" error output. As the error enhancement plan is implemented, these mock messages will be replaced with actual output from the new error system.

## Adding More Examples

To add a new example:

1. Create a file in `examples/` named `example_name.rs`
2. Add `//!` doc comments at the top explaining what it does
3. Implement a `main()` function
4. Use `nixfmt_rs` APIs to demonstrate functionality
5. Run with `cargo run --example example_name`
