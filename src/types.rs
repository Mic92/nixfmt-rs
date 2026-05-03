//! AST types matching nixfmt Haskell's Types.hs

/// Identifier / number text. `CompactString` stores up to 24 bytes inline,
/// which covers virtually every Nix identifier and literal, eliminating the
/// per-token heap allocation that previously dominated parser drop time.
pub type TokenText = compact_str::CompactString;

/// A byte offset range in the source with line information.
///
/// Stored as `u32` so a `Span` is 16 bytes instead of 32; every AST leaf
/// carries one, and the parser moves leaves by value constantly, so the
/// halved width measurably reduces `memmove` traffic. Nix source files are
/// far below 4 GiB, so the narrower offsets are not a practical limitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    start: u32,      // byte offset
    end: u32,        // byte offset
    start_line: u32, // line number (1-indexed)
    end_line: u32,   // line number (1-indexed)
}

impl Span {
    /// Create a span from byte offsets, with line numbers defaulting to 1.
    #[allow(clippy::cast_possible_truncation)] // source files are < 4 GiB
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Create a new span with line information
    #[allow(clippy::cast_possible_truncation)]
    pub const fn with_lines(start: usize, end: usize, start_line: usize, end_line: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: start_line as u32,
            end_line: end_line as u32,
        }
    }

    /// Create a zero-length span at the given offset
    #[allow(clippy::cast_possible_truncation)]
    pub const fn point(offset: usize) -> Self {
        Self {
            start: offset as u32,
            end: offset as u32,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Start byte offset.
    #[inline]
    pub const fn start(self) -> usize {
        self.start as usize
    }

    /// End byte offset (exclusive).
    #[inline]
    pub const fn end(self) -> usize {
        self.end as usize
    }

    /// Line number of the start offset (1-indexed).
    #[inline]
    pub const fn start_line(self) -> usize {
        self.start_line as usize
    }

    /// Line number of the end offset (1-indexed).
    #[inline]
    pub const fn end_line(self) -> usize {
        self.end_line as usize
    }

    /// Length in bytes.
    #[inline]
    pub const fn len(self) -> usize {
        (self.end - self.start) as usize
    }

    /// True iff the span covers zero bytes.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Byte range, suitable for slicing the source: `source[span.range()]`.
    #[inline]
    pub const fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// Trivia - comments and whitespace
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trivium {
    EmptyLine(),
    LineComment(Box<str>),
    /// `BlockComment(is_doc`, lines)
    /// `is_doc` = true for /** */ comments
    BlockComment(bool, Box<[Box<str>]>),
    LanguageAnnotation(Box<str>),
}

/// Wrapper around a list of trivia items (comments/whitespace).
///
/// Stored as a boxed slice behind an `Option` so the overwhelmingly common
/// empty case is two zero words and never allocates: every `Ann<T>` carries
/// one of these, and the parser moves `Ann` values by value through every
/// production, so the 24→16 byte saving compounds across the whole AST.
/// Trivia runs are built once at lexeme boundaries and then read-only, so a
/// frozen slice (single allocation) fits better than a growable `Vec`.
#[derive(Debug, Clone, Default)]
pub struct Trivia(Option<Box<[Trivium]>>);

impl Trivia {
    /// Empty trivia list (no allocation).
    #[inline]
    pub const fn new() -> Self {
        Self(None)
    }

    /// Single-element trivia list.
    pub fn one(t: Trivium) -> Self {
        Self(Some(Box::new([t])))
    }

    /// Append a trivium.
    ///
    /// Reallocates the backing slice; callers on hot paths should accumulate
    /// into a `Vec<Trivium>` and convert once. Existing call sites only hit
    /// this on comment-bearing tokens, which are rare.
    pub fn push(&mut self, t: Trivium) {
        let mut v: Vec<Trivium> = std::mem::take(self).into();
        v.push(t);
        *self = v.into();
    }

    /// Insert at `idx`. Same reallocation caveat as [`Self::push`].
    pub fn insert(&mut self, idx: usize, t: Trivium) {
        let mut v: Vec<Trivium> = std::mem::take(self).into();
        v.insert(idx, t);
        *self = v.into();
    }

    /// Append all items from `iter`, allocating only if it yields any.
    pub fn extend<I: IntoIterator<Item = Trivium>>(&mut self, iter: I) {
        let mut iter = iter.into_iter();
        if let Some(first) = iter.next() {
            let mut v: Vec<Trivium> = std::mem::take(self).into();
            v.push(first);
            v.extend(iter);
            *self = v.into();
        }
    }

    /// Drop all items, retaining no allocation.
    #[inline]
    pub fn clear(&mut self) {
        self.0 = None;
    }
}

impl PartialEq for Trivia {
    fn eq(&self, other: &Self) -> bool {
        // `None` and `Some(empty)` are observationally identical.
        self[..] == other[..]
    }
}
impl Eq for Trivia {}

impl std::ops::Deref for Trivia {
    type Target = [Trivium];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Some(v) => v,
            None => &[],
        }
    }
}

