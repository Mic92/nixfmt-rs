//! PrettySimple trait for formatting AST nodes to match nixfmt Haskell's Show output
//!
//! This implementation is based on the pretty-simple Haskell library:
//! https://github.com/cdepillabout/pretty-simple
//!
//! Key algorithm from pretty-simple's `list` function:
//! - Empty list: []
//! - Single simple element: [ element ]
//! - Otherwise: multiline with comma-first

use crate::types::*;

/// Writer interface - handles output, colors, and indentation
pub trait Writer {
    /// Write plain text at current indentation
    fn write_plain(&mut self, text: &str);

    /// Write colored text at current indentation
    fn write_colored(&mut self, text: &str, color: &str);

    /// Start a new line
    fn newline(&mut self);

    /// Execute a closure with increased delimiter color depth
    fn with_color<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R;

    /// Get the current delimiter color (only valid within `with_color`)
    fn current_color(&self) -> &'static str;

    /// Execute a closure with increased depth
    fn with_depth<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R;
}

// ANSI color codes
const NUMBER_COLOR: &str = "\x1b[0;92;1m";
const STRING_QUOTE_COLOR: &str = "\x1b[0;97;1m";
const STRING_CONTENT_COLOR: &str = "\x1b[0;94;1m";

/// Trait for types that can be formatted as Haskell-style output
pub trait PrettySimple {
    fn format<W: Writer>(&self, w: &mut W);

    /// Check if this value is "simple" (can be formatted inline)
    /// Based on pretty-simple's isSimple function
    fn is_simple(&self) -> bool {
        false // Most things are not simple by default
    }

    /// Check if this type has built-in delimiters (brackets, braces, parens)
    /// Types with delimiters don't need extra parens when used as constructor arguments
    fn has_delimiters(&self) -> bool {
        false // Most types don't have delimiters
    }

    /// Whether this value is logically empty (used for collection heuristics)
    fn is_empty(&self) -> bool {
        false
    }
}

