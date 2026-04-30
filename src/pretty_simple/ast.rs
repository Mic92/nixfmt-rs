//! `PrettySimple` implementations for AST nodes

use super::{
    NUMBER_COLOR, PrettySimple, STRING_CONTENT_COLOR, STRING_QUOTE_COLOR, Writer, escape_string,
    format_bracket_list, sub_expr, with_brackets,
};
use crate::format_constructor;
use crate::format_enum;
use crate::format_record;
use crate::types::*;

/// Generate a `PrettySimple` impl for a primitive/atomic type:
/// `is_simple` and `is_atomic` are always `true`; only `format` varies.
macro_rules! simple_atom {
    ($ty:ty, |$self_:ident, $w:ident| $body:expr) => {
        impl PrettySimple for $ty {
            fn format<W: Writer>(&self, $w: &mut W) {
                let $self_ = self;
                $body
            }
            fn is_simple(&self) -> bool {
                true
            }
            fn is_atomic(&self) -> bool {
                true
            }
        }
    };
}

// &str / String: quoted string literals (pretty-simple's StringLit)
simple_atom!(&str, |s, w| {
    w.write_colored("\"", STRING_QUOTE_COLOR);
    // Escape special characters to match Haskell's show behavior
    w.write_colored(&escape_string(s), STRING_CONTENT_COLOR);
    w.write_colored("\"", STRING_QUOTE_COLOR);
});
simple_atom!(String, |s, w| s.as_str().format(w));

// isize / usize: number literals (pretty-simple's NumberLit)
simple_atom!(isize, |n, w| w.write_colored(&n.to_string(), NUMBER_COLOR));
simple_atom!(usize, |n, w| w.write_colored(&n.to_string(), NUMBER_COLOR));

simple_atom!(bool, |b, w| w.write_plain(if *b {
    "True"
} else {
    "False"
}));

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

impl<T: PrettySimple> PrettySimple for Item<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            Item::Item(inner) => {
                format_constructor!(w, "Item", [inner]);
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
            Trivium::BlockComment(_is_doc, lines) => lines.is_simple(),
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

// Haskell `Trivia` is `Seq Trivium` since nixfmt 1.2.0; Show renders as `fromList [..]`.
impl PrettySimple for Trivia {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_plain("fromList");
        sub_expr(w, &self.to_vec());
    }

    fn renders_inline_parens(&self) -> bool {
        // `( fromList [ EmptyLine ] )` stays on one line when the inner list is simple.
        self.to_vec().is_simple()
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
            StringPart::TextPart(_) => true,
            StringPart::Interpolation(_) => false,
        }
    }
}

/// `PrettySimple` for Token - constructor applications for data-carrying tokens
impl PrettySimple for Token {
    fn format<W: Writer>(&self, w: &mut W) {
        format_enum!(self, w, {
            Integer(s) => [&s.as_str()],
            Float(s) => [&s.as_str()],
            Identifier(s) => [&s.as_str()],
            EnvPath(s) => [&s.as_str()],
            _ => {
                w.write_plain(&format!("{self:?}"));
            }
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// Helper wrapper for formatting span as "Pos N" for Haskell compatibility
/// Even though we use Span internally, the pretty-printed AST should match Haskell
#[derive(Debug)]
struct SpanWrapper(Span);

impl PrettySimple for SpanWrapper {
    fn format<W: Writer>(&self, w: &mut W) {
        use crate::error::context::ErrorContext;

        w.write_plain("Pos ");
        let ctx = ErrorContext::new(w.source(), None);
        let pos = ctx.position(self.0.start);
        pos.line.format(w);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// `PrettySimple` for `TrailingComment` - constructor with comment contents
/// In Haskell's Show output, this becomes a Parens with simple elements,
/// so it formats inline as: ( `TrailingComment` "text" )
impl PrettySimple for TrailingComment {
    fn format<W: Writer>(&self, w: &mut W) {
        with_brackets(w, "(", ")", true, |w, _| {
            w.write_plain(" ");
            format_constructor!(w, "TrailingComment", [&&*self.0]);
            w.write_plain(" ");
        });
    }

    fn has_delimiters(&self) -> bool {
        true
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

/// Generic `PrettySimple` for Vec<T>
/// Based on pretty-simple's Brackets in Show output
/// Implements the `list` function logic:
/// - Vec<T> in Rust corresponds to a single "row" [[T]] in Haskell's `CommaSeparated`
/// - Empty vec: []
/// - All elements simple: [ elem1, elem2, ... ] (inline, space-separated with commas)
/// - Any element complex: multiline with comma-first
impl<T: PrettySimple> PrettySimple for Vec<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        format_bracket_list(w, self, true);
    }

    fn is_simple(&self) -> bool {
        // Mirrors pretty-simple's isListSimple:
        // isListSimple [[e]] = isSimple e && case e of Other s -> not $ any isSpace s ; _ -> True
        // isListSimple _:_ = False
        // isListSimple [] = True
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
        true
    }

    fn is_empty(&self) -> bool {
        <Vec<T>>::is_empty(self)
    }
}

/// Generic `PrettySimple` for Option<T>
/// Based on Haskell's Show instance for Maybe
impl<T: PrettySimple> PrettySimple for Option<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            Some(value) => {
                format_constructor!(w, "Just", [value]);
            }
            None => {
                w.write_plain("Nothing");
            }
        }
    }

    fn is_simple(&self) -> bool {
        self.is_none()
    }
}

/// `PrettySimple` for tuples (A, B)
/// Based on Haskell's Show instance for tuples
impl<A: PrettySimple, B: PrettySimple> PrettySimple for (A, B) {
    fn format<W: Writer>(&self, w: &mut W) {
        with_brackets(w, "(", ")", true, |w, paren_color| {
            w.write_plain(" ");
            self.0.format(w);
            w.newline();
            w.write_colored(",", paren_color);
            w.write_plain(" ");
            self.1.format(w);
            w.newline();
        });
    }

    fn has_delimiters(&self) -> bool {
        true
    }
}

/// `PrettySimple` for Box<T>
/// Box is transparent in Haskell's Show output
impl<T: PrettySimple> PrettySimple for Box<T> {
    fn format<W: Writer>(&self, w: &mut W) {
        (**self).format(w);
    }

    fn is_simple(&self) -> bool {
        (**self).is_simple()
    }
}
