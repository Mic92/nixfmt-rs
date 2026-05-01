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
    pub start: u32,      // byte offset
    pub end: u32,        // byte offset
    pub start_line: u32, // line number (1-indexed)
    pub end_line: u32,   // line number (1-indexed)
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
}

/// Trivia - comments and whitespace
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trivium {
    EmptyLine(),
    LineComment(String),
    /// `BlockComment(is_doc`, lines)
    /// `is_doc` = true for /** */ comments
    BlockComment(bool, Vec<String>),
    LanguageAnnotation(String),
}

/// Wrapper around a list of trivia items (comments/whitespace).
///
/// Stored as a boxed `Vec` behind an `Option` so the overwhelmingly common
/// empty case is a single null word: every `Ann<T>` carries one of these, and
/// the parser moves `Ann` values by value through every production, so the
/// 24→16 byte saving compounds across the whole AST.
#[derive(Debug, Clone, Default)]
#[allow(clippy::box_collection)] // intentional: Option<Box<Vec>> is 8 bytes, Vec is 24
pub struct Trivia(Option<Box<Vec<Trivium>>>);

impl Trivia {
    /// Empty trivia list (no allocation).
    #[inline]
    pub const fn new() -> Self {
        Self(None)
    }

    /// Single-element trivia list.
    pub fn one(t: Trivium) -> Self {
        Self(Some(Box::new(vec![t])))
    }

    #[inline]
    fn vec_mut(&mut self) -> &mut Vec<Trivium> {
        self.0.get_or_insert_with(|| Box::new(Vec::new()))
    }

    /// Append a trivium, allocating storage on first use.
    #[inline]
    pub fn push(&mut self, t: Trivium) {
        self.vec_mut().push(t);
    }

    /// Insert at `idx`, allocating storage on first use.
    pub fn insert(&mut self, idx: usize, t: Trivium) {
        self.vec_mut().insert(idx, t);
    }

    /// Append all items from `iter`, allocating storage only if it yields any.
    pub fn extend<I: IntoIterator<Item = Trivium>>(&mut self, iter: I) {
        let mut iter = iter.into_iter();
        if let Some(first) = iter.next() {
            let v = self.vec_mut();
            v.push(first);
            v.extend(iter);
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
            Self(Some(Box::new(value)))
        }
    }
}