impl From<Vec<Trivium>> for Trivia {
    fn from(value: Vec<Trivium>) -> Self {
        if value.is_empty() {
            Self(None)
        } else {
            Self(Some(value.into_boxed_slice()))
        }
    }
}

impl From<Trivia> for Vec<Trivium> {
    fn from(val: Trivia) -> Self {
        val.0.map(Self::from).unwrap_or_default()
    }
}

impl IntoIterator for Trivia {
    type Item = Trivium;
    type IntoIter = std::vec::IntoIter<Trivium>;

    fn into_iter(self) -> Self::IntoIter {
        Vec::from(self).into_iter()
    }
}

impl<'a> IntoIterator for &'a Trivia {
    type Item = &'a Trivium;
    type IntoIter = std::slice::Iter<'a, Trivium>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Trailing comment on same line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingComment(pub Box<str>);

/// Annotated wrapper - every AST node has:
/// - `pre_trivia`: Comments/whitespace before the token
/// - span: Byte range in source
/// - value: The actual value
/// - `trail_comment`: Optional trailing comment on same line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ann<T> {
    pub pre_trivia: Trivia,
    pub span: Span,
    pub value: T,
    pub trail_comment: Option<TrailingComment>,
}

impl<T: Clone> Ann<T> {
    pub fn without_trail(&self) -> Self {
        Self {
            trail_comment: None,
            ..self.clone()
        }
    }

    pub fn without_pre(&self) -> Self {
        Self {
            pre_trivia: Trivia::new(),
            ..self.clone()
        }
    }
}

/// Type-erased shared view of an `Ann<_>`'s trivia fields.
///
/// Lets `FirstToken` return a uniform borrow regardless of the underlying
/// `Ann<T>` payload type (`Token`, `NixString`, `Path`, ...).
pub struct AnnSlot<'a> {
    pub pre_trivia: &'a Trivia,
    pub trail_comment: &'a Option<TrailingComment>,
}

/// Mutable counterpart of [`AnnSlot`].
pub struct AnnSlotMut<'a> {
    pub pre_trivia: &'a mut Trivia,
    pub trail_comment: &'a mut Option<TrailingComment>,
}

impl<'a, T> From<&'a Ann<T>> for AnnSlot<'a> {
    fn from(a: &'a Ann<T>) -> Self {
        AnnSlot {
            pre_trivia: &a.pre_trivia,
            trail_comment: &a.trail_comment,
        }
    }
}

impl<'a, T> From<&'a mut Ann<T>> for AnnSlotMut<'a> {
    fn from(a: &'a mut Ann<T>) -> Self {
        AnnSlotMut {
            pre_trivia: &mut a.pre_trivia,
            trail_comment: &mut a.trail_comment,
        }
    }
}

