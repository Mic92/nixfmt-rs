//! nixfmt-rs2 CLI
//!
//! Mirrors the flag surface and exit-code semantics of the Haskell `nixfmt`
//! binary so the two can be used interchangeably by editors / CI.

use std::io::{self, Read, Write};
use std::process::exit;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
nixfmt_rs [OPTIONS] [FILES]
  Format Nix source code

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
struct Opts {
    width: usize,
    indent: usize,
    check: bool,
    quiet: bool,
    // Accepted for CLI parity; the formatter currently has no strict mode hook.
    _strict: bool,
    verify: bool,
    ast: bool,
    ir: bool,
    parse_only: bool,
    // Accepted for CLI parity; mergetool mode is not implemented.
    _mergetool: bool,
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
                println!("nixfmt_rs {VERSION}");
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
            "-s" | "--strict" => o._strict = true,
            "-v" | "--verify" => o.verify = true,
            "-a" | "--ast" => o.ast = true,
            "--ir" => o.ir = true,
            "--parse-only" => o.parse_only = true,
            "-m" | "--mergetool" => o._mergetool = true,
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
        nixfmt_rs::format_with(s, o.width, o.indent)
            .map_err(|e| nixfmt_rs::format_error(s, Some(name), &e))
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
        if out != source {
            if let Err(e) = std::fs::write(name, &out) {
                if !o.quiet {
                    eprintln!("{name}: {e}");
                }
                return false;
            }
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
        for path in &o.files {
            match std::fs::read_to_string(path) {
                Ok(source) => ok &= process(&o, path, &source, true),
                Err(e) => {
                    if !o.quiet {
                        eprintln!("{path}: {e}");
                    }
                    ok = false;
                }
            }
        }
    }

    exit(if ok { 0 } else { 1 });
}
