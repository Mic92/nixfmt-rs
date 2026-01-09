# Error Enhancement Plan

## Status

**Overall Progress:** Phase 3 Complete (Basic Formatting) ✅

- ✅ **Phase 1: Core Types Rewrite** - COMPLETED
- ✅ **Phase 2: Parser Integration** - COMPLETED
- ✅ **Phase 3: Basic Formatting** - COMPLETED
- ⏳ **Phase 4: Beautiful Formatting** - Pending
- ⏳ **Phase 5: Enhanced Error Messages** - Pending
- ⏳ **Phase 6: Multiple Error Collection** - Pending

**Current State:** Basic error formatting is now functional! Errors are displayed with source snippets showing 1-3 lines of context, accurate pointers using `^` characters, and helpful hints. The ErrorContext and ErrorFormatter are fully implemented and integrated with the error_visualization example. The output now shows file:line:col headers, proper line number alignment, and context lines before/after the error. All 238 tests pass. Next step is to enhance the formatter with box-drawing characters, colors, and multi-line span support (Phase 4).

**Key Architectural Decision:** The `Writer` trait now provides `source()` method to access the original source text. This allows computing line numbers from byte offsets during pretty-printing without passing source as a parameter through every function. This keeps the API clean and maintains Haskell nixfmt compatibility (outputting `sourceLine = Pos N` where N is computed on-the-fly).

## Design Philosophy

**Clean Break from Old API:**
- No backward compatibility constraints
- Design for the best possible error experience
- Simple, clean types without legacy baggage

**Separation of Concerns:**
- Errors store **byte offsets** and **messages** only (lightweight, no lifetimes)
- Display context (filename, source, line mappings) is **provided at render time**
- Error formatting is a separate concern from error creation

**Benefits:**
- No source text ownership/lifetime issues
- Errors can be collected, sorted, and deduplicated without carrying source
- Single source of truth for source text
- Easy to test error creation separately from formatting

## Visualization

**See it in action:**
```bash
cargo run --example error_visualization
```

This example demonstrates 14 common error cases, showing:
- Current basic error output
- Future enhanced error output (target goal)
- Source code context with line numbers
- Helpful explanations and suggestions

The example serves as:
- Visual reference for what we're building
- Test bed for new error formatting
- Documentation of common error scenarios
- Motivation for why better errors matter

## Current State Analysis

### Existing Error Structure (`src/error.rs`)

```rust
pub struct ParseError {
    pub pos: Pos,              // Line number only: Pos(usize)
    pub message: String,       // Generic error message
    pub context: Vec<String>,  // Stack of parsing contexts
}
```

**Limitations:**
- Only tracks line number, no column or byte offset
- No span information (start/end range)
- Generic string messages (not structured)
- No error codes for documentation
- Basic display format with no source snippets
- No support for suggestions or "did you mean"
- Can't highlight multiple locations

## Proposed New Architecture

### Core Error Type (Lightweight)

```rust
/// A parse error with span and message
pub struct ParseError {
    /// Primary error location (byte offsets into source)
    pub span: Span,

    /// Error kind with structured data
    pub kind: ErrorKind,

    /// Additional related spans with labels
    pub labels: Vec<Label>,
}

/// A byte offset range in the source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
}

/// Labeled related location
pub struct Label {
    pub span: Span,
    pub message: String,
    pub style: LabelStyle,
}

pub enum LabelStyle {
    Primary,   // Main error location
    Secondary, // Related/context location
    Note,      // Informational
}
```

### Structured Error Kinds

```rust
pub enum ErrorKind {
    /// Unexpected token: expected X, found Y
    UnexpectedToken {
        expected: Vec<String>,  // ["';'", "'}'"]
        found: String,          // "'in'"
    },

    /// Unclosed delimiter (brace, bracket, paren, string)
    UnclosedDelimiter {
        delimiter: char,        // '{', '[', '(', '"', '\''
        opening_span: Span,     // where it was opened
    },

    /// Missing required token
    MissingToken {
        token: String,          // "';'"
        after: String,          // "attribute definition"
    },

    /// Unknown identifier with suggestions
    UnknownIdentifier {
        name: String,
        suggestions: Vec<String>,  // Levenshtein distance matches
    },

    /// Invalid syntax pattern
    InvalidSyntax {
        description: String,
        hint: Option<String>,
    },

    /// Chained comparison operators (1 < 2 < 3)
    ChainedComparison {
        first_op: String,
        second_op: String,
    },

    /// Generic message (for gradual migration)
    Message(String),
}
```