/// Walk to the leftmost leaf `Ann<_>` of an AST node.
///
/// Haskell analogue: `mapFirstToken'` / `matchFirstToken` (Types.hs).
pub trait FirstToken {
    fn first_token(&self) -> AnnSlot<'_>;
    fn first_token_mut(&mut self) -> AnnSlotMut<'_>;
}

/// Expand one match arm for `first_token_impl!`: `leaf` arms hit an `Ann<_>`
/// directly via `.into()`, `recurse` arms call `$rec` (`first_token` or
/// `first_token_mut`) on a child node.
macro_rules! first_token_arm {
    (leaf, $rec:ident, $e:expr) => {
        $e.into()
    };
    (recurse, $rec:ident, $e:expr) => {
        $e.$rec()
    };
}

/// Generate both `first_token` and `first_token_mut` from one set of match
/// arms, avoiding the otherwise-identical `&`/`&mut` duplication.
macro_rules! first_token_impl {
    ($ty:ty; $($pat:pat => $kind:ident $e:expr),+ $(,)?) => {
        impl FirstToken for $ty {
            fn first_token(&self) -> AnnSlot<'_> {
                match self { $($pat => first_token_arm!($kind, first_token, $e),)+ }
            }
            fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
                match self { $($pat => first_token_arm!($kind, first_token_mut, $e),)+ }
            }
        }
    };
}

first_token_impl! { Term;
    Self::Token(l) => leaf l,
    Self::SimpleString(s) | Self::IndentedString(s) => leaf s,
    Self::Path(p) => leaf p,
    Self::List { open, .. }
    | Self::Parenthesized { open, .. }
    | Self::Set { rec: None, open, .. } => leaf open,
    Self::Set { rec: Some(rec), .. } => leaf rec,
    Self::Selection { base, .. } => recurse base,
}

first_token_impl! { Parameter;
    Self::Id(n) => leaf n,
    Self::Set { open, .. } => leaf open,
    Self::Context { lhs, .. } => recurse lhs,
}

first_token_impl! { Expression;
    Self::Term(t) => recurse t,
    Self::With { kw_with: kw, .. }
    | Self::Let { kw_let: kw, .. }
    | Self::Assert { kw_assert: kw, .. }
    | Self::If { kw_if: kw, .. }
    | Self::Negation { minus: kw, .. }
    | Self::Inversion { bang: kw, .. } => leaf kw,
    Self::Abstraction { param, .. } => recurse param,
    Self::Application { func: g, .. }
    | Self::Operation { lhs: g, .. }
    | Self::MemberCheck { lhs: g, .. } => recurse g,
}

/// Haskell `convertTrailing`.
impl From<&TrailingComment> for Trivium {
    fn from(tc: &TrailingComment) -> Self {
        Self::LineComment(format!(" {}", tc.0).into_boxed_str())
    }
}

/// Items with interleaved comments (for lists, sets, let bindings)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item<T> {
    /// An actual item
    Item(T),
    /// Trivia interleaved in items
    Comments(Trivia),
}

/// Items wrapper (newtype)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items<T>(pub Vec<Item<T>>);

/// A token annotated with trivia and span (Haskell: `Leaf`).
pub type Leaf = Ann<Token>;

/// String parts - either text or interpolation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    TextPart(Box<str>),
    Interpolation(Box<Whole<Expression>>),
}

/// A path literal: a single line of text / interpolation parts (Haskell: `Path`).
pub type Path = Ann<Vec<StringPart>>;

/// A string consists of lines, each of which consists of text elements and interpolations
pub type NixString = Ann<Vec<Vec<StringPart>>>;

/// Simple selector (no dot prefix)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    ID(Leaf),
    Interpol(Ann<StringPart>),
    String(NixString),
}

/// Selector with optional dot
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub dot: Option<Leaf>,
    pub selector: SimpleSelector,
}

