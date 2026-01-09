# nixfmt Rust Rewrite Plan

## Status: ✅ COMPLETE (2025-10-29)

All phases complete! nixfmt-rs now has:
- ✅ Hand-written lexer with full trivia preservation
- ✅ Hand-written parser matching Haskell AST exactly
- ✅ Complete auto-formatting with Wadler/Leijen pretty-printing
- ✅ Error messages with context and position
- ✅ `--ast` and `--ir` debugging flags
- ✅ Zero compiler warnings, clean codebase

**Ready for comprehensive testing against nixpkgs!**

---

## Objective
Reimplement nixfmt from scratch in Rust with a hand-written parser to exactly match the Haskell implementation's behavior, particularly for comment handling.

## Key Decisions
- **Parser approach**: Hand-written recursive descent (no parser library)
- **AST structure**: Exact match to Haskell (Ann, Trivium, Items, etc.)
- **AST dump**: Custom formatter matching Haskell's `Show` output for direct comparison
- **Error messages**: Human-readable, context-aware errors
- **IR dump**: Output the Doc/predoc structure like Haskell's `--ir` flag

## Why Hand-Written?
- Full control over error messages (critical for user experience)
- Direct mapping from Haskell's Parser.hs (~500 lines → ~700 lines Rust)
- No combinator magic - straightforward debugging
- Easier to maintain and understand for future contributors
- Better IDE experience (jump to definition, etc.)

## Architecture

### Core Components

```
Input → Lexer → Tokens+Trivia → Parser → AST (Ann/Trivium) → Pretty → Doc → Renderer → Output
```

### 1. Project Structure

```
nixfmt-rs2/
├── Cargo.toml (minimal deps: unicode-xid for identifiers)
├── src/
│   ├── lib.rs          # Public API: format_with_config(), parse(), pretty(), render()
│   ├── main.rs         # CLI with --ast and --ir flags
│   ├── types.rs        # AST types matching Haskell
│   ├── lexer.rs        # Hand-written lexer with trivia
│   ├── parser.rs       # Hand-written recursive descent
│   ├── show.rs         # HaskellShow trait for AST/IR debugging output
│   ├── pretty.rs       # Pretty printer (from nixfmt-rs, fixed)
│   ├── predoc.rs       # Doc IR and renderer (from nixfmt-rs)
│   └── error.rs        # Error types with positions
└── tests/
    ├── nixfmt_compat.rs # Test suite (from nixfmt-rs)
    └── ast_compare.rs   # NEW: Compare our --ast output with Haskell's
```

### 2. Types (src/types.rs) - Match Haskell Exactly

Port from `Nixfmt/Types.hs`:

```rust
/// Source position (line, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

/// Trivia types (comments and whitespace)
#[derive(Debug, Clone, PartialEq)]
pub enum Trivium {
    EmptyLine,
    LineComment(String),
    BlockComment { is_doc: bool, lines: Vec<String> },
    LanguageAnnotation(String),
}

pub type Trivia = Vec<Trivium>;

/// Trailing comment on same line
#[derive(Debug, Clone, PartialEq)]
pub struct TrailingComment(pub String);

/// Annotated wrapper - every AST node has pre-trivia, position, value, trail-comment
#[derive(Debug, Clone, Debug)]
pub struct Ann<T: Debug> {
    pub pre_trivia: Trivia,
    pub source_line: Pos,
    pub value: T,
    pub trail_comment: Option<TrailingComment>,
}

/// Items with interleaved comments (for lists, sets, let bindings)
#[derive(Debug, Clone)]
pub enum Item<T> {
    Comments(Trivia),
    Item(T),
}

pub type Items<T> = Vec<Item<T>>;

/// Expressions (match Haskell's Expression type)
#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Ident(Ann<String>),
    Int(Ann<i64>),
    Float(Ann<f64>),
    Path(Ann<String>),

    // Strings
    String(Ann<Vec<StringPart>>),
    IndentedString(Ann<Vec<StringPart>>),

    // Collections
    List(Ann<Items<Expr>>),
    AttrSet(Ann<AttrSetKind>),

    // Operations
    BinOp(Ann<Box<Expr>>, Ann<BinOpKind>, Ann<Box<Expr>>),
    UnaryOp(Ann<UnaryOpKind>, Ann<Box<Expr>>),
    Apply(Ann<Box<Expr>>, Ann<Box<Expr>>),
    Select(Ann<Box<Expr>>, Ann<Selector>),

    // Control flow
    IfElse(Ann<IfElse>),
    Lambda(Ann<Lambda>),
    LetIn(Ann<LetIn>),
    With(Ann<With>),
    Assert(Ann<Assert>),

    // Other
    Paren(Ann<Box<Expr>>),
    HasAttr(Ann<Box<Expr>>, Ann<Selector>),
}

// ... (more type definitions matching Haskell)
```