/// PrettySimple for &str - quoted string literals
/// Based on pretty-simple's StringLit
impl PrettySimple for &str {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_colored("\"", STRING_QUOTE_COLOR);
        w.write_colored(self, STRING_CONTENT_COLOR);
        w.write_colored("\"", STRING_QUOTE_COLOR);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// PrettySimple for String - delegates to &str
impl PrettySimple for String {
    fn format<W: Writer>(&self, w: &mut W) {
        self.as_str().format(w);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// PrettySimple for usize - number literals
/// Based on pretty-simple's NumberLit
impl PrettySimple for usize {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_colored(&self.to_string(), NUMBER_COLOR);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// PrettySimple for bool - Haskell Bool values
impl PrettySimple for bool {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_plain(if *self { "True" } else { "False" });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// sub_expr from pretty-simple's subExpr - formats a single expression with appropriate spacing
/// From Haskell:
///   subExpr x = let doc = prettyExpr opts x
///               in if isSimple x
///                  then nest 2 doc  -- space before simple
///                  else nest indentAmount $ line' <> doc  -- newline before complex
fn sub_expr<T: PrettySimple, W: Writer>(w: &mut W, arg: &T) {
    if arg.is_simple() {
        // Simple (with or without delimiters): space before
        w.write_plain(" ");
        arg.format(w);
    } else if arg.has_delimiters() {
        // Complex with delimiters: newline before
        w.newline();
        arg.format(w);
    } else {
        // Complex without delimiters: newline, indent, wrap in parens
        w.newline();
        w.with_color(|w_colored| {
            let paren_color = w_colored.current_color();
            w_colored.with_depth(|w_inner| {
                w_inner.write_colored("(", paren_color);
                w_inner.write_plain(" ");
                arg.format(w_inner);
                w_inner.newline();
                w_inner.write_colored(")", paren_color);
            });
        });
    }
}

/// Helper for list elements - handles spacing for simple vs delimited entries
fn list_elem<T: PrettySimple, W: Writer>(w: &mut W, elem: &T) {
    if elem.has_delimiters() {
        if elem.is_empty() {
            w.write_plain(" ");
            elem.format(w);
        } else {
            w.newline();
            elem.format(w);
        }
    } else {
        w.write_plain(" ");
        elem.format(w);
    }
}

/// Helper for record values - just format the value directly as part of the row
/// In Haskell's pShow, record field values are not wrapped by subExpr
fn format_record_value<T: PrettySimple, W: Writer>(w: &mut W, value: &T) {
    if value.has_delimiters() && !value.is_empty() {
        // Non-empty delimited values get a newline before them
        w.newline();
        value.format(w);
    } else {
        // Everything else: just add a space and format inline
        w.write_plain(" ");
        value.format(w);
    }
}

/// Helper for inline delimiters - writes colored delimiters with content on single line
/// Format: <open> <content> <close>
/// Caller is responsible for color/depth context
fn write_delimited<W: Writer, F>(w: &mut W, color: &str, open: &str, close: &str, f: F)
where
    F: FnOnce(&mut W),
{
    w.write_colored(open, color);
    w.write_plain(" ");
    f(w);
    w.write_plain(" ");
    w.write_colored(close, color);
}

/// Macro to format constructor applications
/// Based on pretty-simple's: Parens (CommaSeparated [[Other "Constructor", arg1, arg2, ...]])
/// Uses subExpr logic: simple elements get space before, complex get newline
/// Usage: format_constructor!(w, "ConstructorName", [arg1, arg2, arg3])
macro_rules! format_constructor {
    // Constructor with no arguments
    ($w:expr, $name:expr, []) => {
        $w.write_plain($name);
    };

    // Constructor with arguments - uses sub_expr for each
    ($w:expr, $name:expr, [ $($arg:expr),+ $(,)? ]) => {{
        $w.write_plain($name);
        $(
            sub_expr($w, $arg);
        )*
    }};
}

/// Macro to format record fields with comma separation
/// Based on pretty-simple's list function for Braces
/// From Haskell: Braces xss -> list "{" "}" xss
///
/// Usage: format_record!(w, [("field1", &value1), ("field2", &value2), ...])
macro_rules! format_record {
    ($w:expr, [ $(($name:expr, $value:expr)),+ $(,)? ]) => {{
        // Capture current color, then newline and increment depth
        $w.newline();
        $w.with_color(|w_color| {
            let brace_color = w_color.current_color();
            w_color.with_depth(|w| {
                w.write_colored("{", brace_color);
                format_record!(@fields w, brace_color; ; $( ($name, $value) ),+);
                w.newline();
                w.write_colored("}", brace_color);
            });
        });
    }};

    // Base case: first field
    (@fields $w:expr, $brace_color:expr; ; ($name:expr, $value:expr) $(, ($rest_name:expr, $rest_value:expr))* ) => {
        $w.write_plain(" ");
        $w.write_plain($name);
        $w.write_plain(" =");
        format_record_value($w, $value);
        $(
            format_record!(@fields $w, $brace_color; comma; ($rest_name, $rest_value));
        )*
    };

    // Recursive case: subsequent fields (with comma)
    (@fields $w:expr, $brace_color:expr; comma; ($name:expr, $value:expr)) => {
        $w.newline();
        $w.write_colored(",", $brace_color);
        $w.write_plain(" ");
        $w.write_plain($name);
        $w.write_plain(" =");
        format_record_value($w, $value);
    };
}

/// Macro to format enum match arms
/// Automatically generates match arms that call format_constructor! for each variant
///
/// Usage without wildcard:
///   format_enum!(self, w, {
///       Variant1(field) => [field],
///       Variant2(field1, field2) => [field1, field2],
///   });
///
/// Usage with wildcard (for fallback case):
///   format_enum!(self, w, {
///       Variant1(field) => [field],
///       _ => { w.write_plain(&format!("{:?}", self)); }
///   });
macro_rules! format_enum {
    // Version without wildcard
    ($self:expr, $w:expr, {
        $( $variant:ident ( $($field:ident),* $(,)? ) => [ $($arg:expr),* $(,)? ] ),* $(,)?
    }) => {
        match $self {
            $(
                Self::$variant($($field),*) => {
                    format_constructor!($w, stringify!($variant), [$($arg),*]);
                }
            )*
        }
    };

    // Version with wildcard
    ($self:expr, $w:expr, {
        $( $variant:ident ( $($field:ident),* $(,)? ) => [ $($arg:expr),* $(,)? ] ),* ,
        _ => $wildcard_body:block $(,)?
    }) => {
        match $self {
            $(
                Self::$variant($($field),*) => {
                    format_constructor!($w, stringify!($variant), [$($arg),*]);
                }
            )*
            _ => $wildcard_body
        }
    };
}

// Implement for all AST types

impl PrettySimple for Whole<Expression> {
    fn format<W: Writer>(&self, w: &mut W) {
        self.value.format(w);
        w.newline(); // Final newline at end of output
    }
}

impl PrettySimple for Expression {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            Term(term) => [term],
            With(kw, expr1, semi, expr2) => [kw, &**expr1, semi, &**expr2],
            Let(kw, items, in_kw, body) => [kw, &items.0, in_kw, &**body],
            Assert(kw, expr1, semi, expr2) => [kw, &**expr1, semi, &**expr2],
            If(if_kw, cond, then_kw, then_expr, else_kw, else_expr) => [if_kw, &**cond, then_kw, &**then_expr, else_kw, &**else_expr],
            Abstraction(param, colon, body) => [param, colon, &**body],
            Application(func, arg) => [&**func, &**arg],
            Operation(left, op, right) => [&**left, op, &**right],
            MemberCheck(expr, question, selectors) => [&**expr, question, selectors],
            Negation(minus, expr) => [minus, &**expr],
            Inversion(not, expr) => [not, &**expr],
        });
    }
}

impl PrettySimple for Term {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            Token(leaf) => [leaf],
            SimpleString(string) => [string],
            IndentedString(string) => [string],
            Path(path) => [path],
            List(open, items, close) => [open, &items.0, close],
            Set(rec, open, items, close) => [rec, open, &items.0, close],
            Selection(term, selectors, or_default) => [&**term, selectors, or_default],
            Parenthesized(open, expr, close) => [open, &**expr, close],
        });
    }
}

