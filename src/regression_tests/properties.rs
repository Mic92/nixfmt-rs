//! Property-based regression harness over the vendored upstream nixfmt test
//! corpus (`tests/fixtures/nixfmt/`).
//!
//! Asserts two invariants the formatter must always uphold:
//!   1. Idempotency:      format(format(x)) == format(x)
//!   2. AST preservation: parse(format(x)) ==_strip_trivia parse(x)
//!
//! The corpus is checked into this repo so the tests are hermetic. Files our
//! parser cannot parse yet are skipped (those are tracked by the parser
//! regression tests, not here).

use crate::tests_common::diff;
use crate::types::*;
use std::path::{Path as FsPath, PathBuf};

// ---------------------------------------------------------------------------
// Trivia stripping
// ---------------------------------------------------------------------------

/// Zero out all source-position / comment information so two ASTs can be
/// compared purely on syntactic structure. The derived `PartialEq` on the
/// AST types then gives us the equality we want.
trait StripTrivia {
    fn strip_trivia(&mut self);
}

fn scrub<T>(ann: &mut Ann<T>) {
    ann.pre_trivia = Trivia::new();
    ann.trail_comment = None;
    ann.span = Span::new(0, 0);
}

impl StripTrivia for Leaf {
    fn strip_trivia(&mut self) {
        scrub(self);
    }
}

impl StripTrivia for StringPart {
    fn strip_trivia(&mut self) {
        if let StringPart::Interpolation(whole) = self {
            whole.strip_trivia();
        }
    }
}

impl StripTrivia for Ann<StringPart> {
    fn strip_trivia(&mut self) {
        scrub(self);
        self.value.strip_trivia();
    }
}

impl StripTrivia for Path {
    fn strip_trivia(&mut self) {
        scrub(self);
        for p in &mut self.value {
            p.strip_trivia();
        }
    }
}

impl StripTrivia for NixString {
    fn strip_trivia(&mut self) {
        scrub(self);
        for line in &mut self.value {
            for p in line {
                p.strip_trivia();
            }
        }
    }
}

impl StripTrivia for SimpleSelector {
    fn strip_trivia(&mut self) {
        match self {
            SimpleSelector::ID(l) => l.strip_trivia(),
            SimpleSelector::Interpol(a) => a.strip_trivia(),
            SimpleSelector::String(s) => s.strip_trivia(),
        }
    }
}

impl StripTrivia for Selector {
    fn strip_trivia(&mut self) {
        if let Some(d) = &mut self.dot {
            d.strip_trivia();
        }
        self.selector.strip_trivia();
    }
}

impl<T: StripTrivia> StripTrivia for Items<T> {
    fn strip_trivia(&mut self) {
        // Interleaved comment items are pure trivia.
        self.0.retain(|i| matches!(i, Item::Item(_)));
        for item in &mut self.0 {
            if let Item::Item(v) = item {
                v.strip_trivia();
            }
        }
    }
}

impl StripTrivia for Binder {
    fn strip_trivia(&mut self) {
        match self {
            Binder::Inherit(kw, from, sels, semi) => {
                kw.strip_trivia();
                if let Some(t) = from {
                    t.strip_trivia();
                }
                for s in sels {
                    s.strip_trivia();
                }
                semi.strip_trivia();
            }
            Binder::Assignment(sels, eq, expr, semi) => {
                for s in sels {
                    s.strip_trivia();
                }
                eq.strip_trivia();
                expr.strip_trivia();
                semi.strip_trivia();
            }
        }
    }
}

impl StripTrivia for ParamAttr {
    fn strip_trivia(&mut self) {
        match self {
            ParamAttr::ParamAttr(name, def, comma) => {
                name.strip_trivia();
                if let Some((q, e)) = def.as_mut() {
                    q.strip_trivia();
                    e.strip_trivia();
                }
                if let Some(c) = comma {
                    c.strip_trivia();
                }
            }
            ParamAttr::ParamEllipsis(l) => l.strip_trivia(),
        }
    }
}

impl StripTrivia for Parameter {
    fn strip_trivia(&mut self) {
        match self {
            Parameter::ID(l) => l.strip_trivia(),
            Parameter::Set(open, attrs, close) => {
                open.strip_trivia();
                // The formatter intentionally normalises the trailing comma on
                // the last attr; treat its presence/absence as trivia.
                if let Some(ParamAttr::ParamAttr(_, _, c)) = attrs.last_mut() {
                    *c = None;
                }
                for a in attrs {
                    a.strip_trivia();
                }
                close.strip_trivia();
            }
            Parameter::Context(a, at, b) => {
                a.strip_trivia();
                at.strip_trivia();
                b.strip_trivia();
            }
        }
    }
}