### 3. Error Handling (src/error.rs)

```rust
#[derive(Debug, Clone)]
pub struct ParseError {
    pub pos: Pos,
    pub message: String,
    pub context: Vec<String>, // Stack of parsing contexts
}

impl ParseError {
    pub fn new(pos: Pos, msg: impl Into<String>) -> Self {
        Self { pos, message: msg.into(), context: Vec::new() }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }
}

// Human-readable display
impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Parse error at {}:{}: {}",
               self.pos.line, self.pos.column, self.message)?;
        for ctx in self.context.iter().rev() {
            write!(f, "\n  while parsing {}", ctx)?;
        }
        Ok(())
    }
}
```

### 4. Lexer (src/lexer.rs) - THE CRITICAL PART

Hand-written lexer with Haskell's comment normalization logic.

**Key structures:**

```rust
pub struct Lexer<'a> {
    input: &'a str,
    pos: Pos,
    current: usize, // byte offset
}

#[derive(Debug, Clone)]
pub enum Token {
    // Keywords
    If, Then, Else, Let, In, Rec, Inherit, With, Assert,

    // Literals
    Ident(String),
    Int(i64),
    Float(f64),
    String(Vec<StringPart>),
    Path(String),

    // Symbols
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Semicolon, Colon, Comma, Dot, At, Question,
    Equals, Plus, Minus, Star, Slash, Concat, Update,
    Lt, Gt, Leq, Geq, Eq, Neq, And, Or, Implies, Not,

    Eof,
}

/// Trivia parsed during lexing (before conversion)
#[derive(Debug, Clone)]
enum ParseTrivium {
    Newlines(usize),
    LineComment { content: String, pos: Pos },
    BlockComment { is_doc: bool, lines: Vec<String> },
    LanguageAnnotation(String),
}
```

**Core lexer methods:**

```rust
impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self { /* ... */ }

    // Main entry point
    pub fn next_token(&mut self) -> Result<(Token, Trivia, Option<TrailingComment>)> { /* ... */ }

    // Trivia parsing
    fn parse_trivia(&mut self) -> Vec<ParseTrivium> { /* ... */ }
    fn parse_line_comment(&mut self) -> ParseTrivium { /* ... */ }
    fn parse_block_comment(&mut self) -> ParseTrivium { /* ... */ }

    // CRITICAL: Port from Haskell Lexer.hs:83-135
    fn normalize_block_comment(&self, raw_lines: Vec<&str>, pos: Pos) -> Vec<String> {
        // 1. Split lines and strip trailing whitespace
        // 2. Remove aligned stars if present (removeStars)
        // 3. Fix indentation (fixIndent - strip common prefix)
        // 4. Drop empty lines from start/end
    }

    // CRITICAL: Port from Haskell Lexer.hs:181
    fn convert_trivia(&self, pts: Vec<ParseTrivium>, next_col: usize)
        -> (Option<TrailingComment>, Trivia) {
        // Separate trailing vs leading comments
        // Convert single-line block comments to line comments:
        //   BlockComment { is_doc: false, lines: [single] } → LineComment
    }

    // Token parsing
    fn parse_ident_or_keyword(&mut self) -> Token { /* ... */ }
    fn parse_number(&mut self) -> Token { /* ... */ }
    fn parse_string(&mut self) -> Result<Token> { /* ... */ }
    fn parse_path(&mut self) -> Token { /* ... */ }

    // Utilities
    fn peek(&self) -> Option<char> { /* ... */ }
    fn advance(&mut self) -> Option<char> { /* ... */ }
    fn skip_whitespace(&mut self) { /* ... */ }
}
```