impl PrettySimple for Item<Term> {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            Item::Item(term) => {
                format_constructor!(w, "Item", [term]);
            }
            Item::Comments(trivia) => {
                w.write_plain("Comments");
                sub_expr(w, trivia);
            }
        }
    }

    fn is_simple(&self) -> bool {
        match self {
            Item::Item(_) => false,
            Item::Comments(trivia) => trivia.is_simple(),
        }
    }
}

impl PrettySimple for Item<Binder> {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            Item::Item(binder) => {
                format_constructor!(w, "Item", [binder]);
            }
            Item::Comments(trivia) => {
                w.write_plain("Comments");
                sub_expr(w, trivia);
            }
        }
    }

    fn is_simple(&self) -> bool {
        match self {
            Item::Item(_) => false,
            Item::Comments(trivia) => trivia.is_simple(),
        }
    }
}

impl PrettySimple for Binder {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            Inherit(kw, from, selectors, semi) => [kw, from, selectors, semi],
            Assignment(sels, eq, expr, semi) => [sels, eq, expr, semi],
        });
    }
}

impl PrettySimple for Selector {
    fn format<W: Writer>(&self, w: &mut W) {
        format_constructor!(w, "Selector", [&self.dot, &self.selector]);
    }
}

impl PrettySimple for SimpleSelector {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            IDSelector(leaf) => [leaf],
            InterpolSelector(part) => [part],
            StringSelector(string) => [string],
        });
    }
}

impl PrettySimple for Trivium {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            EmptyLine() => [],
            LineComment(text) => [text],
            BlockComment(is_doc, lines) => [is_doc, lines],
            LanguageAnnotation(text) => [text],
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for Trivia {
    fn format<W: Writer>(&self, w: &mut W) {
        if self.0.is_empty() {
            w.with_color(|w_color| {
                let bracket_color = w_color.current_color();
                w_color.write_colored("[", bracket_color);
                w_color.write_colored("]", bracket_color);
            });
            return;
        }

        if self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)) {
            self.0[0].format(w);
            return;
        }

