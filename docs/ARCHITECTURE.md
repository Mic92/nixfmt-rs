# Architecture

`nixfmt-rs` is a straight port of the Haskell [nixfmt] pipeline. Each
stage has a 1:1 counterpart in `nixfmt/src/Nixfmt/*.hs`, and the
intermediate representations are dumpable (`--ast`, `--ir`) so the two
implementations can be diffed at every seam.

```
            src/lexer/        src/parser/       src/format/      src/doc/
Nix text ───► tokens+trivia ───► AST (Annotated<T>) ───► Doc IR ───► fixup ───► layout ───► text
            Nixfmt/Lexer.hs   Nixfmt/Parser.hs  Nixfmt/Pretty.hs   Nixfmt/Predoc.hs
                              Nixfmt/Types.hs
```

[nixfmt]: https://github.com/NixOS/nixfmt

## Lexer — `src/lexer/`

Hand-written scanner that emits `Annotated<Token>`: every token carries the
*leading* trivia (blank lines, line/block comments, language
annotations) plus an optional same-line trailing comment. Mirrors
`Nixfmt/Lexer.hs`'s `lexeme`/`takeTrivia` split; sub-expressions inside
`${…}` get an isolated trivia buffer the way `whole` does upstream.

## Parser — `src/parser/`, `src/ast/`

Recursive descent producing the AST in `src/ast/`, which is a
field-for-field transcription of `Nixfmt/Types.hs` (`Annotated`, `Trivium`,
`Item`, `Expression`, `Term`, …). Because the types line up, the
`--ast` dump (rendered by `src/dump/`) is byte-identical to
Haskell `show` filtered through *pretty-simple*, which is what the
regression suite asserts.

## Format — `src/format/`

`impl Emit for <AST node>` turns the tree into the **Doc IR**. This
is where all formatting policy lives: absorption (`absorbRHS`,
`absorbLast`, `absorbParen`), application layout (`prettyApp`),
`if`/`let`/`with` shaping, operator chains. Every non-trivial function
carries a doc comment naming the `Nixfmt/Pretty.hs` definition it
ports, because IR-level divergences are debugged by reading the two
side by side.

## Doc — `src/doc/`

The Wadler/Leijen-style layout engine, ported from `Nixfmt/Predoc.hs`.

```rust
pub type Doc = Vec<Elem>;
enum Elem { Text(..), Spacing(Spacing), Group(GroupKind, Doc) }

enum Spacing  { Softbreak, Break, Hardspace, Softspace, Space,
                Hardline, Emptyline, Newlines(usize) }
enum GroupKind { Regular, Transparent, Priority }
```

- **`fixup`** normalises the tree: merges adjacent `Spacing` to their
  maximum, floats spacing out of group boundaries, propagates nesting.
- **`layout`** is the greedy renderer: for each `Group`, try the
  single-line form (`fits`, which drops `TextKind::Trailing` commas); if
  it overflows, try expanding only the contained `Priority` sub-groups
  (last first); otherwise expand the whole group.

The `--ir` flag prints the post-`fixup` Doc so it can be diffed against
`nixfmt --ir`.

## Debug printers — `src/dump/`, `src/colored_writer.rs`

`dump` reproduces Haskell `Show` + the *pretty-simple* layout
rules (the `is_simple` / `is_atomic` / `has_delimiters` triad) so that
`--ast` output matches upstream exactly without us having to serialise
through an actual `Show` string.
