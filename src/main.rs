//! nixfmt-rs CLI
//!
//! Mirrors the flag surface and exit-code semantics of the Haskell `nixfmt`
//! binary so the two can be used interchangeably by editors / CI.

use std::io::{self, Read, Write};

use clap::{CommandFactory, Parser, ValueEnum};
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use std::path::Path;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};

use nixfmt_rs::VERSION;

mod json_diag;

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
    mergetool: bool,
    json_diagnostics: bool,
    filename: Option<String>,
    files: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum MessageFormat {
    Human,
    Json,
}

#[derive(Parser, Debug)]
#[command(
    name = "nixfmt-rs",
    about = env!("CARGO_PKG_DESCRIPTION"),
    long_about = "Format Nix source code (Rust implementation of nixfmt).\nUse '-' as a file argument to read from stdin.",
    disable_help_flag = true,
    disable_version_flag = true,
    trailing_var_arg = true
)]
struct CliArgs {
    #[arg(
        short = 'w',
        long = "width",
        default_value_t = 100,
        help = "Maximum width in characters"
    )]
    width: usize,
    #[arg(
        long = "indent",
        default_value_t = 2,
        help = "Number of spaces to use for indentation"
    )]
    indent: usize,
    #[arg(
        short = 'c',
        long = "check",
        help = "Check whether files are formatted without modifying them"
    )]
    check: bool,
    #[arg(short = 'q', long = "quiet", help = "Do not report errors")]
    quiet: bool,
    #[arg(
        short = 's',
        long = "strict",
        help = "Enable a stricter formatting mode (accepted, currently no-op)"
    )]
    strict: bool,
    #[arg(
        short = 'v',
        long = "verify",
        help = "Apply sanity checks on the output after formatting"
    )]
    verify: bool,
    #[arg(
        short = 'a',
        long = "ast",
        help = "Pretty print the internal AST to stderr (debug)"
    )]
    ast: bool,
    #[arg(long = "ir", help = "Pretty print the internal IR to stderr (debug)")]
    ir: bool,
    #[arg(long = "parse-only", help = "Only parse input and report parse errors")]
    parse_only: bool,
    #[arg(
        short = 'm',
        long = "mergetool",
        help = "Git mergetool mode: format BASE/LOCAL/REMOTE, run git merge-file, format and move result to MERGED"
    )]
    mergetool: bool,
    #[arg(
        long = "message-format",
        value_enum,
        default_value = "human",
        help = "How to render diagnostics: human or json"
    )]
    message_format: MessageFormat,
    #[arg(
        short = 'f',
        long = "filename",
        help = "Filename to display when input is read from stdin"
    )]
    filename: Option<String>,
    #[arg(short = '?', long = "help")]
    help: bool,
    #[arg(short = 'V', long = "version")]
    version: bool,
    #[arg(long = "numeric-version", help = "Print just the version number")]
    numeric_version: bool,
    #[arg(value_name = "FILES or -")]
    files: Vec<String>,
}

fn parse_args() -> Result<Opts, String> {
    let cli = CliArgs::try_parse().map_err(|e| {
        if matches!(e.kind(), clap::error::ErrorKind::UnknownArgument) {
            let argv = std::env::args().skip(1);
            if let Some(flag) = argv.into_iter().find(|a| a.starts_with('-')) {
                return format!("Unknown flag: {flag}");
            }
        }
        e.to_string()
    })?;

    if cli.help {
        let mut cmd = CliArgs::command();
        cmd.print_help().map_err(|e| e.to_string())?;
        println!();
        exit(0);
    }
    if cli.version {
        println!("nixfmt-rs {VERSION}");
        exit(0);
    }
    if cli.numeric_version {
        println!("{VERSION}");
        exit(0);
    }

    Ok(Opts {
        width: cli.width,
        indent: cli.indent,
        check: cli.check,
        quiet: cli.quiet,
        strict: cli.strict,
        verify: cli.verify,
        ast: cli.ast,
        ir: cli.ir,
        parse_only: cli.parse_only,
        mergetool: cli.mergetool,
        json_diagnostics: matches!(cli.message_format, MessageFormat::Json),
        filename: cli.filename,
        files: cli.files,
    })
}

fn render_err(o: &Opts, source: &str, name: &str, e: &nixfmt_rs::ParseError) -> String {
    if o.json_diagnostics {
        json_diag::parse_error(source, name, e)
    } else {
        nixfmt_rs::format_error(source, Some(name), e)
    }
}

