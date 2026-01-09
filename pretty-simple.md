# Pretty-Simple Formatting - Final Design

This document explains the `src/pretty_simple.rs` implementation, which formats Rust AST to match Haskell nixfmt's `--ast` output using the pretty-simple library's formatting rules.

## Overview

The implementation directly formats Rust data structures to match Haskell's `Show` output as processed by pretty-simple, achieving exact compatibility with nixfmt's AST output for testing.

## Core Trait: `PrettySimple`

```rust
trait PrettySimple {
    fn format<W: Writer>(&self, w: &mut W);
    fn is_simple(&self) -> bool { false }
    fn has_delimiters(&self) -> bool { false }
    fn is_empty(&self) -> bool { false }
    fn is_atomic(&self) -> bool { false }
}
```

### Trait Methods Explained

#### `is_simple()` - Can Render Inline?

Determines if an element can be formatted on a single line in a row.

**True for:**
- Primitives: `String`, `usize`, `bool`
- Nullary constructors: `EmptyLine`
- Constructor applications with simple arguments: `LineComment("text")`, `TextPart("hello")`, `BlockComment(True, ["doc"])`
- Empty or single-simple-element containers: `[]`, `[EmptyLine]`

**False for:**
- Multi-element containers: `["a", "b", "c"]`
- Constructors with complex arguments: `BlockComment(True, ["a", "b", "c"])`
- Nested complex structures

**Implementation pattern:**
```rust
// Primitives
impl PrettySimple for String {
    fn is_simple(&self) -> bool { true }
}

// Nullary constructors
impl PrettySimple for Trivium {
    fn is_simple(&self) -> bool {
        match self {
            Trivium::EmptyLine() => true,
            Trivium::LineComment(_) => true,  // Simple arg
            Trivium::BlockComment(_, lines) => lines.is_simple(),  // Check args
            Trivium::LanguageAnnotation(_) => true,  // Simple arg
        }
    }
}

// Containers
impl<T> PrettySimple for Vec<T> {
    fn is_simple(&self) -> bool {
        if self.is_empty() { return true; }
        if self.len() == 1 {
            let item = &self[0];
            return item.is_atomic() || (item.is_simple() && item.has_delimiters());
        }
        false
    }
}
```

#### `has_delimiters()` - Formats With Brackets/Braces/Parens?

Indicates types that format with their own delimiters.

**True for:**
- `Vec<T>` → brackets `[ ]`
- `Trivia` → brackets `[ ]`
- `TrailingComment` → parens `( )`
- `Ann<T>` → braces `{ }`
- Tuples → parens `( )`

**False for:**
- Primitives
- ADT variants (they use `format_constructor!`)
- Enums

**Purpose:**
- In `sub_expr()`: Complex types with delimiters don't need extra parens
- In `format_delimited_value()`: Control newline placement

#### `is_empty()` - Logically Empty?

```rust
impl<T> PrettySimple for Vec<T> {
    fn is_empty(&self) -> bool {
        <Vec<T>>::is_empty(self)
    }
}
```

Used to distinguish `[]` from `[...]` for formatting decisions.

#### `is_atomic()` - Single Element In Parsed Form?

**NEW ABSTRACTION** - Critical for `Vec::is_simple()` logic.

In Haskell, when `show` produces a string, pretty-simple parses it:
- `"EmptyLine"` → `[Other "EmptyLine"]` → **1 element**
- `"TextPart \"hello\""` → `[Other "TextPart ", StringLit "hello"]` → **2 elements**

**True for:**
- Primitives: `String`, `usize`, `bool` (parse as single StringLit/NumberLit)
- Nullary constructors: `EmptyLine` (parse as single Other)

**False for:**
- Constructor applications: `TextPart("hello")`, `LineComment("text")`
- Delimited types: `Vec`, `Ann`

**Why it matters:**