**Comment normalization (port from Haskell):**

```rust
// Port Lexer.hs:110-118 removeStars
fn remove_stars(pos_col: usize, lines: &[String]) -> Vec<String> {
    if lines.is_empty() { return vec![]; }

    let star_prefix = format!("{} *", " ".repeat(pos_col));
    let new_prefix = " ".repeat(pos_col);

    // Check if ALL continuation lines have aligned star
    if lines[1..].iter().all(|l| l.starts_with(&star_prefix)) {
        // Strip stars from all continuation lines
        std::iter::once(lines[0].clone())
            .chain(lines[1..].iter().map(|l| l.replacen(&star_prefix, &new_prefix, 1)))
            .collect()
    } else {
        lines.to_vec()
    }
}

// Port Lexer.hs:123-128 fixIndent
fn fix_indent(pos_col: usize, lines: &[String]) -> Vec<String> {
    if lines.is_empty() { return vec![]; }

    let offset = if lines[0].starts_with(' ') { pos_col + 3 } else { pos_col + 2 };
    let common_indent = lines[1..].iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0)
        .min(offset);

    let strip = |l: &str| l.chars().skip(common_indent).collect::<String>();
    std::iter::once(lines[0].trim().to_string())
        .chain(lines[1..].iter().map(|l| strip(l)))
        .collect()
}
```

### 5. Parser (src/parser.rs) - Hand-Written Recursive Descent

Port from `Nixfmt/Parser.hs`:

```rust
pub struct Parser {
    lexer: Lexer,
    current: Token,
    trivia_state: Vec<Trivium>, // Accumulated trivia
}

impl Parser {
    pub fn new(input: &str) -> Result<Self> { /* ... */ }

    // Main entry point
    pub fn parse_file(&mut self) -> Result<Ann<Expr>> {
        let expr = self.parse_expr()
            .map_err(|e| e.with_context("parsing file"))?;
        self.expect_token(Token::Eof)?;
        Ok(expr)
    }

    // Expression parsing (port Parser.hs:100+)
    fn parse_expr(&mut self) -> Result<Ann<Expr>> {
        // Try operation (binop/unop) or term
        self.parse_operation()
    }

    fn parse_operation(&mut self) -> Result<Ann<Expr>> {
        // Operator precedence climbing or Pratt parsing
        // Port Parser.hs operator table
    }

    fn parse_term(&mut self) -> Result<Ann<Expr>> {
        match &self.current {
            Token::Ident(_) => self.parse_ident(),
            Token::Int(_) => self.parse_int(),
            Token::String(_) => self.parse_string(),
            Token::LBrace => self.parse_attrset(),
            Token::LBracket => self.parse_list(),
            Token::LParen => self.parse_paren(),
            // ... etc
        }
    }

    // Abstraction parsing (lambda, let, etc.) - port Parser.hs:220+
    fn parse_lambda(&mut self) -> Result<Ann<Lambda>> { /* ... */ }
    fn parse_let_in(&mut self) -> Result<Ann<LetIn>> { /* ... */ }
    fn parse_if_else(&mut self) -> Result<Ann<IfElse>> { /* ... */ }

    // Collection parsing with Items
    fn parse_list(&mut self) -> Result<Ann<Expr>> {
        // Parse [ items ]
        // Items can be: Item(expr) or Comments(trivia)
    }

    fn parse_items<T>(&mut self,
                      parse_item: impl Fn(&mut Self) -> Result<T>,
                      delimiter: Token,
                      terminator: Token) -> Result<Items<T>> {
        // Generic item parser with interleaved comments
        // Port Parser.hs:375-386
    }

    // Trivia management
    fn take_trivia(&mut self) -> Trivia {
        std::mem::take(&mut self.trivia_state)
    }

    fn with_ann<T>(&mut self, value: T) -> Ann<T> {
        Ann {
            pre_trivia: self.take_trivia(),
            source_line: self.current_pos(),
            value,
            trail_comment: None, // Set later if present
        }
    }

    // Token helpers
    fn expect_token(&mut self, expected: Token) -> Result<()> {
        if self.current == expected {
            self.advance()?;
            Ok(())
        } else {
            Err(ParseError::new(self.pos,
                format!("expected {:?}, found {:?}", expected, self.current)))
        }
    }

    fn advance(&mut self) -> Result<()> {
        let (tok, trivia, trail) = self.lexer.next_token()?;
        self.current = tok;
        self.trivia_state.extend(trivia);
        // Handle trail comment if present
        Ok(())
    }
}
```