impl From<Trivia> for Vec<Trivium> {
    fn from(val: Trivia) -> Self {
        val.0.map(|b| *b).unwrap_or_default()
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

impl<T> Ann<T> {
    /// Wrap a value with a span and no surrounding trivia.
    pub const fn new(value: T, span: Span) -> Self {
        Self {
            pre_trivia: Trivia::new(),
            span,
            value,
            trail_comment: None,
        }
    }
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

    pub fn bare(&self) -> Self {
        Self {
            pre_trivia: Trivia::new(),
            trail_comment: None,
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

impl FirstToken for Term {
    fn first_token(&self) -> AnnSlot<'_> {
        match self {
            Self::Token(l) => l.into(),
            Self::SimpleString(s) | Self::IndentedString(s) => s.into(),
            Self::Path(p) => p.into(),
            Self::List(open, _, _)
            | Self::Set(None, open, _, _)
            | Self::Parenthesized(open, _, _) => open.into(),
            Self::Set(Some(rec), _, _, _) => rec.into(),
            Self::Selection(inner, _, _) => inner.first_token(),
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Self::Token(l) => l.into(),
            Self::SimpleString(s) | Self::IndentedString(s) => s.into(),
            Self::Path(p) => p.into(),
            Self::List(open, _, _)
            | Self::Set(None, open, _, _)
            | Self::Parenthesized(open, _, _) => open.into(),
            Self::Set(Some(rec), _, _, _) => rec.into(),
            Self::Selection(inner, _, _) => inner.first_token_mut(),
        }
    }
}

impl FirstToken for Parameter {
    fn first_token(&self) -> AnnSlot<'_> {
        match self {
            Self::ID(n) => n.into(),
            Self::Set(open, _, _) => open.into(),
            Self::Context(first, _, _) => first.first_token(),
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Self::ID(n) => n.into(),
            Self::Set(open, _, _) => open.into(),
            Self::Context(first, _, _) => first.first_token_mut(),
        }
    }
}

impl FirstToken for Expression {
    fn first_token(&self) -> AnnSlot<'_> {
        match self {
            Self::Term(t) => t.first_token(),
            Self::With(kw, ..)
            | Self::Let(kw, ..)
            | Self::Assert(kw, ..)
            | Self::If(kw, ..)
            | Self::Negation(kw, _)
            | Self::Inversion(kw, _) => kw.into(),
            Self::Abstraction(p, _, _) => p.first_token(),
            Self::Application(g, _) | Self::Operation(g, _, _) | Self::MemberCheck(g, _, _) => {
                g.first_token()
            }
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Self::Term(t) => t.first_token_mut(),
            Self::With(kw, ..)
            | Self::Let(kw, ..)
            | Self::Assert(kw, ..)
            | Self::If(kw, ..)
            | Self::Negation(kw, _)
            | Self::Inversion(kw, _) => kw.into(),
            Self::Abstraction(p, _, _) => p.first_token_mut(),
            Self::Application(g, _) | Self::Operation(g, _, _) | Self::MemberCheck(g, _, _) => {
                g.first_token_mut()
            }
        }
    }
}

/// Haskell `convertTrailing`.
impl From<&TrailingComment> for Trivium {
    fn from(tc: &TrailingComment) -> Self {
        Self::LineComment(format!(" {}", tc.0))
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
    TextPart(String),
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
    /// inherit keyword, optional (from), selectors, semicolon
    Inherit(Leaf, Option<Term>, Vec<SimpleSelector>, Leaf),
    /// selectors = expr ;
    Assignment(Vec<Selector>, Leaf, Expression, Leaf),
}

/// Terms (atoms)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Token(Leaf),
    SimpleString(NixString),
    IndentedString(NixString),
    Path(Path),
    /// [ items ]
    List(Leaf, Items<Self>, Leaf),
    /// { items } or rec { items } or let { items }
    Set(Option<Leaf>, Leaf, Items<Binder>, Leaf),
    /// term.selector1.selector2 or term.selector or term
    Selection(Box<Self>, Vec<Selector>, Option<(Leaf, Box<Self>)>),
    /// ( expr )
    Parenthesized(Leaf, Box<Expression>, Leaf),
}

/// Parameter attribute
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamAttr {
    /// name, optional (? default), optional comma
    ParamAttr(Leaf, Box<Option<(Leaf, Expression)>>, Option<Leaf>),
    ParamEllipsis(Leaf),
}

/// Lambda parameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parameter {
    ID(Leaf),
    Set(Leaf, Vec<ParamAttr>, Leaf),
    /// a @ b or a @ { b }
    Context(Box<Self>, Leaf, Box<Self>),
}

/// Expressions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Term(Term),
    /// with expr ; expr
    With(Leaf, Box<Self>, Leaf, Box<Self>),
    /// let bindings in expr
    Let(Leaf, Items<Binder>, Leaf, Box<Self>),
    /// assert expr ; expr
    Assert(Leaf, Box<Self>, Leaf, Box<Self>),
    /// if expr then expr else expr
    If(Leaf, Box<Self>, Leaf, Box<Self>, Leaf, Box<Self>),
    /// param : body
    Abstraction(Parameter, Leaf, Box<Self>),
    /// function application
    Application(Box<Self>, Box<Self>),
    /// Binary operation
    Operation(Box<Self>, Leaf, Box<Self>),
    /// expr ? selector
    MemberCheck(Box<Self>, Leaf, Vec<Selector>),
    /// - expr (negation)
    Negation(Leaf, Box<Self>),
    /// ! expr (boolean inversion)
    Inversion(Leaf, Box<Self>),
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