impl StripTrivia for Term {
    fn strip_trivia(&mut self) {
        match self {
            Term::Token(l) => l.strip_trivia(),
            Term::SimpleString(s) | Term::IndentedString(s) => s.strip_trivia(),
            Term::Path(p) => p.strip_trivia(),
            Term::List(o, items, c) => {
                o.strip_trivia();
                items.strip_trivia();
                c.strip_trivia();
            }
            Term::Set(rec, o, items, c) => {
                if let Some(r) = rec {
                    r.strip_trivia();
                }
                o.strip_trivia();
                items.strip_trivia();
                c.strip_trivia();
            }
            Term::Selection(t, sels, default) => {
                t.strip_trivia();
                for s in sels {
                    s.strip_trivia();
                }
                if let Some((or_kw, d)) = default {
                    or_kw.strip_trivia();
                    d.strip_trivia();
                }
            }
            Term::Parenthesized(o, e, c) => {
                o.strip_trivia();
                e.strip_trivia();
                c.strip_trivia();
            }
        }
    }
}

impl StripTrivia for Expression {
    fn strip_trivia(&mut self) {
        match self {
            Expression::Term(t) => t.strip_trivia(),
            Expression::With(kw, a, semi, b) | Expression::Assert(kw, a, semi, b) => {
                kw.strip_trivia();
                a.strip_trivia();
                semi.strip_trivia();
                b.strip_trivia();
            }
            Expression::Let(kw, binders, in_kw, body) => {
                kw.strip_trivia();
                binders.strip_trivia();
                in_kw.strip_trivia();
                body.strip_trivia();
            }
            Expression::If(i, c, t, a, e, b) => {
                i.strip_trivia();
                c.strip_trivia();
                t.strip_trivia();
                a.strip_trivia();
                e.strip_trivia();
                b.strip_trivia();
            }
            Expression::Abstraction(p, colon, body) => {
                p.strip_trivia();
                colon.strip_trivia();
                body.strip_trivia();
            }
            Expression::Application(f, a) => {
                f.strip_trivia();
                a.strip_trivia();
            }
            Expression::Operation(a, op, b) => {
                a.strip_trivia();
                op.strip_trivia();
                b.strip_trivia();
            }
            Expression::MemberCheck(e, q, sels) => {
                e.strip_trivia();
                q.strip_trivia();
                for s in sels {
                    s.strip_trivia();
                }
            }
            Expression::Negation(op, e) | Expression::Inversion(op, e) => {
                op.strip_trivia();
                e.strip_trivia();
            }
        }
    }
}

impl<T: StripTrivia> StripTrivia for Whole<T> {
    fn strip_trivia(&mut self) {
        self.value.strip_trivia();
        self.trailing_trivia = Trivia::new();
    }
}

// ---------------------------------------------------------------------------
// Input collection
// ---------------------------------------------------------------------------

fn collect_fixture_nix_files(dir: &FsPath, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_fixture_nix_files(&p, out);
        } else if p.extension().map(|e| e == "nix").unwrap_or(false) {
            out.push(p);
        }
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nixfmt")
}