/// Binder (for attribute sets and let bindings)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binder {
    /// `inherit (from) attrs ;`
    Inherit {
        kw: Leaf,
        from: Option<Term>,
        attrs: Vec<SimpleSelector>,
        semi: Leaf,
    },
    /// `path = value ;`
    Assignment {
        path: Vec<Selector>,
        eq: Leaf,
        value: Expression,
        semi: Leaf,
    },
}

/// `or` default clause on a `Selection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetDefault {
    pub or_kw: Leaf,
    pub value: Box<Term>,
}

/// Terms (atoms)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Token(Leaf),
    SimpleString(NixString),
    IndentedString(NixString),
    Path(Path),
    /// `[ items ]`
    List {
        open: Leaf,
        items: Items<Self>,
        close: Leaf,
    },
    /// `{ items }`, `rec { items }`, `let { items }`
    Set {
        rec: Option<Leaf>,
        open: Leaf,
        items: Items<Binder>,
        close: Leaf,
    },
    /// `base.selector1.selector2` with optional `or default`
    Selection {
        base: Box<Self>,
        selectors: Vec<Selector>,
        default: Option<SetDefault>,
    },
    /// `( expr )`
    Parenthesized {
        open: Leaf,
        expr: Box<Expression>,
        close: Leaf,
    },
}

/// `? expr` default clause on a function parameter attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDefault {
    pub question: Leaf,
    pub value: Expression,
}

/// Parameter attribute
#[derive(Debug, Clone, PartialEq, Eq)]
// `Attr` is intentionally larger than `Ellipsis`; pattern lists use `Attr`
// for almost every entry, so the boxing the lint suggests is a pessimisation.
#[allow(clippy::large_enum_variant)]
pub enum ParamAttr {
    /// `name (? default) (,)`
    Attr {
        name: Leaf,
        default: Option<ParamDefault>,
        comma: Option<Leaf>,
    },
    Ellipsis(Leaf),
}

/// Lambda parameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parameter {
    Id(Leaf),
    Set {
        open: Leaf,
        attrs: Vec<ParamAttr>,
        close: Leaf,
    },
    /// `a @ b` or `a @ { b }`
    Context {
        lhs: Box<Self>,
        at: Leaf,
        rhs: Box<Self>,
    },
}

/// Expressions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Term(Term),
    /// `with scope ; body`
    With {
        kw_with: Leaf,
        scope: Box<Self>,
        semi: Leaf,
        body: Box<Self>,
    },
    /// `let bindings in body`
    Let {
        kw_let: Leaf,
        bindings: Items<Binder>,
        kw_in: Leaf,
        body: Box<Self>,
    },
    /// `assert cond ; body`
    Assert {
        kw_assert: Leaf,
        cond: Box<Self>,
        semi: Leaf,
        body: Box<Self>,
    },
    /// `if cond then ... else ...`
    If {
        kw_if: Leaf,
        cond: Box<Self>,
        kw_then: Leaf,
        then_branch: Box<Self>,
        kw_else: Leaf,
        else_branch: Box<Self>,
    },
    /// `param : body`
    Abstraction {
        param: Parameter,
        colon: Leaf,
        body: Box<Self>,
    },
    /// function application
    Application {
        func: Box<Self>,
        arg: Box<Self>,
    },
    /// binary operation
    Operation {
        lhs: Box<Self>,
        op: Leaf,
        rhs: Box<Self>,
    },
    /// `lhs ? path`
    MemberCheck {
        lhs: Box<Self>,
        question: Leaf,
        path: Vec<Selector>,
    },
    /// `- expr` (negation)
    Negation {
        minus: Leaf,
        expr: Box<Self>,
    },
    /// `! expr` (boolean inversion)
    Inversion {
        bang: Leaf,
        expr: Box<Self>,
    },
}

/// Whole - an expression including final trivia
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whole<T> {
    pub value: T,
    pub trailing_trivia: Trivia,
}

/// A complete source file: top-level expression plus trailing trivia (Haskell: `File`).
pub type File = Whole<Expression>;