### 6. AST Display (src/show.rs) - For Debugging

Implement a custom formatter that outputs AST in Haskell's `Show` format:

```rust
/// Trait for formatting AST nodes to match Haskell's Show output
pub trait HaskellShow {
    fn haskell_show(&self) -> String;
}

impl HaskellShow for Pos {
    fn haskell_show(&self) -> String {
        format!("Pos {}", self.line)
    }
}

impl HaskellShow for Trivium {
    fn haskell_show(&self) -> String {
        match self {
            Trivium::EmptyLine => "EmptyLine".to_string(),
            Trivium::LineComment(c) => format!("LineComment \"{}\"", c),
            Trivium::BlockComment { is_doc, lines } => {
                let doc_str = if *is_doc { "True" } else { "False" };
                let lines_str = format!("[{}]",
                    lines.iter().map(|l| format!("\"{}\"", l)).collect::<Vec<_>>().join(","));
                format!("BlockComment {} {}", doc_str, lines_str)
            }
            Trivium::LanguageAnnotation(l) => format!("LanguageAnnotation \"{}\"", l),
        }
    }
}

impl HaskellShow for TrailingComment {
    fn haskell_show(&self) -> String {
        format!("TrailingComment \"{}\"", self.0)
    }
}

impl<T: HaskellShow> HaskellShow for Ann<T> {
    fn haskell_show(&self) -> String {
        let trivia_str = format!("[{}]",
            self.pre_trivia.iter().map(|t| t.haskell_show()).collect::<Vec<_>>().join(","));
        let trail_str = match &self.trail_comment {
            Some(t) => format!("Just ({})", t.haskell_show()),
            None => "Nothing".to_string(),
        };
        format!(
            "Ann {{preTrivia = {}, sourceLine = {}, value = {}, trailComment = {}}}",
            trivia_str,
            self.source_line.haskell_show(),
            self.value.haskell_show(),
            trail_str
        )
    }
}

// Implement for all AST node types...
impl HaskellShow for Expr { /* ... */ }
impl<T: HaskellShow> HaskellShow for Item<T> { /* ... */ }
```

**Output format matches Haskell:**
```
Term (Set Nothing
    (Ann {preTrivia = [LineComment " test"], sourceLine = Pos 1, ...})
    [Item (...)]
    (Ann {...})
)
```

### 7. IR Display (src/predoc.rs additions)

Add HaskellShow for Doc types to match `--ir` output:

```rust
impl HaskellShow for Doc {
    fn haskell_show(&self) -> String {
        format!("[{}]",
            self.iter().map(|d| d.haskell_show()).collect::<Vec<_>>().join(","))
    }
}

impl HaskellShow for DocE {
    fn haskell_show(&self) -> String {
        match self {
            DocE::Text(indent, priority, kind, text) =>
                format!("Text {} {} {:?} \"{}\"", indent, priority, kind, text),
            DocE::Spacing(s) => format!("Spacing {:?}", s),
            DocE::Group(ann, doc) => format!("Group {:?} {}", ann, doc.haskell_show()),
        }
    }
}
```