fn collect_inputs() -> Vec<PathBuf> {
    let root = fixture_root();
    let mut files = Vec::new();
    // `correct/` are already-formatted snippets; `diff/*/` contain in/out pairs.
    collect_fixture_nix_files(&root.join("correct"), &mut files);
    collect_fixture_nix_files(&root.join("diff"), &mut files);
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// Diff helper: only show changed hunks with a couple of lines of context.
// ---------------------------------------------------------------------------

fn minimised_diff(a: &str, b: &str) -> String {
    diff::render(
        a,
        b,
        diff::DiffOpts {
            context: Some(2),
            color: false,
        },
    )
}

// ---------------------------------------------------------------------------
// Corpus driver
// ---------------------------------------------------------------------------

/// Run `check(path, src, ast)` over every fixture that parses, print a
/// `[tag] checked N files, M failures` summary, and panic if any check
/// returned `Err`. Unparseable inputs are skipped (parser gaps are tracked by
/// the parser regression suite).
fn for_each_parsed_fixture(
    tag: &str,
    mut check: impl FnMut(&FsPath, &str, File) -> Result<(), String>,
) {
    let files = collect_inputs();
    assert!(!files.is_empty(), "fixture corpus missing");
    let mut failures = 0usize;
    let mut checked = 0usize;
    for path in &files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(ast) = crate::parse(&src) else {
            continue;
        };
        checked += 1;
        if let Err(msg) = check(path, &src, ast) {
            eprintln!("\n[{tag}] {}: {msg}", path.display());
            failures += 1;
        }
    }
    eprintln!("[{tag}] checked {checked} files, {failures} failures");
    if failures > 0 {
        panic!("[{tag}] {failures} file(s) failed");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn idempotent_on_fixture_corpus() {
    for_each_parsed_fixture("idempotency", |_path, src, _ast| {
        // `format` only errs on parse, which the driver already accepted.
        let once = crate::format(src).map_err(|e| format!("format failed: {e:?}"))?;
        let twice = crate::format(&once).map_err(|e| format!("reparse failed: {e:?}"))?;
        if once != twice {
            return Err(format!(
                "format is not idempotent\n{}",
                minimised_diff(&once, &twice)
            ));
        }
        Ok(())
    });
}

#[test]
fn ast_preserved_on_fixture_corpus() {
    for_each_parsed_fixture("ast-preservation", |_path, src, mut before| {
        let formatted = crate::format(src).map_err(|e| format!("format failed: {e:?}"))?;
        let mut after = crate::parse(&formatted)
            .map_err(|e| format!("formatted output failed to parse: {e:?}"))?;
        before.strip_trivia();
        after.strip_trivia();
        if before != after {
            return Err(format!(
                "AST changed by formatting\n{}",
                minimised_diff(src, &formatted)
            ));
        }
        Ok(())
    });
}

/// For every `diff/*/in.nix` fixture, format the input and compare against the
/// upstream `out.nix` golden. Unlike the two property tests above this is *not*
/// an invariant we already uphold — it tracks remaining divergence from the
/// reference formatter — so mismatches are logged and counted but do **not**
/// fail the test. Parse errors on our own output, however, do.
#[test]
fn formats_to_golden_on_fixture_corpus() {
    let diff_root = fixture_root().join("diff");
    let mut dirs: Vec<_> = std::fs::read_dir(&diff_root)
        .expect("diff fixture dir missing")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no diff/* fixtures found");

    let mut checked = 0usize;
    let mut matched = 0usize;
    let mut diverged = 0usize;
    let mut skipped_parse = 0usize;
    for dir in &dirs {
        let in_path = dir.join("in.nix");
        let out_path = dir.join("out.nix");
        let (Ok(input), Ok(expected)) = (
            std::fs::read_to_string(&in_path),
            std::fs::read_to_string(&out_path),
        ) else {
            continue;
        };
        let got = match crate::format(&input) {
            Ok(s) => s,
            Err(_) => {
                // Parser gaps are tracked by the parser regression suite.
                skipped_parse += 1;
                continue;
            }
        };
        checked += 1;
        if got == expected {
            matched += 1;
        } else {
            diverged += 1;
            eprintln!(
                "\n[golden] {}: diverges from out.nix",
                dir.strip_prefix(&diff_root).unwrap_or(dir).display()
            );
            eprintln!("{}", minimised_diff(&expected, &got));
        }
    }
    eprintln!(
        "[golden] {checked} checked, {matched} match, {diverged} diverge, \
         {skipped_parse} skipped (parse)"
    );
    // Divergence is expected while we close the gap; only assert we actually
    // exercised the corpus.
    assert!(checked > 0, "no diff fixtures were checked");
}

/// Every fixture under `invalid/` must be *rejected* by our parser. These are
/// inputs the reference `nixfmt` refuses; accepting any of them is a parser
/// bug (we'd silently produce garbage on bad syntax).
#[test]
fn rejects_invalid_fixture_corpus() {
    let mut files = Vec::new();
    collect_fixture_nix_files(&fixture_root().join("invalid"), &mut files);
    files.sort();
    assert!(!files.is_empty(), "invalid/ fixture corpus missing");

    let mut accepted = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read fixture");
        if crate::parse(&src).is_ok() {
            accepted.push(f.clone());
        }
    }
    if !accepted.is_empty() {
        for f in &accepted {
            eprintln!("[invalid] wrongly ACCEPTED: {}", f.display());
        }
        panic!(
            "{} invalid fixture(s) were accepted (should be parse errors)",
            accepted.len()
        );
    }
    eprintln!(
        "[invalid] all {} fixture(s) correctly rejected",
        files.len()
    );
}