/// Tokens
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Integer(TokenText),
    Float(TokenText),
    Identifier(TokenText),
    EnvPath(TokenText),

    // Keywords
    KAssert,
    KElse,
    KIf,
    KIn,
    KInherit,
    KLet,
    KOr,
    KRec,
    KThen,
    KWith,

    // Delimiters
    TBraceOpen,
    TBraceClose,
    TBrackOpen,
    TBrackClose,
    TInterOpen,  // ${
    TInterClose, // }
    TParenOpen,
    TParenClose,

    // Operators
    TAssign,            // =
    TAt,                // @
    TColon,             // :
    TComma,             // ,
    TDot,               // .
    TDoubleQuote,       // "
    TDoubleSingleQuote, // ''
    TEllipsis,          // ...
    TQuestion,          // ?
    TSemicolon,         // ;
    TConcat,            // ++
    TNegate,            // - (as operator)
    TUpdate,            // //
    TPlus,              // +
    TMinus,             // -
    TMul,               // *
    TDiv,               // /
    TAnd,               // &&
    TOr,                // ||
    TEqual,             // ==
    TGreater,           // >
    TGreaterEqual,      // >=
    TImplies,           // ->
    TLess,              // <
    TLessEqual,         // <=
    TNot,               // !
    TUnequal,           // !=
    TPipeForward,       // |>
    TPipeBackward,      // <|

    Sof,    // Start of file
    TTilde, // ~ (for paths)
}

impl Token {
    /// Source text for keyword / operator tokens (Haskell: `tokenText`).
    pub fn text(&self) -> &str {
        match self {
            Self::Identifier(s) | Self::Integer(s) | Self::Float(s) | Self::EnvPath(s) => {
                s.as_str()
            }
            Self::KAssert => "assert",
            Self::KElse => "else",
            Self::KIf => "if",
            Self::KIn => "in",
            Self::KInherit => "inherit",
            Self::KLet => "let",
            Self::KOr => "or",
            Self::KRec => "rec",
            Self::KThen => "then",
            Self::KWith => "with",
            Self::TBraceOpen => "{",
            Self::TBraceClose | Self::TInterClose => "}",
            Self::TBrackOpen => "[",
            Self::TBrackClose => "]",
            Self::TInterOpen => "${",
            Self::TParenOpen => "(",
            Self::TParenClose => ")",
            Self::TAssign => "=",
            Self::TAt => "@",
            Self::TColon => ":",
            Self::TComma => ",",
            Self::TDot => ".",
            Self::TDoubleQuote => "\"",
            Self::TDoubleSingleQuote => "''",
            Self::TEllipsis => "...",
            Self::TQuestion => "?",
            Self::TSemicolon => ";",
            Self::TPlus => "+",
            Self::TMinus | Self::TNegate => "-",
            Self::TMul => "*",
            Self::TDiv => "/",
            Self::TConcat => "++",
            Self::TUpdate => "//",
            Self::TAnd => "&&",
            Self::TOr => "||",
            Self::TEqual => "==",
            Self::TGreater => ">",
            Self::TGreaterEqual => ">=",
            Self::TImplies => "->",
            Self::TLess => "<",
            Self::TLessEqual => "<=",
            Self::TNot => "!",
            Self::TUnequal => "!=",
            Self::TPipeForward => "|>",
            Self::TPipeBackward => "<|",
            Self::Sof => "end of file",
            Self::TTilde => "~",
        }
    }
}

impl Token {
    /// Check if this is an update, concat, or plus operator (for special formatting)
    pub const fn is_update_concat_plus(&self) -> bool {
        matches!(self, Self::TUpdate | Self::TConcat | Self::TPlus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivia_is_two_words() {
        // Guard against accidentally regressing to a fatter representation;
        // every Ann<T> in the AST embeds one of these.
        assert_eq!(std::mem::size_of::<Trivia>(), 16);
    }
}
