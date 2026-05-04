//! `Dump` implementations for AST nodes

use super::{
    Dump, NUMBER_COLOR, STRING_CONTENT_COLOR, STRING_QUOTE_COLOR, Writer, dump_list, escape_string,
    sub_expr, with_brackets,
};
use crate::ast::{
    Annotated, Binder, Expression, Item, ParamAttr, ParamDefault, Parameter, Selector, SetDefault,
    SimpleSelector, Span, StringPart, Term, Token, Trailed, TrailingComment, Trivia, TriviaPiece,
};
use crate::dump_enum;
use crate::format_constructor;
use crate::format_record;

/// Generate a `Dump` impl for a primitive/atomic type:
/// `is_simple` and `is_atomic` are always `true`; only `format` varies.
macro_rules! simple_atom {
    ($ty:ty, |$self_:ident, $w:ident| $body:expr) => {
        impl Dump for $ty {
            fn dump<W: Writer>(&self, $w: &mut W) {
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
simple_atom!(String, |s, w| s.as_str().dump(w));
simple_atom!(Box<str>, |s, w| (&**s).dump(w));

// isize / usize: number literals (pretty-simple's NumberLit)
simple_atom!(isize, |n, w| w.write_colored(&n.to_string(), NUMBER_COLOR));
simple_atom!(usize, |n, w| w.write_colored(&n.to_string(), NUMBER_COLOR));

simple_atom!(bool, |b, w| w.write_plain(if *b {
    "True"
} else {
    "False"
}));

impl Dump for Trailed<Expression> {
    fn dump<W: Writer>(&self, w: &mut W) {
        self.value.dump(w);
        w.newline(); // Final newline at end of output
    }
}

impl Dump for Expression {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::Term(term) => format_constructor!(w, "Term", [term]),
            Self::With {
                kw_with,
                scope,
                semi,
                body,
            } => {
                format_constructor!(w, "With", [kw_with, &**scope, semi, &**body]);
            }
            Self::Let {
                kw_let,
                bindings,
                kw_in,
                body,
            } => {
                format_constructor!(w, "Let", [kw_let, &bindings.0, kw_in, &**body]);
            }
            Self::Assert {
                kw_assert,
                cond,
                semi,
                body,
            } => {
                format_constructor!(w, "Assert", [kw_assert, &**cond, semi, &**body]);
            }
            Self::If {
                kw_if,
                cond,
                kw_then,
                then_branch,
                kw_else,
                else_branch,
            } => {
                format_constructor!(
                    w,
                    "If",
                    [
                        kw_if,
                        &**cond,
                        kw_then,
                        &**then_branch,
                        kw_else,
                        &**else_branch
                    ]
                );
            }
            Self::Abstraction { param, colon, body } => {
                format_constructor!(w, "Abstraction", [param, colon, &**body]);
            }
            Self::Application { func, arg } => {
                format_constructor!(w, "Application", [&**func, &**arg]);
            }
            Self::Operation { lhs, op, rhs } => {
                format_constructor!(w, "Operation", [&**lhs, op, &**rhs]);
            }
            Self::MemberCheck {
                lhs,
                question,
                path,
            } => {
                format_constructor!(w, "MemberCheck", [&**lhs, question, path]);
            }
            Self::Negation { minus, expr } => {
                format_constructor!(w, "Negation", [minus, &**expr]);
            }
            Self::Inversion { bang, expr } => {
                format_constructor!(w, "Inversion", [bang, &**expr]);
            }
        }
    }
}

impl Dump for Term {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::Token(leaf) => format_constructor!(w, "Token", [leaf]),
            Self::SimpleString(s) => format_constructor!(w, "SimpleString", [s]),
            Self::IndentedString(s) => format_constructor!(w, "IndentedString", [s]),
            Self::Path(p) => format_constructor!(w, "Path", [p]),
            Self::List { open, items, close } => {
                format_constructor!(w, "List", [open, &items.0, close]);
            }
            Self::Set {
                rec,
                open,
                items,
                close,
            } => {
                format_constructor!(w, "Set", [rec, open, &items.0, close]);
            }
            Self::Selection {
                base,
                selectors,
                default,
            } => {
                format_constructor!(w, "Selection", [&**base, selectors, default]);
            }
            Self::Parenthesized { open, expr, close } => {
                format_constructor!(w, "Parenthesized", [open, &**expr, close]);
            }
        }
    }
}