fn report(o: &Opts, file: Option<&str>, severity: &str, msg: &str) {
    eprintln!("{}", render_msg(o, file, severity, msg));
}

fn try_format(o: &Opts, name: &str, source: &str) -> Result<String, String> {
    let fmt = |s: &str| {
        let mut opts = nixfmt_rs::Options::default();
        opts.width = o.width;
        opts.indent = o.indent;
        nixfmt_rs::format_with(s, &opts).map_err(|e| render_err(o, s, name, &e))
    };
    let out = fmt(source)?;
    if o.verify {
        let again = fmt(&out)?;
        if again != out {
            return Err(render_msg(
                o,
                Some(name),
                "error",
                "verify: output is not idempotent",
            ));
        }
    }
    Ok(out)
}

fn render_msg(o: &Opts, file: Option<&str>, severity: &str, msg: &str) -> String {
    if o.json_diagnostics {
        json_diag::message(file, severity, msg)
    } else if let Some(f) = file {
        format!("{f}: {msg}")
    } else {
        msg.to_string()
    }
}

/// Returns `true` on success so the caller can fold exit status across files.
fn process(o: &Opts, name: &str, source: &str, in_place: bool) -> bool {
    if o.parse_only {
        return match nixfmt_rs::parse(source) {
            Ok(_) => true,
            Err(e) => {
                if !o.quiet {
                    eprintln!("{}", render_err(o, source, name, &e));
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
                eprintln!("{}", render_err(o, source, name, &e));
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
                report(o, Some(name), "warning", "not formatted");
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
                report(o, Some(name), "error", &e.to_string());
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

    if o.mergetool {
        exit(i32::from(!run_mergetool(&o)));
    }

    let stdin_only = o.files.is_empty() || o.files.iter().all(|f| f == "-");
    if o.files.is_empty() && !o.quiet && !o.json_diagnostics {
        eprintln!(
            "Warning: Bare invocation of nixfmt-rs is deprecated. Use 'nixfmt-rs -' for anonymous stdin."
        );
    }
    if stdin_only {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            eprintln!("error: failed to read stdin: {e}");
            exit(1);
        }
        let name = o.filename.as_deref().unwrap_or("<stdin>");
        ok &= process(&o, name, &buf, false);
    } else if o.files.iter().any(|f| f == "-") {
        eprintln!("error: cannot mix '-' (stdin) with file arguments");
        exit(1);
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
                report(o, Some(&name), "error", &e.to_string());
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
                    report(o, None, "error", &e.to_string());
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

/// `git mergetool` mode: pre-format the three input revisions, invoke
/// `git merge-file`, format its output, then move it onto the MERGED path.
fn run_mergetool(o: &Opts) -> bool {
    let [base, local, remote, merged] = if let [b, l, r, m] = o.files.as_slice() {
        [b.as_str(), l.as_str(), r.as_str(), m.as_str()]
    } else {
        if !o.quiet {
            eprintln!(
                "--mergetool mode expects exactly 4 file arguments ($BASE, $LOCAL, $REMOTE, $MERGED)"
            );
        }
        return false;
    };

    if Path::new(merged)
        .extension()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("nix"))
    {
        if !o.quiet {
            eprintln!("Skipping non-Nix file {merged}");
        }
        return false;
    }

    let pre_format = |label: &str, path: &str| -> bool {
        if process_path(o, Path::new(path)) {
            return true;
        }
        if !o.quiet {
            eprintln!("pre-formatting the {label} version failed");
        }
        false
    };

    let mut ok = pre_format("base", base);
    ok &= pre_format("local", local);
    ok &= pre_format("remote", remote);
    if !ok {
        return false;
    }

    // git merge-file's nonzero exit is the conflict count, not an error;
    // only spawn / signal failures are fatal here.
    let status = match std::process::Command::new("git")
        .args(["merge-file", local, base, remote])
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            if !o.quiet {
                eprintln!("failed to run git merge-file: {e}");
            }
            return false;
        }
    };
    if status.code().is_none() {
        if !o.quiet {
            eprintln!("git merge-file terminated by signal");
        }
        return false;
    }

    if !process_path(o, Path::new(local)) {
        return false;
    }

    if let Err(e) = std::fs::rename(local, merged) {
        if !o.quiet {
            eprintln!("failed to move {local} to {merged}: {e}");
        }
        return false;
    }

    // Forward merge-file's status so `git mergetool` sees conflict count.
    status.success()
}
