# The Haskell `nixfmt` used as the byte-for-byte reference in the test suite
# (`run_reference_nixfmt` in `src/tests_common/ast_format.rs`).
#
# We carry a small patch set on top of the nixpkgs build that fixes upstream
# idempotency / reparse bugs found by fuzzing. nixfmt-rs mirrors the *patched*
# behaviour, so `--ast`/`--ir`/format parity tests stay meaningful instead of
# accumulating "intentional divergence" exceptions. The patches are also the
# upstream PR; drop them here as they get merged.
{ nixfmt }:
nixfmt.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [
    ./patches/0001-Parser-drop-leading-blank-lines-on-the-file-s-first-.patch
    ./patches/0002-Lexer-keep-a-comment-trailing-after-a-multi-line-tok.patch
    ./patches/0003-Pretty-do-not-hoist-a-lone-language-annotation-off-a.patch
    ./patches/0004-Pretty-suppress-language-annotation-fuse-after-an-op.patch
    ./patches/0005-Pretty-ignore-trivia-in-isAbsorbable-for-Parenthesiz.patch
  ];
})