        if self.0.len() == 1 {
            let first = &self.0[0];
            let inline = match first {
                Trivium::LineComment(_) => true,
                Trivium::EmptyLine() => true,
                Trivium::BlockComment(_, lines) => lines.len() <= 1,
                _ => false,
            };
            if inline {
                w.with_color(|w_color| {
                    let bracket_color = w_color.current_color();
                    w_color.with_depth(|w| {
                        write_delimited(w, bracket_color, "[", "]", |w| {
                            first.format(w);
                        });
                    });
                });
                return;
            }
        }

        w.with_color(|w_color| {
            let bracket_color = w_color.current_color();
            w_color.with_depth(|w_depth| {
                w_depth.write_colored("[", bracket_color);
                if let Some((first, rest)) = self.0.split_first() {
                    w_depth.write_plain(" ");
                    first.format(w_depth);
                    for trivium in rest {
                        w_depth.newline();
                        w_depth.write_colored(",", bracket_color);
                        w_depth.write_plain(" ");
                        trivium.format(w_depth);
                    }
                    let inline_close = rest.is_empty()
                        && match first {
                            Trivium::LineComment(_) => true,
                            Trivium::EmptyLine() => true,
                            Trivium::BlockComment(_, lines) => lines.len() <= 1,
                            _ => false,
                        };
                    if inline_close {
                        w_depth.write_plain(" ");
                        w_depth.write_colored("]", bracket_color);
                    } else {
                        w_depth.newline();
                        w_depth.write_colored("]", bracket_color);
                    }
                } else {
                    w_depth.write_colored("]", bracket_color);
                }
            });
        });
    }

    fn is_simple(&self) -> bool {
        self.0.is_empty()
            || (self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)))
            || (self.0.len() == 1 && matches!(self.0[0], Trivium::EmptyLine()))
    }

    fn has_delimiters(&self) -> bool {
        true
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PrettySimple for Parameter {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            IDParameter(leaf) => [leaf],
            SetParameter(open, attrs, close) => [open, attrs, close],
            ContextParameter(left, at, right) => [&**left, at, &**right],
        });
    }
}

impl PrettySimple for ParamAttr {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            ParamAttr(name, default, comma) => [name, default, comma],
            ParamEllipsis(ellipsis) => [ellipsis],
        });
    }
}

impl PrettySimple for StringPart {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            StringPart::TextPart(text) => {
                format_constructor!(w, "TextPart", [text]);
            }
            StringPart::Interpolation(whole) => {
                w.write_plain("Interpolation");
                w.write_plain(" ");
                whole.value.format(w);
            }
        }
    }

    fn is_simple(&self) -> bool {
        true
    }

    fn has_delimiters(&self) -> bool {
        false
    }
}

/// PrettySimple for Token - constructor applications for data-carrying tokens
impl PrettySimple for Token {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            Integer(s) => [s],
            Float(s) => [s],
            Identifier(s) => [s],
            EnvPath(s) => [s],
            _ => {
                // For all other tokens, use Debug output
                w.write_plain(&format!("{:?}", self));
            }
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

// Generic Ann<T> implementation for all T that implement PrettySimple
/// Helper wrapper for formatting "Pos N" inline
struct PosWrapper(usize);

impl PrettySimple for PosWrapper {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_plain("Pos ");
        self.0.format(w);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// PrettySimple for TrailingComment - constructor with comment contents
/// In Haskell's Show output, this becomes a Parens with simple elements,
/// so it formats inline as: ( TrailingComment "text" )
impl PrettySimple for TrailingComment {
    fn format<W: Writer>(&self, w: &mut W) {
        w.with_color(|w_color| {
            let paren_color = w_color.current_color();
            w_color.with_depth(|w| {
                write_delimited(w, paren_color, "(", ")", |w| {
                    format_constructor!(w, "TrailingComment", [&self.0]);
                });
            });
        });
    }

    fn is_simple(&self) -> bool {
        false // Constructor with argument = 2 elements in row, thus complex
    }

