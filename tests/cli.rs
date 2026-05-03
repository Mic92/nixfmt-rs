//! CLI behaviour parity tests against the Haskell `nixfmt` binary.
//!
//! These tests exercise the *interface* (flags, stdio routing, exit codes,
//! in-place writes), not formatting fidelity. Where the reference binary's
//! behaviour is deterministic and content-independent we assert exact parity.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nixfmt"))
}

fn run(program: &str, args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {program}: {e}"));
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

fn ours(args: &[&str], stdin: Option<&str>) -> Output {
    run(bin().to_str().unwrap(), args, stdin)
}

fn nixfmt(args: &[&str], stdin: Option<&str>) -> Output {
    run("nixfmt", args, stdin)
}

fn tmpfile(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, content).unwrap();
    p
}

const UNFORMATTED: &str = "{a=1;}\n";
const FORMATTED: &str = "{ a = 1; }\n";
const INVALID: &str = "{a=1;\n";

#[test]
fn stdin_formats_to_stdout() {
    let out = ours(&[], Some(UNFORMATTED));
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), FORMATTED);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Bare invocation"),
        "expected deprecation warning, got stderr={stderr:?}"
    );

    let ref_out = nixfmt(&[], Some(UNFORMATTED));
    assert_eq!(out.stdout, ref_out.stdout);
    assert_eq!(out.status.code(), ref_out.status.code());
}

#[test]
fn dash_reads_stdin_without_warning() {
    let out = ours(&["-"], Some(UNFORMATTED));
    assert!(out.status.success(), "stderr={:?}", out.stderr);
    assert_eq!(String::from_utf8_lossy(&out.stdout), FORMATTED);
    assert!(
        out.stderr.is_empty(),
        "no warning when '-' is explicit, got {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_unformatted_stdin_exits_1() {
    for flag in ["-c", "--check"] {
        let out = ours(&[flag], Some(UNFORMATTED));
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty(), "no output in check mode");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("not formatted"), "stderr={stderr:?}");

        let ref_out = nixfmt(&[flag], Some(UNFORMATTED));
        assert_eq!(out.status.code(), ref_out.status.code());
    }
}

#[test]
fn check_formatted_stdin_exits_0() {
    let out = ours(&["--check", "-"], Some(FORMATTED));
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());

    let ref_out = nixfmt(&["--check", "-"], Some(FORMATTED));
    assert_eq!(ref_out.status.code(), Some(0));
}

#[test]
fn check_file_does_not_modify() {
    let d = tempfile::tempdir().unwrap();
    let p = tmpfile(&d, "check.nix", UNFORMATTED);
    let out = ours(&["--check", p.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), UNFORMATTED);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not formatted"));
    assert!(stderr.contains(p.file_name().unwrap().to_str().unwrap()));
}

#[test]
fn files_are_formatted_in_place() {
    let d = tempfile::tempdir().unwrap();
    let a = tmpfile(&d, "a.nix", UNFORMATTED);
    let b = tmpfile(&d, "b.nix", "{b=2;}\n");
    let out = ours(&[a.to_str().unwrap(), b.to_str().unwrap()], None);
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "in-place mode writes nothing to stdout"
    );
    assert_eq!(std::fs::read_to_string(&a).unwrap(), FORMATTED);
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "{ b = 2; }\n");
}

#[test]
fn multiple_files_error_continues_and_exits_1() {
    let d = tempfile::tempdir().unwrap();
    let bad = tmpfile(&d, "bad.nix", INVALID);
    let good = tmpfile(&d, "good.nix", UNFORMATTED);
    let out = ours(&[bad.to_str().unwrap(), good.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&good).unwrap(), FORMATTED);
    assert_eq!(std::fs::read_to_string(&bad).unwrap(), INVALID);

    let rbad = tmpfile(&d, "rbad.nix", INVALID);
    let rgood = tmpfile(&d, "rgood.nix", UNFORMATTED);
    let ref_out = nixfmt(&[rbad.to_str().unwrap(), rgood.to_str().unwrap()], None);
    assert_eq!(ref_out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&rgood).unwrap(), FORMATTED);
}

#[test]
fn directory_is_walked_recursively_and_formatted_in_place() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.nix"), UNFORMATTED).unwrap();
    std::fs::write(dir.join("sub/b.nix"), UNFORMATTED).unwrap();
    std::fs::write(dir.join("README.md"), "# not nix\n").unwrap();

    let out = ours(&[dir.to_str().unwrap()], None);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join("a.nix")).unwrap(),
        FORMATTED
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("sub/b.nix")).unwrap(),
        FORMATTED
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("README.md")).unwrap(),
        "# not nix\n"
    );
}

#[test]
fn directory_check_reports_unformatted_and_exits_1() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    std::fs::write(dir.join("ok.nix"), FORMATTED).unwrap();
    std::fs::write(dir.join("bad.nix"), UNFORMATTED).unwrap();

    let out = ours(&["-c", dir.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("bad.nix"));
    assert!(!stderr.contains("ok.nix"));
    // --check must not modify files.
    assert_eq!(
        std::fs::read_to_string(dir.join("bad.nix")).unwrap(),
        UNFORMATTED
    );
}