For `Vec::is_simple()` to work correctly:
```rust
// [EmptyLine] in Haskell parses to:
Brackets (CommaSeparated [[Other "EmptyLine"]])
// Row has 1 element → matches [[e]] → simple if e is simple ✓

// [TextPart "hello"] in Haskell parses to:
Brackets (CommaSeparated [[Other "TextPart ", StringLit "hello"]])
// Row has 2 elements → does NOT match [[e]] → NOT simple ✗

// Our Rust check:
if self.len() == 1 {
    let item = &self[0];
    // Simple if: atomic (1 parsed element) OR simple delimited type
    return item.is_atomic() || (item.is_simple() && item.has_delimiters());
}
```

## Formatting Helpers

### `sub_expr()` - Format Constructor Arguments

From pretty-simple's `subExpr` - handles spacing for constructor arguments.

```rust
fn sub_expr<T: PrettySimple, W: Writer>(w: &mut W, arg: &T) {
    if arg.is_simple() {
        w.write_plain(" ");
        arg.format(w);
    } else if arg.has_delimiters() {
        w.newline();
        arg.format(w);
    } else {
        w.newline();
        w.with_color(|w| {
            w.write_colored("(", color);
            w.with_depth(|w| {
                w.write_plain(" ");
                arg.format(w);
                w.newline();
            });
            w.write_colored(")", color);
        });
    }
}
```

**Examples:**
```
Constructor "simple"      → Constructor "simple"
Constructor [complex]     → Constructor
                              [complex]
Constructor Complex       → Constructor
                              ( Complex )
```

### `format_delimited_value()` - Format List/Record Elements

Unified helper for elements in lists and record fields.

```rust
fn format_delimited_value<T: PrettySimple, W: Writer>(w: &mut W, value: &T) {
    if value.has_delimiters() && !value.is_empty() && !value.is_simple() {
        w.newline();
        value.format(w);
    } else {
        w.write_plain(" ");
        value.format(w);
    }
}
```

**Logic:**
- Non-empty, complex delimited values → newline before
- Everything else (simple, non-delimited, empty) → space before

**Examples:**
```
{ field = "simple" }             → { field = "simple" }
{ field = [] }                   → { field = [] }
{ field = [EmptyLine] }          → { field = [EmptyLine] }
{ field = [complex, items] }     → { field =
                                       [complex, items]
                                   }
```

## Vec Formatting - The Core Algorithm

Based on Haskell's `list` function (Printer.hs:247-261):

```haskell
list open close (CommaSeparated xss) =
  enclose (annotate Open open) (annotate Close close) $ case xss of
    [] -> mempty
    [xs] | all isSimple xs -> space <> hcat (map prettyExpr xs) <> space
    _ -> concatWith lineAndCommaSep ...
```

### Rust Implementation

```rust
impl<T: PrettySimple> PrettySimple for Vec<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        if self.is_empty() {
            w.with_color(|w| {
                let color = w.current_color();
                w.write_colored("[", color);
                w.write_colored("]", color);
            });
            return;
        }

        w.with_color(|w| {
            let color = w.current_color();
            w.with_depth(|w| {
                // Single element with all simple → inline
                if self.len() == 1 && self[0].is_simple() {
                    w.write_colored("[", color);
                    w.write_plain(" ");
                    self[0].format(w);
                    w.write_plain(" ");
                    w.write_colored("]", color);
                } else {
                    // Multiple elements → multiline comma-first
                    w.write_colored("[", color);
                    for (i, item) in self.iter().enumerate() {
                        if i > 0 {
                            w.newline();
                            w.write_colored(",", color);
                        }
                        format_delimited_value(w, item);
                    }
                    w.newline();
                    w.write_colored("]", color);
                }
            });
        });
    }
}
```

### Key Insight: Single Row vs Multiple Rows

In Haskell's `CommaSeparated [[Expr]]`, each inner list is a "row":
- `["a", "b", "c"]` → `CommaSeparated [["a"], ["b"], ["c"]]` → **3 rows**
- Check `[xs]` only matches when there's **exactly 1 row**

