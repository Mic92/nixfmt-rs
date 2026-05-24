#![no_main]
//! Parse-compatibility oracle: does our parser accept/reject the same inputs
//! as `nix-instantiate --parse`?
//!
//! Any input where the two disagree is a bug (in either direction):
//! - We reject what Nix accepts → missing syntax support.
//! - We accept what Nix rejects → overly permissive grammar.
//!
//! The target shells out to `nix-instantiate`, so it is much slower than the
//! pure-Rust fuzzers. Run it with a generous timeout:
//!
//! ```sh
//! cargo fuzz run -s none fuzz_nix_compat -- -max_total_time=600 -timeout=30
//! ```

use libfuzzer_sys::fuzz_target;
use std::io::Write;
use std::process::Command;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    // Skip degenerate inputs that would waste time or confuse the shell.
    if src.len() > 4096 || src.contains('\0') {
        return;
    }

    let we_parse = nixfmt_rs::parse(src).is_ok();

    // Write to a temp file to avoid argument parsing issues (e.g. inputs
    // starting with `-` being interpreted as flags).
    let mut tmp = tempfile::NamedTempFile::new().expect("create tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    tmp.flush().expect("flush tempfile");

    let nix = Command::new("nix-instantiate")
        .args(["--parse", tmp.path().to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();

    let Ok(output) = nix else {
        return; // nix-instantiate not available, skip
    };
    let nix_parses = output.status.success();

    // `nix-instantiate --parse` also resolves variables, so "undefined
    // variable" errors are not parse failures. Same for "infinite
    // recursion" and other eval-time checks that --parse still runs.
    if !nix_parses {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("undefined variable")
            || stderr.contains("infinite recursion")
            || stderr.contains("attribute")
        {
            // Not a parse error; nix-instantiate evaluates more than it
            // should in --parse mode.  Skip this input.
            return;
        }
    }

    if we_parse && !nix_parses {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Known issues we haven't fixed yet — skip to find new bugs.
        // TODO: `or` in expression position (e.g. `a.b or c or d`)
        if stderr.contains("unexpected 'or'") {
            return;
        }
        // TODO: Nix lexes `ident.ident/path` as a path; we see attrpath + division
        if stderr.contains("end of path") {
            return;
        }

        panic!(
            "we accept but nix-instantiate rejects\n--- input ({} bytes) ---\n{src}\n--- stderr ---\n{stderr}",
            src.len()
        );
    }

    if !we_parse && nix_parses {
        panic!(
            "we reject but nix-instantiate accepts\n--- input ({} bytes) ---\n{src}",
            src.len()
        );
    }
});
