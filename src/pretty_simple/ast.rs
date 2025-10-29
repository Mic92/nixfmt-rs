//! PrettySimple implementations for AST nodes

use super::{
    escape_string, format_delimited_value, sub_expr, write_delimited, PrettySimple, Writer,
    NUMBER_COLOR, STRING_CONTENT_COLOR, STRING_QUOTE_COLOR,
};
use crate::format_constructor;
use crate::format_enum;
use crate::format_record;
use crate::types::*;

/// PrettySimple for &str - quoted string literals
/// Based on pretty-simple's StringLit
impl PrettySimple for &str {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_colored("\"", STRING_QUOTE_COLOR);
        // Escape special characters to match Haskell's show behavior
        let escaped = escape_string(self);
        w.write_colored(&escaped, STRING_CONTENT_COLOR);
        w.write_colored("\"", STRING_QUOTE_COLOR);
    }

    fn is_simple(&self) -> bool {
        true
    }

    fn is_atomic(&self) -> bool {
        true // Primitives are atomic
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

    fn is_atomic(&self) -> bool {
        true // Primitives are atomic
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

    fn is_atomic(&self) -> bool {
        true // Primitives are atomic
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

    fn is_atomic(&self) -> bool {
        true // Primitives are atomic
    }
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
        // Use Haskell constructor names for compatibility with nixfmt --ast output
        match self {
            Self::ID(leaf) => {
                format_constructor!(w, "IDSelector", [leaf]);
            }
            Self::Interpol(part) => {
                format_constructor!(w, "InterpolSelector", [part]);
            }
            Self::String(string) => {
                format_constructor!(w, "StringSelector", [string]);
            }
        }
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
        // In Haskell: constructor applications with simple args can be simple
        // BlockComment True ["doc"] → all arguments simple → renders inline
        // BlockComment True ["a","b","c"] → Vec with 3 elements NOT simple → renders multiline
        match self {
            Trivium::EmptyLine() => true,    // Nullary constructor
            Trivium::LineComment(_) => true, // String arg is simple
            Trivium::BlockComment(_is_doc, lines) => {
                // Simple if the Vec is simple (empty or single simple element)
                lines.is_simple()
            }
            Trivium::LanguageAnnotation(_) => true, // String arg is simple
        }
    }

    fn is_atomic(&self) -> bool {
        // Only nullary constructors are atomic (single element in parsed form)
        // EmptyLine → Other "EmptyLine" → atomic
        // LineComment "x" → Other "LineComment " + StringLit → not atomic
        matches!(self, Trivium::EmptyLine())
    }
}

impl PrettySimple for Trivia {
    fn format<W: Writer>(&self, w: &mut W) {
        // Special case: LanguageAnnotation renders without brackets
        if self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)) {
            self.0[0].format(w);
            return;
        }

        // Otherwise, delegate to standard Vec formatting
        self.0.format(w);
    }

    fn is_simple(&self) -> bool {
        // Special case: LanguageAnnotation without brackets is simple
        if self.0.len() == 1 && matches!(self.0[0], Trivium::LanguageAnnotation(_)) {
            return true;
        }
        // Otherwise delegate to Vec's is_simple logic
        self.0.is_simple()
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
        // Use Haskell constructor names for compatibility with nixfmt --ast output
        match self {
            Self::ID(leaf) => {
                format_constructor!(w, "IDParameter", [leaf]);
            }
            Self::Set(open, attrs, close) => {
                format_constructor!(w, "SetParameter", [open, attrs, close]);
            }
            Self::Context(left, at, right) => {
                format_constructor!(w, "ContextParameter", [&**left, at, &**right]);
            }
        }
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
        // For Vec inline rendering: constructor applications with simple args behave as simple
        // In Haskell: the row [Other "TextPart ", StringLit "hello"] passes `all isSimple`
        // So [TextPart "hello"] can be rendered inline
        //
        // However, for structural simplicity (Vec::is_simple), this creates a multi-element row,
        // so the Brackets itself is NOT simple. That's handled by Vec::is_simple logic.
        match self {
            StringPart::TextPart(_) => true,       // Simple argument
            StringPart::Interpolation(_) => false, // Complex argument
        }
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
/// Helper wrapper for formatting span as "Pos N" for Haskell compatibility
/// Even though we use Span internally, the pretty-printed AST should match Haskell
#[derive(Debug)]
struct SpanWrapper(Span);

impl PrettySimple for SpanWrapper {
    fn format<W: Writer>(&self, w: &mut W) {
        use crate::error::context::ErrorContext;

        w.write_plain("Pos ");

        // Compute line number from byte offset
        let ctx = ErrorContext::new(w.source(), None);
        let pos = ctx.position(self.0.start);
        pos.line.format(w);
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
                ("sourceLine", &SpanWrapper(self.span)),
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
        // EXACT Haskell logic from list function (Printer.hs:252-254):
        //   [xs] | all isSimple xs -> space <> hcat (map (prettyExpr opts) xs) <> space
        //   _ -> concatWith lineAndCommaSep ...
        w.with_color(|w_color| {
            let bracket_color = w_color.current_color();
            w_color.with_depth(|w_inner| {
                // EXACT Haskell logic: [xs] | all isSimple xs
                // This matches when there is ONE row (single element in Vec) with all simple elements
                // Multiple elements in Vec → multiple rows → takes else branch (multiline)
                if self.len() == 1 && self[0].is_simple() {
                    // Case: [xs] | all isSimple xs (ONE row, all elements simple)
                    // Inline format: [ elem1 elem2 ... ]
                    w_inner.write_colored("[", bracket_color);
                    w_inner.write_plain(" ");
                    for (i, item) in self.iter().enumerate() {
                        if i > 0 {
                            w_inner.write_plain(" ");
                        }
                        item.format(w_inner);
                    }
                    w_inner.write_plain(" ");
                    w_inner.write_colored("]", bracket_color);
                } else {
                    // Case: _ (multiline with comma-first)
                    w_inner.write_colored("[", bracket_color);
                    for (i, item) in self.iter().enumerate() {
                        if i > 0 {
                            w_inner.newline();
                            w_inner.write_colored(",", bracket_color);
                        }
                        format_delimited_value(w_inner, item);
                    }
                    w_inner.newline();
                    w_inner.write_colored("]", bracket_color);
                }
            });
        });
    }

    fn is_simple(&self) -> bool {
        // Mirrors pretty-simple's isListSimple:
        // isListSimple [[e]] = isSimple e && case e of Other s -> not $ any isSpace s ; _ -> True
        // isListSimple _:_ = False
        // isListSimple [] = True
        //
        // Empty list is simple
        if self.is_empty() {
            return true;
        }
        // Single element: simple if it's atomic OR (simple AND has delimiters)
        // In Haskell: [[e]] matches only when the row has ONE element
        // - [EmptyLine] → row: [Other "EmptyLine"] → 1 element → atomic → simple
        // - [TextPart "x"] → row: [Other, StringLit] → 2 elements → NOT simple
        // - [[]] → row: [Brackets []] → 1 element, simple delimited → simple
        if self.len() == 1 {
            let item = &self[0];
            return item.is_atomic() || (item.is_simple() && item.has_delimiters());
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