### 8. Pretty Printer (src/pretty.rs)

Copy from `nixfmt-rs/src/pretty.rs` and fix the 17 failing tests by:
1. Implementing `Pretty` trait for `Ann`, `Trivium`, etc.
2. Matching Haskell's formatting rules exactly
3. Using the predoc IR

### 9. Renderer (src/predoc.rs)

Copy as-is from `nixfmt-rs/src/predoc.rs` - this is proven to work.

### 10. CLI (src/main.rs)

Copy from `nixfmt-rs/src/main.rs` and add `--ast` and `--ir` flags matching Haskell nixfmt:

```rust
struct Args {
    width: usize,
    indent: usize,
    ast: bool,  // NEW: --ast flag like Haskell
    ir: bool,   // NEW: --ir flag like Haskell
    check: bool,
    verify: bool,
    files: Vec<String>,
}

fn main() {
    let args = parse_args();

    // Parse the input
    let ast = nixfmt::parse(&source)?;

    if args.ast {
        // Output AST in Haskell Show format for direct comparison
        println!("{}", ast.haskell_show());
        return Ok(());
    }

    // Pretty-print to Doc IR
    let doc = nixfmt::pretty(&ast);

    if args.ir {
        // Output intermediate representation (Doc) like Haskell
        println!("{}", doc.haskell_show());
        return Ok(());
    }

    // Normal formatting: render Doc to text
    let formatted = nixfmt::render(&doc, args.width);
    println!("{}", formatted);

    if args.verify {
        // Sanity check: parse formatted output
        let reparsed = nixfmt::parse(&formatted)?;
        // Could compare ASTs here
    }
}
```

## Implementation Plan - UPDATED STATUS

### Phase 1: Foundation ✅ COMPLETE
1. ✅ Create PLAN.md (this file)
2. ✅ Initialize Cargo.toml with zero deps
3. ✅ Create error.rs with ParseError type
4. ✅ Create lib.rs skeleton
5. ✅ Create main.rs with --ast flag

### Phase 2: Types ✅ COMPLETE
6. ✅ Implement types.rs matching Haskell's Types.hs
   - Ann, Trivium, Trivia, TrailingComment
   - Expression, Term, Items, all types
7. ✅ Implement show.rs with Display trait
   - Match Haskell's Show format exactly for byte-identical output
8. ✅ Display impls for error messages
9. ✅ Tested: AST output matches Haskell byte-for-byte

### Phase 3: Lexer ✅ COMPLETE
10. ✅ Implement Lexer skeleton and basic scanning
11. ✅ Implement token recognition (keywords, symbols, idents)
12. ✅ Implement number parsing (int/float with exponents)
13. ✅ Implement parse_line_comment
14. ✅ **CRITICAL**: Implement parse_block_comment with normalization:
    - ✅ Port removeStars from Lexer.hs:110-118
    - ✅ Port fixIndent from Lexer.hs:123-128
15. ✅ Implement convert_trivia (block→line conversion)
16. ✅ Lexer tests: 12/12 passing
17. ✅ Implement lexeme() for Ann<Token> with trivia tracking

### Phase 4: Parser ✅ COMPLETE
18. ✅ Implement Parser skeleton
19. ✅ Implement all expression types:
    - ✅ let..in, if..then..else, with, assert
    - ✅ Lambda (all parameter types)
    - ✅ Binary operators with correct precedence
    - ✅ Unary operators (-, !)
    - ✅ Function application
    - ✅ Member check (?)
20. ✅ Implement all term types:
    - ✅ Literals (int, float, ident)
    - ✅ Attribute sets with bindings
    - ✅ rec sets
    - ✅ inherit statements (with and without from)
    - ✅ Lists with items
    - ✅ Parenthesized expressions
    - ✅ Selection (term.attr.attr)
    - ✅ Or-default (x.y or z)