In Rust:
- `vec!["a", "b", "c"]` has 3 elements → **3 rows** in Haskell → multiline
- `vec![EmptyLine]` has 1 element → **1 row** in Haskell → can be inline if simple

## Macros

### `format_constructor!` - Constructor Applications

```rust
format_constructor!(w, "Constructor", [arg1, arg2, arg3])
```

Expands to:
```rust
w.write_plain("Constructor");
sub_expr(w, arg1);
sub_expr(w, arg2);
sub_expr(w, arg3);
```

### `format_record!` - Record Types

```rust
format_record!(w, [
    ("field1", &value1),
    ("field2", &value2),
])
```

Expands to comma-separated record with braces:
```
{
  field1 = value1
, field2 = value2
}
```

### `format_enum!` - Enum Match Arms

```rust
format_enum!(self, w, {
    Variant1(field) => [field],
    Variant2(a, b) => [a, b],
});
```

Generates match arms that call `format_constructor!`.

## Special Cases

### `Trivia` - Mostly Generic

`Trivia` is a newtype wrapper around `Vec<Trivium>` with one special case:

```rust
impl PrettySimple for Trivia {
    fn format<W: Writer>(&self, w: &mut W) {
        // Special case: LanguageAnnotation renders without brackets
        if self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)) {
            self.0[0].format(w);
            return;
        }

        // Otherwise delegate to Vec
        self.0.format(w);
    }

    fn is_simple(&self) -> bool {
        if self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)) {
            return true;
        }
        self.0.is_simple()
    }
}
```

## The Haskell Algorithm Deep Dive

### How Pretty-Simple Works

1. **Haskell generates Show output:** `show (LineComment "text")` → `"LineComment \"text\""`
2. **Pretty-simple parses the string:**
   ```haskell
   ExprParser.parseExpr "LineComment \"text\""
   → [Other "LineComment ", StringLit "text"]
   ```
3. **Formats the parsed Expr:** Uses `isSimple` on individual elements

### Key Insight: No Unpacking

The parser doesn't create `Parens` for constructor applications unless it sees actual parens in the string. Constructor names become `Other`, arguments become separate elements.

### The `isSimple` Function (Printer.hs:264-274)

```haskell
isSimple :: Expr -> Bool
isSimple = \case
  Brackets (CommaSeparated xs) -> isListSimple xs
  Braces (CommaSeparated xs) -> isListSimple xs
  Parens (CommaSeparated xs) -> isListSimple xs
  _ -> True  -- Other, StringLit, NumberLit, CharLit are always simple!
  where
    isListSimple = \case
      [[e]] -> isSimple e && case e of Other s -> not $ any isSpace s ; _ -> True
      _:_ -> False
      [] -> True
```

**Key points:**
- Non-delimited types (Other/StringLit/NumberLit) are **always simple**
- Delimited types are simple only if they contain `[[e]]` (single row, single simple element)

### Our Rust Approximation

Since we don't have the parsing step, we approximate:

1. **`is_simple()`:** Can this render inline in a row? (like Haskell's `isSimple` on individual elements)
2. **`is_atomic()`:** Would this parse as a single element? (determines structural simplicity)
3. **`Vec::is_simple()`:** Single element that's atomic OR simple-delimited (approximates `[[e]]` check)

## Test Results

✅ **79/79 tests passing**

All AST format tests produce output identical to nixfmt Haskell's `--ast` mode.

## Summary

The final design uses three key abstractions:
- **`is_simple()`** - can render inline (element-level simplicity)
- **`is_atomic()`** - single parsed element (structural atomicity)
- **`has_delimiters()`** - formats with brackets/braces/parens

This cleanly separates concerns and allows `Vec` formatting to correctly distinguish between:
- `[EmptyLine]` → atomic element → inline
- `[TextPart "x"]` → non-atomic constructor app → multiline when nested
- `[[]]` → simple delimited element → inline

No custom special cases needed beyond `Trivia`'s `LanguageAnnotation` handling.