// `SetDefault` and `ParamDefault` are display-equivalent to the original
// `(Leaf, _)` tuples; format them as 2-tuples so AST output stays
// byte-identical with Haskell `nixfmt --ast`.
/// Format two parts as a Haskell 2-tuple `( a, b )` literal.
fn format_pair<W: Writer, A: Dump, B: Dump>(w: &mut W, a: &A, b: &B) {
    with_brackets(w, "(", ")", true, |w, paren_color| {
        w.write_plain(" ");
        a.dump(w);
        w.newline();
        w.write_colored(",", paren_color);
        w.write_plain(" ");
        b.dump(w);
        w.newline();
    });
}

impl Dump for SetDefault {
    fn dump<W: Writer>(&self, w: &mut W) {
        format_pair(w, &self.or_kw, &*self.value);
    }
    fn has_delimiters(&self) -> bool {
        true
    }
}

impl Dump for ParamDefault {
    fn dump<W: Writer>(&self, w: &mut W) {
        format_pair(w, &self.question, &self.value);
    }
    fn has_delimiters(&self) -> bool {
        true
    }
}

impl<T: Dump> Dump for Item<T> {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::Item(inner) => {
                format_constructor!(w, "Item", [inner]);
            }
            Self::Comments(trivia) => {
                w.write_plain("Comments");
                sub_expr(w, trivia);
            }
        }
    }

    fn is_simple(&self) -> bool {
        match self {
            Self::Item(_) => false,
            Self::Comments(trivia) => trivia.is_simple(),
        }
    }
}

impl Dump for Binder {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::Inherit {
                kw,
                from,
                attrs,
                semi,
            } => {
                format_constructor!(w, "Inherit", [kw, from, attrs, semi]);
            }
            Self::Assignment {
                path,
                eq,
                value,
                semi,
            } => {
                format_constructor!(w, "Assignment", [path, eq, value, semi]);
            }
        }
    }
}

impl Dump for Selector {
    fn dump<W: Writer>(&self, w: &mut W) {
        format_constructor!(w, "Selector", [&self.dot, &self.selector]);
    }
}

