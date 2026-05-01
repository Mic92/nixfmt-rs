//! nixfmt-rs CLI
//!
//! Mirrors the flag surface and exit-code semantics of the Haskell `nixfmt`
//! binary so the two can be used interchangeably by editors / CI.

use std::io::{self, Read, Write};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use std::path::Path;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
nixfmt-rs [OPTIONS] [FILES]
  Format Nix source code (Rust implementation of nixfmt)

Common flags:
  -w --width=INT        Maximum width in characters [default: 100]
     --indent=INT       Number of spaces to use for indentation [default: 2]
  -c --check            Check whether files are formatted without modifying them
  -m --mergetool        Git mergetool mode (not implemented)
  -q --quiet            Do not report errors
  -s --strict           Enable a stricter formatting mode (accepted, currently no-op)
  -v --verify           Apply sanity checks on the output after formatting
  -a --ast              Pretty print the internal AST to stderr (debug)
  -f --filename=ITEM    Filename to display when input is read from stdin
     --ir               Pretty print the internal IR to stderr (debug)
  -? --help             Display help message
  -V --version          Print version information
     --numeric-version  Print just the version number
";

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // flat CLI flag bag
struct Opts {
    width: usize,
    indent: usize,
    check: bool,
    quiet: bool,
    #[allow(dead_code)] // accepted for CLI parity; no strict-mode hook yet
    strict: bool,
    verify: bool,
    ast: bool,
    ir: bool,
    parse_only: bool,
    #[allow(dead_code)] // accepted for CLI parity; mergetool mode unimplemented
    mergetool: bool,
    filename: Option<String>,
    files: Vec<String>,
}

fn parse_args() -> Result<Opts, String> {
    let mut o = Opts {
        width: 100,
        indent: 2,
        ..Opts::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            if let Some(v) = inline.clone() {
                return Ok(v);
            }
            args.next()
                .ok_or_else(|| format!("Missing value for flag: {name}"))
        };
        let mut int = |name: &str| -> Result<usize, String> {
            value(name)?
                .parse()
                .map_err(|_| format!("Invalid integer for {name}"))
        };
        match flag.as_str() {
            "-?" | "--help" => {
                print!("{HELP}");
                exit(0);
            }
            "-V" | "--version" => {
                println!("nixfmt-rs {VERSION}");
                exit(0);
            }
            "--numeric-version" => {
                println!("{VERSION}");
                exit(0);
            }
            "-w" | "--width" => o.width = int("--width")?,
            "--indent" => o.indent = int("--indent")?,
            "-c" | "--check" => o.check = true,
            "-q" | "--quiet" => o.quiet = true,
            "-s" | "--strict" => o.strict = true,
            "-v" | "--verify" => o.verify = true,
            "-a" | "--ast" => o.ast = true,
            "--ir" => o.ir = true,
            "--parse-only" => o.parse_only = true,
            "-m" | "--mergetool" => o.mergetool = true,
            "-f" | "--filename" => o.filename = Some(value("--filename")?),
            "--" => {
                o.files.extend(args.by_ref());
            }
            s if s.starts_with("-w") && !s.starts_with("--") && s.len() > 2 => {
                o.width = s[2..]
                    .parse()
                    .map_err(|_| "Invalid integer for --width".to_string())?;
            }
            s if s.starts_with('-') => return Err(format!("Unknown flag: {s}")),
            _ => o.files.push(arg),
        }
    }
    Ok(o)
}

fn try_format(o: &Opts, name: &str, source: &str) -> Result<String, String> {
    let fmt = |s: &str| {
        let mut opts = nixfmt_rs::Options::default();
        opts.width = o.width;
        opts.indent = o.indent;
        nixfmt_rs::format_with(s, &opts).map_err(|e| nixfmt_rs::format_error(s, Some(name), &e))
    };
    let out = fmt(source)?;
    if o.verify {
        let again = fmt(&out).map_err(|e| format!("{name}: nixfmt verify: reparse failed\n{e}"))?;
        if again != out {
            return Err(format!("{name}: nixfmt verify: output is not idempotent"));
        }
    }
    Ok(out)
}