### Display Context (Provided at Render Time)

```rust
/// Context needed to format errors with source snippets
pub struct ErrorContext<'a> {
    /// The source code
    pub source: &'a str,

    /// Optional filename for display
    pub filename: Option<&'a str>,

    /// Byte offsets of line starts (computed once, shared)
    pub line_starts: &'a [usize],
}

impl<'a> ErrorContext<'a> {
    /// Create context from source
    pub fn new(source: &'a str, filename: Option<&'a str>) -> Self {
        let line_starts = compute_line_starts(source);
        Self {
            source,
            filename,
            line_starts: Box::leak(line_starts.into_boxed_slice()),
        }
    }

    /// Convert byte offset to (line, column)
    pub fn position(&self, offset: usize) -> Position {
        // Binary search in line_starts
    }

    /// Extract source lines for a span
    pub fn snippet(&self, span: Span) -> &str {
        &self.source[span.start..span.end]
    }

    /// Get line containing offset
    pub fn line_at(&self, offset: usize) -> (usize, &str) {
        // returns (line_number, line_text)
    }
}

/// Computed position (line, column)
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub line: usize,    // 1-based
    pub column: usize,  // 0-based
}
```

### Error Formatter

```rust
/// Rich error formatter with source snippets
pub struct ErrorFormatter<'a> {
    context: &'a ErrorContext<'a>,
    use_color: bool,
}

impl<'a> ErrorFormatter<'a> {
    pub fn new(context: &'a ErrorContext<'a>) -> Self {
        Self {
            context,
            use_color: false,  // TODO: detect terminal support
        }
    }

    /// Format a single error
    pub fn format(&self, error: &ParseError) -> String {
        let mut output = String::new();

        // Error header
        self.format_header(error, &mut output);

        // Source snippet with pointer
        self.format_snippet(error, &mut output);

        // Notes and hints
        self.format_notes(error, &mut output);

        output
    }

    fn format_header(&self, error: &ParseError, out: &mut String) {
        let pos = self.context.position(error.span.start);
        write!(out, "Error");

        // Show error code if available
        if let Some(code) = error.code() {
            write!(out, "[{}]", code);
        }

        // Main message
        writeln!(out, ": {}", error.message());

        // File location
        if let Some(filename) = self.context.filename {
            writeln!(out, "  ┌─ {}:{}:{}", filename, pos.line, pos.column);
        } else {
            writeln!(out, "  ┌─ line {}:{}", pos.line, pos.column);
        }
    }

    fn format_snippet(&self, error: &ParseError, out: &mut String) {
        // Show 2-3 lines of context with line numbers
        // Point to error location with ^^^^^
        // Show secondary labels with -----
    }

    fn format_notes(&self, error: &ParseError, out: &mut String) {
        // Show hints, suggestions, examples
        match &error.kind {
            ErrorKind::UnknownIdentifier { suggestions, .. } => {
                if !suggestions.is_empty() {
                    writeln!(out, "  = help: did you mean '{}'?", suggestions[0]);
                }
            }
            _ => {}
        }
    }
}
```

## Implementation Plan

### Phase 1: Core Types Rewrite ✅ COMPLETED

**Goal:** Replace Pos with Span, rewrite error types from scratch

**Changes:**
- [x] Remove `Pos` type from `src/types.rs`
- [x] Add `Span` type to `src/types.rs`
- [x] Update `Ann<T>` to use `Span` instead of `Pos`
- [x] Rewrite `src/error.rs` with new types (ParseError, ErrorKind, etc.)
- [x] Add `src/error/context.rs` for ErrorContext
- [x] Add `src/error/format.rs` for ErrorFormatter
- [x] Update lexer to track token start positions
- [x] Update all `Ann<Token>` creation to use spans
- [x] Update parser to use new error types (all error sites migrated to Span)
- [x] Update PrettySimple trait to compute line numbers from byte offsets
- [x] All tests passing

**Files Modified:**
- `src/types.rs` - Replaced Pos with Span, updated Ann<T>
- `src/error.rs` - Complete rewrite with ErrorKind, Label types
- `src/error/context.rs` - New file for ErrorContext (line/column computation)
- `src/error/format.rs` - New file for ErrorFormatter (basic structure)
- `src/lexer.rs` - Now tracks token start/end for accurate spans
- `src/parser.rs` - All error sites updated to use Span
- `src/pretty_simple.rs` - Writer trait now provides source(), SpanWrapper computes line numbers
- `src/colored_writer.rs` - Stores source reference for line number computation
- `tests/common/ast_format.rs` - Updated to pass source to ColoredWriter