impl Dump for SimpleSelector {
    fn dump<W: Writer>(&self, w: &mut W) {
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

impl Dump for TriviaPiece {
    fn dump<W: Writer>(&self, w: &mut W) {
        dump_enum!(self, w, {
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
            // Nullary constructor / single string arg are simple.
            Self::EmptyLine() | Self::LineComment(_) | Self::LanguageAnnotation(_) => true,
            Self::BlockComment(_is_doc, lines) => lines.is_simple(),
        }
    }

    fn is_atomic(&self) -> bool {
        // Only nullary constructors are atomic (single element in parsed form)
        // EmptyLine → Other "EmptyLine" → atomic
        // LineComment "x" → Other "LineComment " + StringLit → not atomic
        matches!(self, Self::EmptyLine())
    }
}

// Haskell `Trivia` is `Seq Trivium` since nixfmt 1.2.0; Show renders as `fromList [..]`.
impl Dump for Trivia {
    fn dump<W: Writer>(&self, w: &mut W) {
        w.write_plain("fromList");
        sub_expr(w, &self.to_vec());
    }

    fn renders_inline_parens(&self) -> bool {
        // `( fromList [ EmptyLine ] )` stays on one line when the inner list is simple.
        self.to_vec().is_simple()
    }
}

impl Dump for Parameter {
    fn dump<W: Writer>(&self, w: &mut W) {
        // Use Haskell constructor names for compatibility with nixfmt --ast output
        match self {
            Self::Id(leaf) => {
                format_constructor!(w, "IDParameter", [leaf]);
            }
            Self::Set { open, attrs, close } => {
                format_constructor!(w, "SetParameter", [open, attrs, close]);
            }
            Self::Context { lhs, at, rhs } => {
                format_constructor!(w, "ContextParameter", [&**lhs, at, &**rhs]);
            }
        }
    }
}

impl Dump for ParamAttr {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::Attr {
                name,
                default,
                comma,
            } => {
                format_constructor!(w, "ParamAttr", [name, default, comma]);
            }
            Self::Ellipsis(ellipsis) => {
                format_constructor!(w, "ParamEllipsis", [ellipsis]);
            }
        }
    }
}

impl Dump for StringPart {
    fn dump<W: Writer>(&self, w: &mut W) {
        match self {
            Self::TextPart(text) => {
                format_constructor!(w, "TextPart", [text]);
            }
            Self::Interpolation(whole) => {
                w.write_plain("Interpolation");
                w.write_plain(" ");
                whole.value.dump(w);
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
            Self::TextPart(_) => true,
            Self::Interpolation(_) => false,
        }
    }
}

/// `Dump` for Token - constructor applications for data-carrying tokens
impl Dump for Token {
    fn dump<W: Writer>(&self, w: &mut W) {
        dump_enum!(self, w, {
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

impl Dump for SpanWrapper {
    fn dump<W: Writer>(&self, w: &mut W) {
        use crate::error::ErrorContext;

        w.write_plain("Pos ");
        let ctx = ErrorContext::new(w.source(), None);
        let pos = ctx.position(self.0.start());
        pos.line.dump(w);
    }

    fn is_simple(&self) -> bool {
        true
    }
}

/// `Dump` for `TrailingComment` - constructor with comment contents
/// In Haskell's Show output, this becomes a Parens with simple elements,
/// so it formats inline as: ( `TrailingComment` "text" )
impl Dump for TrailingComment {
    fn dump<W: Writer>(&self, w: &mut W) {
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

impl<T: Dump> Dump for Annotated<T> {
    fn dump<W: Writer>(&self, w: &mut W) {
        // Reference `nixfmt --ast` emits the Haskell constructor name.
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

/// Generic `Dump` for Vec<T>
/// Based on pretty-simple's Brackets in Show output
/// Implements the `list` function logic:
/// - Vec<T> in Rust corresponds to a single "row" [[T]] in Haskell's `CommaSeparated`
/// - Empty vec: []
/// - All elements simple: [ elem1, elem2, ... ] (inline, space-separated with commas)
/// - Any element complex: multiline with comma-first
impl<T: Dump> Dump for Vec<T> {
    fn dump<W: Writer>(&self, w: &mut W) {
        dump_list(w, self, true);
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
        <Self>::is_empty(self)
    }
}

/// Generic `Dump` for Option<T>
/// Based on Haskell's Show instance for Maybe
impl<T: Dump> Dump for Option<T> {
    fn dump<W: Writer>(&self, w: &mut W) {
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

/// `Dump` for tuples (A, B)
/// Based on Haskell's Show instance for tuples
impl<A: Dump, B: Dump> Dump for (A, B) {
    fn dump<W: Writer>(&self, w: &mut W) {
        with_brackets(w, "(", ")", true, |w, paren_color| {
            w.write_plain(" ");
            self.0.dump(w);
            w.newline();
            w.write_colored(",", paren_color);
            w.write_plain(" ");
            self.1.dump(w);
            w.newline();
        });
    }

    fn has_delimiters(&self) -> bool {
        true
    }
}

/// `Dump` for `Box<[T]>` — renders like a `Vec<T>` (sequence brackets).
impl<T: Dump> Dump for Box<[T]> {
    fn dump<W: Writer>(&self, w: &mut W) {
        dump_list(w, self, true);
    }

    fn is_simple(&self) -> bool {
        if self.is_empty() {
            return true;
        }
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
        <[T]>::is_empty(self)
    }
}

/// `Dump` for Box<T>
/// Box is transparent in Haskell's Show output
impl<T: Dump> Dump for Box<T> {
    fn dump<W: Writer>(&self, w: &mut W) {
        (**self).dump(w);
    }

    fn is_simple(&self) -> bool {
        (**self).is_simple()
    }
}