/// Returns `true` on success so the caller can fold exit status across files.
fn process(o: &Opts, name: &str, source: &str, in_place: bool) -> bool {
    if o.parse_only {
        return match nixfmt_rs::parse(source) {
            Ok(_) => true,
            Err(e) => {
                if !o.quiet {
                    eprintln!("{}", nixfmt_rs::format_error(source, Some(name), &e));
                }
                false
            }
        };
    }
    if o.ast || o.ir {
        // Upstream routes debug dumps to stderr and exits 1 so scripts never
        // mistake them for formatted output.
        let res = if o.ast {
            nixfmt_rs::format_ast(source)
        } else {
            nixfmt_rs::format_ir(source)
        };
        match res {
            Ok(s) => eprint!("{s}"),
            Err(e) if !o.quiet => {
                eprintln!("{}", nixfmt_rs::format_error(source, Some(name), &e));
            }
            Err(_) => {}
        }
        return false;
    }

    let out = match try_format(o, name, source) {
        Ok(s) => s,
        Err(msg) => {
            if !o.quiet {
                eprintln!("{msg}");
            }
            return false;
        }
    };

    if o.check {
        if out != source {
            if !o.quiet {
                eprintln!("{name}: not formatted");
            }
            return false;
        }
        return true;
    }

    if in_place {
        // Skip the write when unchanged to preserve mtimes for build tools.
        if out != source
            && let Err(e) = std::fs::write(name, &out)
        {
            if !o.quiet {
                eprintln!("{name}: {e}");
            }
            return false;
        }
    } else {
        let _ = io::stdout().write_all(out.as_bytes());
    }
    true
}

fn main() {
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };

    let mut ok = true;

    if o.files.is_empty() {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            eprintln!("error: failed to read stdin: {e}");
            exit(1);
        }
        let name = o.filename.as_deref().unwrap_or("<stdin>");
        ok &= process(&o, name, &buf, false);
    } else {
        // Debug dumps stream to stderr; running them in parallel would
        // interleave output, so keep those modes sequential.
        let parallel = !(o.ast || o.ir || o.parse_only);
        ok &= walk_and_process(&o, parallel);
    }

    exit(i32::from(!ok));
}

fn process_path(o: &Opts, path: &Path) -> bool {
    let name = path.to_string_lossy();
    match std::fs::read_to_string(path) {
        Ok(source) => process(o, &name, &source, true),
        Err(e) => {
            if !o.quiet {
                eprintln!("{name}: {e}");
            }
            false
        }
    }
}

/// Walk argument paths with `ignore`'s parallel walker and run `process_path`
/// on every match. Explicit file arguments are passed through even without a
/// `.nix` extension; the filter only applies to entries discovered under a
/// directory argument.
fn walk_and_process(o: &Opts, parallel: bool) -> bool {
    let mut args = o.files.iter();
    let first = args.next().expect("caller checked non-empty");
    let mut wb = ignore::WalkBuilder::new(first);
    for a in args {
        wb.add(a);
    }
    // We are a formatter, not a search tool: walk everything.
    wb.standard_filters(false);

    let want = |e: &ignore::DirEntry| {
        e.file_type().is_some_and(|t| t.is_file())
            && (e.depth() == 0 || e.path().extension().is_some_and(|x| x == "nix"))
    };

    let visit = |entry: Result<ignore::DirEntry, ignore::Error>| -> bool {
        match entry {
            Ok(e) if want(&e) => process_path(o, e.path()),
            Ok(_) => true,
            Err(e) => {
                if !o.quiet {
                    eprintln!("{e}");
                }
                false
            }
        }
    };

    if !parallel {
        wb.threads(1);
        return wb.build().map(visit).fold(true, |a, b| a & b);
    }

    let ok = AtomicBool::new(true);
    wb.build_parallel().run(|| {
        Box::new(|entry| {
            if !visit(entry) {
                ok.store(false, Ordering::Relaxed);
            }
            ignore::WalkState::Continue
        })
    });
    ok.load(Ordering::Relaxed)
}