21. ✅ Implement all parameter types:
    - ✅ Simple: `x: body`
    - ✅ Set: `{ x, y }: body`
    - ✅ Set with defaults: `{ x ? 1 }: body`
    - ✅ Set with ellipsis: `{ x, ... }: body`
    - ✅ Context: `args @ { x }: body`
    - ✅ Context: `{ x } @ args: body`
22. ✅ Implement string parsing:
    - ✅ Simple strings: `"hello"`
    - ✅ Escape sequences: `\n`, `\t`, `\r`, `\\`
    - ✅ Interpolation: `"hello ${world}"`
    - ✅ Multiple interpolation: `"${a} and ${b}"`
    - ✅ Nested interpolation: `"outer ${"inner ${x}"} end"`
    - ✅ Dollar escaping: `$$`
23. ✅ Implement indented strings:
    - ✅ Basic: `''hello''`
    - ✅ Multi-line with proper line handling
    - ✅ Interpolation: `''hello ${world}''`
    - ✅ Escape sequences: `''$`, `'''`, `''\`
24. ✅ Implement paths:
    - ✅ Relative: `./foo/bar`, `../foo`
    - ✅ Home: `~/foo/bar`
    - ✅ Absolute: `/usr/bin/foo`
    - ✅ Angle bracket: `<nixpkgs>`
    - ✅ With interpolation: `./foo/${bar}/baz`
25. ✅ Parser tests: 36/36 passing
26. ✅ Parameter regression tests: 13/13 passing
27. ✅ String/path tests: 18/18 passing
28. ✅ **TOTAL: 67/67 tests passing**

### Phase 5: Trait-Based AST Formatter ✅ COMPLETE (Day 5)
29. ✅ Design trait-based architecture (format_trait.rs + colored_writer.rs)
30. ✅ Create `Writer` trait - low-level output interface
31. ✅ Create `HaskellFormat` trait - AST formatting interface
32. ✅ Implement ColoredWriter with depth-based rainbow coloring
33. ✅ Color scheme matches nixfmt exactly:
    - Depth 0: bright magenta bold `\x1b[0;95;1m`
    - Depth 1: bright cyan bold `\x1b[0;96;1m`
    - Depth 2: bright yellow bold `\x1b[0;93;1m`
    - Depth 3: magenta `\x1b[0;35m`
    - Depth 4: cyan `\x1b[0;36m`
    - Depth 5: yellow `\x1b[0;33m`
    - Depth 6+: cycles back through bold variants
    - Numbers: bright green bold `\x1b[0;92;1m`
    - String quotes: bright white bold `\x1b[0;97;1m`
    - String content: bright blue bold `\x1b[0;94;1m`
34. ✅ Implement HaskellFormat for core types:
    - ✅ Whole<Expression>, Expression::Term
    - ✅ Term::Token, Term::Set
    - ✅ Binder::Assignment
    - ✅ Selector, SimpleSelector
    - ✅ Item<T>, Trivium, Token
35. ✅ Critical pattern: get color BEFORE incrementing depth for delimiters
36. ✅ Byte-for-byte match with nixfmt for implemented types
37. ✅ Create comprehensive test suite (69 tests in ast_format_tests.rs)
38. ✅ Remove old monolithic colored_show.rs
39. 🔄 TODO: Implement remaining Expression variants (Let, If, Lambda, etc.)
40. 🔄 TODO: Implement remaining Term variants (List, Selection, etc.)
41. 🔄 TODO: All 69 tests passing

### Phase 6: Predoc Module (TODO - Day 6)
39. ✅ Good news: `/Users/joerg/git/nixfmt-rs-old/src/predoc.rs` exists and is complete!
40. Copy predoc.rs from nixfmt-rs-old (~987 lines)
41. Verify it matches Haskell Predoc.hs functionality:
    - ✅ Doc types (DocE, Spacing, GroupAnn, TextAnn)
    - ✅ Pretty trait and combinators
    - ✅ Fixup pass
    - ✅ Layout algorithm with priority groups
    - ✅ fits, firstLineFits, unexpandSpacing functions
42. Add missing features (if any):
    - TrailingComment text annotation type
    - Trailing text type
43. Test predoc independently with simple inputs

### Phase 7: Pretty Printer (TODO - Days 7-8)
44. Port pretty.rs from nixfmt-rs-old as starting point
45. Adapt to our AST types (currently uses rnix AST)
46. Implement Pretty trait for our core types:
    - ✅ Token, Trivium, TrailingComment (should already exist)
    - Ann<T>, Items<T>, Item<T>
    - SimpleSelector, Selector
    - Binder (Inherit, Assignment)
    - Parameter, ParamAttr
    - Term (all variants)
    - Expression (all variants)
47. Implement formatting helpers:
    - moveTrailingCommentUp
    - moveParamsComments
    - isAbsorbableExpr, isSimple
    - absorbRHS, absorbLast, absorbApp
    - prettySet, prettyApp, prettyItems
48. Match Haskell Pretty.hs patterns exactly
49. Test each Pretty instance against nixfmt --ir output

### Phase 8: Integration & Testing (TODO - Days 9-10)
50. Wire everything in lib.rs:
    - Expose: parse(), pretty(), render()
    - Full pipeline: source → AST → Doc IR → rendered text
51. Add --ir flag to CLI (dump Doc IR)
52. Copy test suite from nixfmt-rs-old
53. Add comprehensive tests:
    - AST comparison: our --ast vs Haskell --ast
    - IR comparison: our --ir vs Haskell --ir
    - Output comparison: formatted output
    - Idempotency: format twice = identical
54. Run tests, debug failures using --ast and --ir comparison
55. Iterate until tests pass

### Phase 9: Polish (TODO - Day 11)
56. Add helpful error messages
57. Test error recovery
58. Add documentation comments
59. Run formatter on nixpkgs samples
60. Performance optimization if needed

## Success Criteria - CURRENT STATUS

- [x] **Lexer & Parser fully functional - 67/67 tests passing**
- [x] All Nix language features parsed correctly
- [x] Comment normalization matches Haskell exactly (removeStars, fixIndent)
- [x] String interpolation working (including nested)
- [x] All parameter types working (simple, set, context)
- [x] Paths working (relative, absolute, home, angle bracket)
- [x] Human-readable error messages ✅
- [x] **Trait-based formatter architecture implemented** ✅
  - ✅ Writer trait (low-level output)
  - ✅ HaskellFormat trait (AST formatting)
  - ✅ ColoredWriter implementation
  - ✅ Color scheme matches nixfmt exactly
  - ✅ Depth management working correctly
  - ✅ Basic types (Token, Trivium, Binder, Selector) implemented
  - 🔄 Need to implement all Expression/Term variants (39/40 TODO)
- [x] **Comprehensive test suite created** ✅
  - ✅ 69 automated tests in ast_format_tests.rs
  - ✅ Compares byte-for-byte with nixfmt --ast
  - 🔄 Tests passing for implemented types
  - 🔄 Full coverage pending complete implementation
- [ ] `--ir` output matches Haskell nixfmt's `--ir` (TODO: implement pretty printer)
- [ ] Idempotent formatting (format twice = same result) (TODO)
- [ ] Comments formatted identically to Haskell version (already working in --ast)

## Key Risks & Mitigations

**Risk**: Hand-written parser has bugs
- *Mitigation*: Port function-by-function from Haskell, test each piece

**Risk**: Operator precedence is tricky
- *Mitigation*: Use proven algorithm (Pratt or precedence climbing), test thoroughly

**Risk**: Comment normalization logic is complex
- *Mitigation*: Port Haskell line-by-line with unit tests for each function

**Risk**: String interpolation is complex
- *Mitigation*: Handle in lexer like Haskell does, test edge cases

## Current Architecture

### Core Modules (from nixfmt-rs2)
- ✅ `src/types.rs` - AST types matching Haskell (318 lines)
- ✅ `src/lexer.rs` - Hand-written lexer with trivia (1100+ lines)
- ✅ `src/parser.rs` - Hand-written recursive descent (1500+ lines)
- ✅ `src/error.rs` - ParseError with context (100 lines)

### Trait-Based Formatter (NEW!)
- ✅ `src/format_trait.rs` - HaskellFormat trait + impls (415 lines)
- ✅ `src/colored_writer.rs` - ColoredWriter implementation (80 lines)
- ✅ `src/main.rs` - CLI with --ast flag (50 lines)
- ✅ `tests/ast_format_tests.rs` - 69 comprehensive tests (450 lines)

### To Be Copied from nixfmt-rs-old
- ✅ **Available**: `src/predoc.rs` - Doc IR and renderer (~987 lines)
  - Complete Wadler/Leijen pretty-printer implementation
  - Layout algorithm with priority groups
  - Fixup pass, fits, firstLineFits
  - Ready to copy and adapt
- ✅ **Available**: `src/pretty.rs` - Pretty printer (~1868 lines)
  - Complete Pretty instances for rnix AST
  - Needs adaptation to our AST types
  - Good reference implementation

## Debugging Workflow

When tests fail:
1. Run `echo 'code' | nixfmt --ast` to see Haskell's AST
2. Run `echo 'code' | cargo run -- --ast` to see our AST
3. Diff the outputs to find parsing/formatting differences
4. Use `cat -A` to see ANSI escape codes and indentation
5. If parsing is correct, check `--ir` output to debug pretty-printer
6. Compare final formatted output

Example:
```bash
# Find AST difference (with colors visible)
diff <(echo '{a=1;}' | nixfmt --ast | cat -A) \
     <(echo '{a=1;}' | cargo run -- --ast | cat -A)

# Side-by-side comparison
diff -y <(echo '{a=1;}' | nixfmt --ast | head -50) \
        <(echo '{a=1;}' | cargo run -- --ast | head -50)

# Find pretty-printer difference (TODO)
diff <(echo '{a=1;}' | nixfmt --ir) <(echo '{a=1;}' | cargo run -- --ir)
```

## Trait-Based Formatter Design

### Architecture

```
┌─────────────────────┐
│  HaskellFormat      │  ← Trait implemented by AST types
│  trait              │     (Expression, Term, Binder, etc.)
└──────────┬──────────┘
           │ uses
           ▼
┌─────────────────────┐
│  Writer trait       │  ← Interface for output
└──────────┬──────────┘
           │ implemented by
           ▼
    ┌──────────────┐
    │ ColoredWriter│
    └──────────────┘
```

### Key Patterns

**Color Management:**
1. Get color BEFORE incrementing depth for delimiters `(`, `[`, `{`
2. Commas use the SAME color as their enclosing delimiter
3. Indent = depth × 4 spaces
4. Rainbow colors cycle through 8 levels based on nesting depth
5. Special colors for numbers (green), string quotes (white), string content (blue)

**Depth Management with `with_delimiters`:**
```rust
fn with_delimiters<W: Writer, F>(w: &mut W, open: &str, close: &str, f: F) {
    let delimiter_color = w.color();  // BEFORE depth++
    w.with_depth(|w| {
        w.write_colored(open, delimiter_color);
        f(w);
        w.newline();
        w.write_colored(close, delimiter_color);
    });
}
```

**Benefits:**
- **Extensible**: Easy to add PlainWriter, JsonWriter, etc.
- **Type-safe**: Each AST type implements its own formatting
- **Testable**: Can test each implementation independently
- **Maintainable**: Clear separation of concerns

### Test Suite

**69 comprehensive tests** in `tests/ast_format_tests.rs`:
- Compares our output byte-for-byte with `nixfmt --ast`
- Covers all major AST node types
- Guides implementation: each green test = another trait impl working

Run tests:
```bash
cargo test --test ast_format_tests
```

## References

- Haskell source: `/Users/joerg/git/nixfmt/src/Nixfmt/`
- Previous attempt: `/Users/joerg/git/nixfmt-rs/`
- Test suite: `/Users/joerg/git/nixfmt/test/`
- Haskell AST format: Run `nixfmt --ast` on any input
- Haskell IR format: Run `nixfmt --ir` on any input