    fn has_delimiters(&self) -> bool {
        true // Has parens
    }
}

impl<T: PrettySimple> PrettySimple for Ann<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_plain("Ann");

        format_record!(
            w,
            [
                ("preTrivia", &self.pre_trivia),
                ("sourceLine", &PosWrapper(self.source_line.0)),
                ("value", &self.value),
                ("trailComment", &self.trail_comment),
            ]
        );
    }
}

/// Generic PrettySimple for Vec<T>
/// Based on pretty-simple's Brackets in Show output
/// Implements the `list` function logic:
/// - Vec<T> in Rust corresponds to a single "row" [[T]] in Haskell's CommaSeparated
/// - Empty vec: []
/// - All elements simple: [ elem1, elem2, ... ] (inline, space-separated with commas)
/// - Any element complex: multiline with comma-first
impl<T: PrettySimple> PrettySimple for Vec<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        // Empty list: [] - use current depth, don't increment
        if self.is_empty() {
            w.with_color(|w_color| {
                let bracket_color = w_color.current_color();
                w_color.write_colored("[", bracket_color);
                w_color.write_colored("]", bracket_color);
            });
            return;
        }

        // Non-empty: increment depth first, then capture color (matching Open annotation)
        w.with_color(|w_color| {
            let bracket_color = w_color.current_color();
            w_color.with_depth(|w_inner| {
                if self.len() == 1 && !self[0].has_delimiters() && self[0].is_simple() {
                    write_delimited(w_inner, bracket_color, "[", "]", |w| {
                        self[0].format(w);
                    });
                } else {
                    // Multiline with comma-first
                    w_inner.write_colored("[", bracket_color);
                    for (i, item) in self.iter().enumerate() {
                        if i > 0 {
                            w_inner.newline();
                            w_inner.write_colored(",", bracket_color);
                        }
                        list_elem(w_inner, item);
                    }
                    w_inner.newline();
                    w_inner.write_colored("]", bracket_color);
                }
            });
        });
    }

    fn is_simple(&self) -> bool {
        // Mirrors pretty-simple's list simplicity heuristic:
        // - Empty list is simple
        // - Single simple element without its own delimiters stays inline
        if self.is_empty() {
            return true;
        }
        if self.len() == 1 {
            let item = &self[0];
            return item.is_simple() && !item.has_delimiters();
        }
        false
    }

    fn has_delimiters(&self) -> bool {
        true // Vec formats with brackets
    }

    fn is_empty(&self) -> bool {
        <Vec<T>>::is_empty(self)
    }
}

/// Generic PrettySimple for Option<T>
/// Based on Haskell's Show instance for Maybe
impl<T: PrettySimple> PrettySimple for Option<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            Some(value) => {
                // Just value is a constructor application
                format_constructor!(w, "Just", [value]);
            }
            None => {
                w.write_plain("Nothing");
            }
        }
    }

    fn is_simple(&self) -> bool {
        // Nothing is simple (no args), Just is complex
        self.is_none()
    }
}

/// PrettySimple for tuples (A, B)
/// Based on Haskell's Show instance for tuples
impl<A: PrettySimple, B: PrettySimple> PrettySimple for (A, B) {
    fn format<W: Writer>(&self, w: &mut W) {
        // Tuple: (a, b)
        w.with_color(|w_color| {
            let paren_color = w_color.current_color();
            w_color.with_depth(|w_inner| {
                w_inner.write_colored("(", paren_color);
                w_inner.write_plain(" ");
                self.0.format(w_inner);
                w_inner.newline();
                w_inner.write_colored(",", paren_color);
                w_inner.write_plain(" ");
                self.1.format(w_inner);
                w_inner.newline();
                w_inner.write_colored(")", paren_color);
            });
        });
    }

    fn is_simple(&self) -> bool {
        false
    }

    fn has_delimiters(&self) -> bool {
        true
    }
}

/// PrettySimple for Box<T>
/// Box is transparent in Haskell's Show output
impl<T: PrettySimple> PrettySimple for Box<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        (**self).format(w);
    }

    fn is_simple(&self) -> bool {
        (**self).is_simple()
    }
}