#[test]
fn missing_file_exits_1() {
    let out = ours(&["/nonexistent/path/xyz.nix"], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(!out.stderr.is_empty());

    let ref_out = nixfmt(&["/nonexistent/path/xyz.nix"], None);
    assert_eq!(ref_out.status.code(), Some(1));
}

#[test]
fn parse_error_exits_1() {
    let out = ours(&[], Some(INVALID));
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());

    let ref_out = nixfmt(&[], Some(INVALID));
    assert_eq!(out.status.code(), ref_out.status.code());
}

#[test]
fn quiet_suppresses_errors_but_keeps_exit_code() {
    let out = ours(&["-q"], Some(INVALID));
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty(), "quiet must suppress stderr");

    let ref_out = nixfmt(&["-q"], Some(INVALID));
    assert_eq!(ref_out.status.code(), Some(1));
    assert!(ref_out.stderr.is_empty());
}

#[test]
fn quiet_suppresses_check_message() {
    let out = ours(&["-c", "-q"], Some(UNFORMATTED));
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
}

#[test]
fn width_flag_forces_multiline() {
    let src = "{ aaaaaaaa = 1; bbbbbbbb = 2; }\n";
    let narrow = ours(&["--width=10"], Some(src));
    assert!(narrow.status.success());
    let narrow_s = String::from_utf8_lossy(&narrow.stdout);
    assert!(
        narrow_s.lines().count() > 1,
        "expected multi-line at width 10, got {narrow_s:?}"
    );

    let alt = ours(&["-w", "10"], Some(src));
    assert_eq!(alt.stdout, narrow.stdout);
    let alt2 = ours(&["-w10"], Some(src));
    assert_eq!(alt2.stdout, narrow.stdout);

    let ref_out = nixfmt(&["--width=10"], Some(src));
    assert_eq!(narrow.stdout, ref_out.stdout);
}

#[test]
fn indent_flag_changes_indentation() {
    let src = "{\n  a = 1;\n}\n";
    let out = ours(&["--indent=4"], Some(src));
    assert!(out.status.success());
    let ref_out = nixfmt(&["--indent=4"], Some(src));
    assert_eq!(out.stdout, ref_out.stdout);
    assert!(String::from_utf8_lossy(&out.stdout).contains("    a"));
}

#[test]
fn debug_dumps_go_to_stderr_and_exit_1() {
    for flag in ["--ast", "--ir"] {
        let out = ours(&[flag], Some("1\n"));
        assert_eq!(
            out.status.code(),
            Some(1),
            "{flag}: debug dumps exit non-zero"
        );
        assert!(out.stdout.is_empty(), "{flag} must not write to stdout");
        assert!(!out.stderr.is_empty(), "{flag} writes to stderr");

        let ref_out = nixfmt(&[flag], Some("1\n"));
        assert_eq!(ref_out.status.code(), Some(1));
        assert!(ref_out.stdout.is_empty());
    }
}

#[test]
fn ast_on_file_does_not_modify() {
    let d = tempfile::tempdir().unwrap();
    let p = tmpfile(&d, "ast.nix", UNFORMATTED);
    let out = ours(&["--ast", p.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), UNFORMATTED);
}

#[test]
fn strict_flag_is_accepted() {
    let out = ours(&["--strict"], Some(FORMATTED));
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), FORMATTED);
}

#[test]
fn verify_flag_is_accepted() {
    let out = ours(&["--verify"], Some(UNFORMATTED));
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), FORMATTED);

    let ref_out = nixfmt(&["--verify"], Some(UNFORMATTED));
    assert_eq!(out.status.code(), ref_out.status.code());
}

#[test]
fn filename_flag_used_in_errors() {
    let out = ours(&["--filename=custom.nix"], Some(INVALID));
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("custom.nix"), "stderr={stderr:?}");

    let ref_out = nixfmt(&["--filename=custom.nix"], Some(INVALID));
    let ref_err = String::from_utf8_lossy(&ref_out.stderr);
    assert!(ref_err.contains("custom.nix"));
}

#[test]
fn filename_flag_used_in_check_message() {
    let out = ours(&["-c", "--filename", "foo.nix"], Some(UNFORMATTED));
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("foo.nix: not formatted"),
        "stderr={stderr:?}"
    );
}

#[test]
fn help_exits_0() {
    let out = ours(&["--help"], None);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("--check"));

    let ref_out = nixfmt(&["--help"], None);
    assert_eq!(ref_out.status.code(), Some(0));
}

#[test]
fn version_flags() {
    let out = ours(&["--version"], None);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));

    let out = ours(&["--numeric-version"], None);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn unknown_long_flag_starting_with_w() {
    // Regression: `-wNN` shorthand must not swallow unrelated `--w*` flags.
    let out = ours(&["--wrong"], None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Unknown flag"), "stderr={stderr:?}");
}

#[test]
fn unknown_flag_exits_1() {
    let out = ours(&["--bogus"], None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Unknown flag"), "stderr={stderr:?}");

    let ref_out = nixfmt(&["--bogus"], None);
    assert_eq!(ref_out.status.code(), Some(1));
}