**Key Implementation Details:**
- Span stores byte offsets (start, end) instead of line numbers
- Line numbers are computed on-demand from byte offsets using ErrorContext
- Pretty printer maintains Haskell compatibility by outputting "sourceLine = Pos N" where N is computed from span
- Writer trait provides access to source text for line number computation
- All existing tests pass with new architecture

---

### Phase 2: Parser Integration ✅ COMPLETED

**Goal:** Update all parser error sites to use new error types

**Changes:**
- [x] Update all `ParseError::new()` calls to use new structure
- [x] Track opening delimiters for pairing errors (strings, indented strings, paths)
- [x] Add helpful hints to InvalidSyntax errors
- [x] Update tests to match new error messages

**Completed Error Updates:**

1. **Unexpected token errors** (10+ sites) - Now use `ErrorKind::UnexpectedToken` with:
   - Lexer: '&', '|', single quote, '$', unexpected characters
   - Parser: term parsing, selector parsing, EOF checking, parameter parsing, URI scheme
   - Specify expected tokens and what was found

2. **Unclosed delimiter errors** (3 sites) - Now use `ErrorKind::UnclosedDelimiter` with:
   - Simple strings (")
   - Indented strings ('')
   - Angle bracket paths (<)
   - Track opening span for pairing

3. **Missing token errors** (1 site) - Now use `ErrorKind::MissingToken`:
   - Missing ':' after URI scheme

4. **Invalid syntax errors** (8 sites) - Now use `ErrorKind::InvalidSyntax` with hints:
   - @ outside lambda parameters
   - Set parameter syntax issues
   - Parameter attribute errors
   - Path trailing slash
   - Invalid path characters

5. **Chained comparison errors** (1 site) - Now use `ErrorKind::ChainedComparison`:
   - Track both operator names
   - Prevent chaining at same precedence level

**Files Modified:**
- `src/parser.rs` - Updated ~25 error sites with structured ErrorKind variants
- `src/lexer.rs` - Updated ~7 error sites with structured ErrorKind variants
- `tests/coverage_test.rs` - Updated 3 test assertions to match new error messages

**All Tests Passing:** ✅ 94 tests pass

**Deferred to Later Phases:**
- Secondary labels (labels field exists but not yet populated) - Phase 4
- Multiple error collection (still single error return) - Phase 6

---

### Phase 3: Basic Formatting ✅ COMPLETED

**Goal:** Display errors with source snippets and pointers

**Changes:**
- [x] Implement `ErrorContext::new()` and position conversion
- [x] Implement `ErrorFormatter` with basic snippet display
- [x] Show file:line:col header
- [x] Show 1-3 lines of context with line numbers
- [x] Point to error with `^` characters
- [x] Keep it simple - fancy features come later
- [x] Update `examples/error_visualization.rs` to show real output

**Implementation Details:**
- `ErrorContext` fully functional with line/column computation from byte offsets
- `ErrorFormatter` displays errors with proper source snippets
- Shows 1 line before error, error line, and 1 line after (when available)
- Empty separator line after header for visual clarity
- Accurate pointer calculation handling multi-byte UTF-8 characters
- Line numbers properly aligned with padding
- Error codes displayed in header (e.g., Error[E001])
- Helpful hints and notes displayed for each error kind
- All 238 tests passing

**Files Modified:**
- `src/error/format.rs` - Enhanced `format_snippet()` to show context lines
- `examples/error_visualization.rs` - Updated to use ErrorFormatter instead of basic Display

---

### Phase 4: Beautiful Formatting

**Goal:** Add box-drawing, colors, secondary labels

**Changes:**
- [ ] Add box-drawing characters (┌─ │ etc.)
- [ ] Support secondary labels with different underlines
- [ ] Add color support (feature-gated)
- [ ] Format notes and help messages nicely
- [ ] Handle multi-line spans
- [ ] Handle long lines with truncation
- [ ] Compare output against `examples/error_visualization.rs` mock output

### Phase 5: Enhanced Error Messages

**Goal:** Add suggestions, hints, and "did you mean"

**Changes:**
- [ ] Add Levenshtein distance for typo suggestions
- [ ] Add common mistake detection
- [ ] Add fix-it suggestions (e.g., "add semicolon here")
- [ ] Add examples of correct syntax
- [ ] Generate helpful notes based on error kind

**Example improvements:**

**Before:**
```
Parse error at line 42: expected ';'
```

**After:**
```
Error: Missing semicolon after attribute definition
  ┌─ config.nix:42:56
  │
42│   services.nginx.enable = true
  │                               ^ expected ';' here
43│   networking.firewall.enable = false;
  │   ─────────────────────────── next attribute starts here
  │
  = note: attribute definitions in sets must be terminated with ';'
  = help: add a semicolon: `services.nginx.enable = true;`
```

### Phase 6: Multiple Error Collection

**Goal:** Show multiple errors in one pass

**Changes:**
- [ ] Change parser to collect errors instead of returning immediately
- [ ] Add error recovery at synchronization points
- [ ] Deduplicate similar/cascading errors
- [ ] Sort errors by location before display
- [ ] Update public API to return `Vec<ParseError>`

**API:**
```rust
pub fn parse(source: &str) -> Result<File, Vec<ParseError>>
```

**Note:** This is the last phase and can be deferred. Single error is fine initially.

## Detailed Requirements

### What Must Be Tracked During Parsing

**In Lexer:**
- ✓ Current byte offset (`pos` field - already tracked)
- ✓ Line number (already tracked)
- ✓ Column (already tracked, but could be recomputed)
- ✗ Line starts (compute once at start)

**In Parser:**
- ✗ Start offset of current token
- ✗ End offset of current token
- ✗ Opening delimiter positions (for matching pairs)

**In Errors:**
- ✗ Error span (byte start/end)
- ✗ Structured error kind
- ✗ Related spans (secondary labels)

### Span Creation Patterns

```rust
// Single token span
let span = Span::new(token.start, token.end);

// Multi-token span
let span = Span::new(first_token.start, last_token.end);

// From current parser position
let span = Span::point(self.lexer.pos);  // zero-length

// Extending a span
let span = span.extend_to(self.lexer.pos);
```

### Ann<T> Enhancement

```rust
// Current
pub struct Ann<T> {
    pub pre_trivia: Trivia,
    pub source_line: Pos,     // Just line number
    pub value: T,
    pub trail_comment: Option<TrailingComment>,
}

// New (clean break)
pub struct Ann<T> {
    pub pre_trivia: Trivia,
    pub span: Span,           // Byte range (replaces source_line)
    pub value: T,
    pub trail_comment: Option<TrailingComment>,
}
```

**Note:** `Pos` type can be removed entirely, replaced by `Span` everywhere.

## Usage Example

```rust
// Parse
let source = std::fs::read_to_string("config.nix")?;
let result = parse(&source);

// On error, format with context
if let Err(error) = result {
    let context = ErrorContext::new(&source, Some("config.nix"));
    let formatter = ErrorFormatter::new(&context);
    eprintln!("{}", formatter.format(&error));
}

// Or multiple errors
let (ast, errors) = parse_with_recovery(&source);
if !errors.is_empty() {
    let context = ErrorContext::new(&source, Some("config.nix"));
    let formatter = ErrorFormatter::new(&context);

    for error in &errors {
        eprintln!("{}", formatter.format(error));
        eprintln!(); // blank line between errors
    }
}
```

## Error Recovery Strategy

**Synchronization points** (safe places to resume parsing):
- After semicolon (`;`)
- After closing brace (`}`)
- At next keyword (`let`, `if`, `with`, etc.)
- After closing bracket (`]`)

**Recovery actions:**
1. Report error with current context
2. Skip tokens until synchronization point
3. Resume parsing from synchronized state
4. Continue to find more errors

**Avoid cascading errors:**
- If error span overlaps previous error, skip it
- If in "panic mode", suppress similar errors
- Track whether we're in error recovery

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_missing_semicolon_error() {
    let source = "{ a = 1 b = 2; }";
    let error = parse(source).unwrap_err();

    assert!(matches!(error.kind, ErrorKind::MissingToken { .. }));
    assert_eq!(error.span.start, 7); // after "1"
}

#[test]
fn test_error_formatting() {
    let source = "{ a = 1 }";
    let error = parse(source).unwrap_err();
    let context = ErrorContext::new(source, None);
    let formatted = ErrorFormatter::new(&context).format(&error);

    assert!(formatted.contains("^")); // has pointer
    assert!(formatted.contains("a = 1")); // has snippet
}
```

### Snapshot Tests
- Store expected error output in `tests/errors/`
- Compare actual formatted errors with snapshots
- Update snapshots when improving messages


### Real-World Tests
- Collect common error patterns from nixpkgs
- Test with intentionally broken Nix files
- Validate suggestions are helpful

### Example Program

The `examples/error_visualization.rs` program provides:
- Side-by-side comparison of current vs. future error output
- 14 common error scenarios with mock future output
- Visual reference for implementation goals
- Easy way to demo improvements as they're implemented

**Run it:**
```bash
cargo run --example error_visualization
```

As each error enhancement is implemented, update the example to show real output instead of mock output.

## Error Message Quality Guidelines

**Good error messages:**
- ✓ Say what's wrong specifically ("missing semicolon" not "parse error")
- ✓ Point to exact location
- ✓ Explain why it's wrong
- ✓ Suggest how to fix it
- ✓ Show examples when helpful
- ✓ Use clear, non-technical language

**Bad error messages:**
- ✗ "Parse error" (too generic)
- ✗ "Unexpected token" (without saying what was expected)
- ✗ Technical jargon without explanation
- ✗ Multiple ways to say the same thing
- ✗ Blaming the user ("you forgot...")

## Migration Path

Since we're making a clean break, the migration is straightforward:

### Step 1: Replace Pos with Span everywhere
```rust
// Remove
pub struct Pos(pub usize);

// Replace with
pub struct Span { pub start: usize, pub end: usize }

// Update all uses in types.rs
pub struct Ann<T> {
    pub span: Span,  // was: source_line: Pos
    // ...
}
```

### Step 2: Rewrite error.rs completely
```rust
// Delete old ParseError
// Implement new error types from scratch
pub struct ParseError { ... }
pub enum ErrorKind { ... }
pub struct ErrorContext { ... }
pub struct ErrorFormatter { ... }
```

### Step 3: Update lexer to track spans
```rust
// Lexer already tracks byte offset in `pos` field
// Just need to track start of token for span creation
impl Lexer {
    fn token_span(&self, start: usize) -> Span {
        Span { start, end: self.pos }
    }
}
```

### Step 4: Update parser error sites
```rust
// Old
return Err(ParseError::new(self.current.source_line, "expected ';'"));

// New
return Err(ParseError {
    span: self.current.span,
    kind: ErrorKind::MissingToken {
        token: ";".into(),
        after: "attribute definition".into(),
    },
    labels: vec![],
});
```

### Step 5: Update public API
```rust
// lib.rs
pub fn parse(source: &str) -> Result<File, Vec<ParseError>>

// Callers update to:
match parse(source) {
    Ok(ast) => { /* use ast */ }
    Err(errors) => {
        let ctx = ErrorContext::new(source, Some("file.nix"));
        let fmt = ErrorFormatter::new(&ctx);
        for error in errors {
            eprintln!("{}\n", fmt.format(&error));
        }
    }
}
```

## Open Questions

1. **Color support:** Detect terminal capabilities or use feature flag?
   - **Decision:** Start without color, add as feature flag later

2. **Error limits:** Stop after N errors?
   - **Decision:** Default limit of 10, configurable

3. **Span for every AST node:** Worth the memory cost?
   - **Decision:** Start with just errors, expand if needed

4. **Unicode handling:** How to count columns with emoji/wide chars?
   - **Decision:** Count grapheme clusters, not bytes or chars

5. **Long lines:** How to display errors in 200+ character lines?
   - **Decision:** Truncate with "..." but keep error region

## Success Criteria

Errors should:
- [ ] Show exact location (line:column)
- [ ] Display source snippet (2-3 lines)
- [ ] Point to exact error with `^`
- [ ] Use specific messages ("missing semicolon" not "parse error")
- [ ] Suggest fixes when possible
- [ ] Show "did you mean" for typos
- [ ] Format with box-drawing characters
- [ ] Work without source (fallback to basic format)
- [ ] Be comparable to Rust/TypeScript/Elm error quality
- [ ] Have documentation for error codes
- [ ] Match or exceed the quality shown in `examples/error_visualization.rs`

## References

**Inspiration from:**
- Rust compiler diagnostics (gold standard)
- Elm compiler (beginner-friendly)
- TypeScript compiler (great suggestions)
- Clang (clear formatting)
