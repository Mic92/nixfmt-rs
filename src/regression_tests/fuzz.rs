//! Regressions discovered by the `cargo-fuzz` harness in `fuzz/`.
//!
//! Each case is a minimised crash/divergence; the assertions encode the
//! invariant that was violated.

use crate::tests_common::test_format;
use crate::{format, parse_normalized};

fn roundtrip(input: &str) {
    let ast1 = parse_normalized(input).expect("input must parse");
    let f1 = format(input).expect("format must succeed");
    let ast2 = parse_normalized(&f1)
        .unwrap_or_else(|e| panic!("formatted output must re-parse: {e:?}\n{f1}"));
    assert_eq!(
        ast1, ast2,
        "AST changed after format\ninput: {input:?}\nformatted: {f1:?}"
    );
    let f2 = format(&f1).expect("second format must succeed");
    assert_eq!(f1, f2, "format not idempotent\nf1: {f1:?}\nf2: {f2:?}");
}

/// `or` after a non-selection term used to be parsed and silently discarded,
/// turning `t or a` into `t`. Also a bug in upstream Haskell nixfmt.
#[test]
fn fuzz_or_without_selectors_dropped() {
    roundtrip("t or a");
}

/// `1\n.a` is a selection on an integer; emitting it as `1.a` re-lexes as the
/// float `1.` applied to `a`. The printer must keep a space.
#[test]
fn fuzz_integer_selection_relex_as_float() {
    roundtrip("1\n.a");
    assert_eq!(format("1 .a").unwrap(), "1 .a\n");
    // Floats need no protective space; guard the fix against over-application.
    test_format("1.5.x");
    test_format("1..x");
}

/// `''\` followed by a newline must not swallow the newline into the text
/// part: doing so hid the next line from common-indent stripping and made
/// the output grow by two spaces on every format pass.
#[test]
fn fuzz_indented_string_escape_newline_grows() {
    roundtrip("''''\\\nX''");
}

/// A blank (whitespace-only) line inside an indented string must not cap the
/// common indentation; Nix ignores such lines for that calculation.
#[test]
fn fuzz_indented_string_blank_line_indent() {
    roundtrip("[''\n \n  h'']");
}

/// Blank lines at least as long as the common indent keep their excess spaces
/// (Nix evaluation semantics); only shorter blank lines are cleared.
#[test]
fn fuzz_indented_string_blank_line_preserves_excess() {
    test_format("''\n   \n  b''");
    test_format("''\n  \nb''");
}

/// `/* ... */` containing a bare `\r` (no `\n`) was treated as single-line and
/// rewritten to `# ...`, but the lexer (and Nix itself) ends a line comment at
/// `\r` too, so the bytes after it re-lexed as code on the next pass.
/// `split_lines` now splits on bare `\r` as well, so such bodies become
/// multi-line and stay in block form. Upstream Haskell nixfmt has the same bug.
#[test]
fn fuzz_block_comment_with_cr_reparses() {
    roundtrip("2/*\0\r\0");
    roundtrip("/*a\r\0*/3");
    roundtrip("/*a\rb*/3");
    roundtrip("2 /* x\r */\n");
}

/// A trailing `# c` on `[` is rendered on its own line, where it re-lexes as a
/// *leading* comment on the first item. With a lone `/* lang */` annotation as
/// the first item, pass 1 fused it with the term (`/* s */ x`) but pass 2 saw
/// `[LineComment, LanguageAnnotation]` and split them. Upstream fix carried in
/// `nix/patches/0004-*.patch`.
#[test]
fn fuzz_list_open_trail_comment_before_lang_annotation() {
    roundtrip("[#x\n/*s*/\"\"\n]");
    roundtrip("[#x\n/*s*/''ls''\n]");
    // Guard the fix against over-application: without a trailing comment on
    // `[`, the annotation must still fuse.
    assert_eq!(format("[\n/*s*/\"\"\n]").unwrap(), "[\n  /* s */ \"\"\n]\n");
}

/// Leading blank lines on the file's first token made `Term::is_simple` return
/// `false`, so `prettyApp` took the non-simple branch on pass 1; pass 2 (with
/// the blanks now stripped) took the simple branch. Upstream fix carried in
/// `nix/patches/0001-*.patch`.
#[test]
fn fuzz_leading_blank_lines_app_layout() {
    roundtrip("\n\nc\n\"${f\n\n}\"");
    roundtrip("\n\nc d \"${f\n\n}\"");
    roundtrip("\r\rc\n''${f\n\n}''");
    // Nested contexts preserve the blank line in output and were already
    // idempotent; guard the fix against over-application there.
    roundtrip("(\n\nc \"${f\n\n}\")");
    // A real leading comment must still suppress the simple layout.
    assert_eq!(
        format("# foo\nc \"${f\n\n}\"").unwrap(),
        "# foo\nc\n  \"${\n    f\n\n  }\"\n"
    );
}

/// `prettyApp` hoisted a lone `/* lang */` annotation off the call head and
/// emitted it before `ctx.pre`, so the annotation landed directly after the
/// preceding operator with no space. For `/` this produced `a //* sh */ …`,
/// which re-lexes as the `//` update operator and fails to parse. Upstream fix
/// carried in `nix/patches/0003-*.patch`.
#[test]
fn fuzz_lang_annotation_on_app_head_after_div() {
    roundtrip("H/\n/*h*/''''p");
    assert_eq!(
        format("a / /* sh */ \"\" p\n").unwrap(),
        "a / /* sh */ \"\" p\n"
    );
    // Same hoist also dropped the space after other operators / `=`; not a
    // parse error there but still wrong, and now fixed as a side effect.
    assert_eq!(
        format("a + /* sh */ \"\" p\n").unwrap(),
        "a + /* sh */ \"\" p\n"
    );
    assert_eq!(
        format("{a = /* sh */ \"\" p;}\n").unwrap(),
        "{ a = /* sh */ \"\" p; }\n"
    );
    // Inside parens / at top level the annotation was already adjacent to a
    // delimiter that needs no space; keep that.
    roundtrip("(/* sh */ \"\" p)");
    roundtrip("/* sh */ \"\" p");
}

/// A `# c` trailing the closing `"` of a `"…"` literal that spans multiple
/// source lines lands at the next sibling's indent column, so `convert_trivia`
/// reattached it as leading on pass 2 but not pass 1. Upstream fix carried in
/// `nix/patches/0002-*.patch`.
#[test]
fn fuzz_trailing_comment_on_multiline_simple_string() {
    roundtrip("[\"\n\"#c\nt]");
    roundtrip("{a=\"\n\"#c\n;b=1;}");
    roundtrip("f \"\n\"#c\nt");
    roundtrip("(\"\n\"#c\n)");
    // Indented strings were already idempotent (closing `''` sits at indent
    // col); the patch also keeps `# c` trailing here, which differs from
    // unpatched upstream's own-line form but is the less surprising fixed
    // point. Pinned via the reference.
    test_format("[''\na\n''#c\nt]");
}
