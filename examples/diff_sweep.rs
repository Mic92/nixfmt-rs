//! Differential sweep against the reference Haskell `nixfmt`.
//!
//! Replaces `scripts/diff_sweep.sh` and `scripts/diff_sweep_full.sh`: walks a
//! tree of `*.nix` files, runs both implementations, and records mismatches.
//! Our side runs in-process via the library, so the only fork cost is the
//! reference binary.
//!
//!     cargo run --release --example diff_sweep -- [format|ir|ast] [DIR]
//!
//! Env knobs (all optional):
//!   NIXPKGS      root to scan when DIR is omitted (default: ~/git/nixpkgs)
//!   LIMIT        cap file count (0 = all, default: 2000)
//!   `MAX_BYTES`    skip files larger than N bytes (0 = no cap)
//!   JOBS         rayon threads (default: num CPUs)
//!   REF          reference binary (default: nixfmt)
//!   `REF_TIMEOUT`  per-file timeout for the reference, seconds (default: 8)
//!   OUT          output dir (default: ./sweep-out)

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Format,
    Ir,
    Ast,
}

impl Mode {
    const fn name(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Ir => "ir",
            Self::Ast => "ast",
        }
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Run the reference formatter. Returns `None` if it failed to parse the
/// input (or timed out / wasn't found), so the file is skipped rather than
/// counted against us.
fn run_ref(reference: &str, timeout: u64, mode: Mode, path: &Path, source: &str) -> Option<String> {
    // The Haskell `nixfmt` writes `--ast`/`--ir` dumps to stderr and always
    // exits 1 in those modes; for plain formatting it reads stdin (`-`) and
    // writes stdout.
    let mut cmd = Command::new("timeout");
    cmd.arg(timeout.to_string()).arg(reference);
    let feed_stdin = match mode {
        Mode::Format => {
            cmd.arg("-");
            true
        }
        Mode::Ir | Mode::Ast => {
            cmd.arg(format!("--{}", mode.name())).arg(path);
            false
        }
    };
    cmd.stdin(if feed_stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    if feed_stdin {
        let mut stdin = child.stdin.take().unwrap();
        // Ignore EPIPE: nixfmt may exit early on a parse error.
        let _ = stdin.write_all(source.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    match mode {
        Mode::Format => {
            if out.status.success() {
                String::from_utf8(out.stdout).ok()
            } else {
                None
            }
        }
        Mode::Ir | Mode::Ast => {
            // Exit code is always 1 here; 124 means `timeout` fired.
            if out.status.code() == Some(124) {
                return None;
            }
            let dump = String::from_utf8(out.stderr).ok()?;
            // A real parse error from nixfmt mentions the file path on the
            // first line; a successful dump starts with `Whole`/`Ann`/etc.
            let first = dump.lines().next().unwrap_or("");
            if dump.is_empty() || first.contains(&*path.to_string_lossy()) {
                None
            } else {
                Some(dump)
            }
        }
    }
}

fn run_ours(mode: Mode, source: &str) -> Result<String, String> {
    let r = std::panic::catch_unwind(|| match mode {
        Mode::Format => nixfmt_rs::format(source),
        Mode::Ir => nixfmt_rs::format_ir(source),
        Mode::Ast => nixfmt_rs::format_ast(source),
    });
    match r {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(format!("{e:?}")),
        Err(_) => Err("panic".into()),
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let mode = match args.next().as_deref() {
        None | Some("format") => Mode::Format,
        Some("ir") => Mode::Ir,
        Some("ast") => Mode::Ast,
        Some(m) => {
            eprintln!("unknown mode {m:?}; use format|ir|ast");
            std::process::exit(2);
        }
    };
    let root: PathBuf = args
        .next()
        .or_else(|| env::var("NIXPKGS").ok())
        .unwrap_or_else(|| format!("{}/git/nixpkgs", env::var("HOME").unwrap()))
        .into();
    let limit = env_usize("LIMIT", 2000);
    let max_bytes = env_usize("MAX_BYTES", 0) as u64;
    let reference = env::var("REF").unwrap_or_else(|_| "nixfmt".into());
    let ref_timeout: u64 = env::var("REF_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let out_dir: PathBuf = env::var("OUT")
        .unwrap_or_else(|_| "sweep-out".into())
        .into();
    if let Ok(jobs) = env::var("JOBS")
        && let Ok(n) = jobs.parse()
    {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let mut files: Vec<PathBuf> = WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "nix"))
        .filter(|e| max_bytes == 0 || e.metadata().is_ok_and(|m| m.len() <= max_bytes))
        .map(walkdir::DirEntry::into_path)
        .collect();
    files.sort();
    if limit > 0 {
        files.truncate(limit);
    }
    let total = files.len();
    eprintln!(
        "sweeping {total} files mode={} root={}",
        mode.name(),
        root.display()
    );

    let done = AtomicUsize::new(0);
    let mut mismatches: Vec<String> = files
        .par_iter()
        .filter_map(|f| {
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(500) {
                eprintln!("  {n}/{total}");
            }
            let source = fs::read_to_string(f).ok()?;
            let theirs = run_ref(&reference, ref_timeout, mode, f, &source)?;
            match run_ours(mode, &source) {
                Err(_) => Some(format!("REJECT {}", f.display())),
                Ok(ours) if ours == theirs => None,
                Ok(_) => Some(format!("DIFF {}", f.display())),
            }
        })
        .collect();
    mismatches.sort();

    fs::create_dir_all(&out_dir).unwrap();
    let out_file = out_dir.join(format!("mismatch-{}.txt", mode.name()));
    fs::write(
        &out_file,
        mismatches.join("\n") + if mismatches.is_empty() { "" } else { "\n" },
    )
    .unwrap();

    let diff = mismatches.iter().filter(|l| l.starts_with("DIFF ")).count();
    let reject = mismatches.len() - diff;
    eprintln!(
        "mismatches ({}): {} -> {}",
        mode.name(),
        mismatches.len(),
        out_file.display()
    );
    eprintln!("  DIFF    {diff}");
    eprintln!("  REJECT  {reject}");
    std::process::exit(i32::from(!mismatches.is_empty()));
}
