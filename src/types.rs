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
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Create a new span with line information
    pub fn with_lines(start: usize, end: usize, start_line: usize, end_line: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: start_line as u32,
            end_line: end_line as u32,
        }
    }

    /// Create a zero-length span at the given offset
    pub fn point(offset: usize) -> Self {
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
    pub fn new() -> Self {
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
    pub fn new(value: T, span: Span) -> Self {
        Ann {
            pre_trivia: Trivia::new(),
            span,
            value,
            trail_comment: None,
        }
    }
}

impl<T: Clone> Ann<T> {
    pub fn without_trail(&self) -> Self {
        Ann {
            trail_comment: None,
            ..self.clone()
        }
    }

    pub fn without_pre(&self) -> Self {
        Ann {
            pre_trivia: Trivia::new(),
            ..self.clone()
        }
    }

    pub fn bare(&self) -> Self {
        Ann {
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
            Term::Token(l) => l.into(),
            Term::SimpleString(s) | Term::IndentedString(s) => s.into(),
            Term::Path(p) => p.into(),
            Term::List(open, _, _)
            | Term::Set(None, open, _, _)
            | Term::Parenthesized(open, _, _) => open.into(),
            Term::Set(Some(rec), _, _, _) => rec.into(),
            Term::Selection(inner, _, _) => inner.first_token(),
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Term::Token(l) => l.into(),
            Term::SimpleString(s) | Term::IndentedString(s) => s.into(),
            Term::Path(p) => p.into(),
            Term::List(open, _, _)
            | Term::Set(None, open, _, _)
            | Term::Parenthesized(open, _, _) => open.into(),
            Term::Set(Some(rec), _, _, _) => rec.into(),
            Term::Selection(inner, _, _) => inner.first_token_mut(),
        }
    }
}

impl FirstToken for Parameter {
    fn first_token(&self) -> AnnSlot<'_> {
        match self {
            Parameter::ID(n) => n.into(),
            Parameter::Set(open, _, _) => open.into(),
            Parameter::Context(first, _, _) => first.first_token(),
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Parameter::ID(n) => n.into(),
            Parameter::Set(open, _, _) => open.into(),
            Parameter::Context(first, _, _) => first.first_token_mut(),
        }
    }
}

impl FirstToken for Expression {
    fn first_token(&self) -> AnnSlot<'_> {
        match self {
            Expression::Term(t) => t.first_token(),
            Expression::With(kw, ..)
            | Expression::Let(kw, ..)
            | Expression::Assert(kw, ..)
            | Expression::If(kw, ..)
            | Expression::Negation(kw, _)
            | Expression::Inversion(kw, _) => kw.into(),
            Expression::Abstraction(p, _, _) => p.first_token(),
            Expression::Application(g, _)
            | Expression::Operation(g, _, _)
            | Expression::MemberCheck(g, _, _) => g.first_token(),
        }
    }
    fn first_token_mut(&mut self) -> AnnSlotMut<'_> {
        match self {
            Expression::Term(t) => t.first_token_mut(),
            Expression::With(kw, ..)
            | Expression::Let(kw, ..)
            | Expression::Assert(kw, ..)
            | Expression::If(kw, ..)
            | Expression::Negation(kw, _)
            | Expression::Inversion(kw, _) => kw.into(),
            Expression::Abstraction(p, _, _) => p.first_token_mut(),
            Expression::Application(g, _)
            | Expression::Operation(g, _, _)
            | Expression::MemberCheck(g, _, _) => g.first_token_mut(),
        }
    }
}

/// Haskell `convertTrailing`.
impl From<&TrailingComment> for Trivium {
    fn from(tc: &TrailingComment) -> Self {
        Trivium::LineComment(format!(" {}", tc.0))
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
    List(Leaf, Items<Term>, Leaf),
    /// { items } or rec { items } or let { items }
    Set(Option<Leaf>, Leaf, Items<Binder>, Leaf),
    /// term.selector1.selector2 or term.selector or term
    Selection(Box<Term>, Vec<Selector>, Option<(Leaf, Box<Term>)>),
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
    Context(Box<Parameter>, Leaf, Box<Parameter>),
}

/// Expressions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Term(Term),
    /// with expr ; expr
    With(Leaf, Box<Expression>, Leaf, Box<Expression>),
    /// let bindings in expr
    Let(Leaf, Items<Binder>, Leaf, Box<Expression>),
    /// assert expr ; expr
    Assert(Leaf, Box<Expression>, Leaf, Box<Expression>),
    /// if expr then expr else expr
    If(
        Leaf,
        Box<Expression>,
        Leaf,
        Box<Expression>,
        Leaf,
        Box<Expression>,
    ),
    /// param : body
    Abstraction(Parameter, Leaf, Box<Expression>),
    /// function application
    Application(Box<Expression>, Box<Expression>),
    /// Binary operation
    Operation(Box<Expression>, Leaf, Box<Expression>),
    /// expr ? selector
    MemberCheck(Box<Expression>, Leaf, Vec<Selector>),
    /// - expr (negation)
    Negation(Leaf, Box<Expression>),
    /// ! expr (boolean inversion)
    Inversion(Leaf, Box<Expression>),
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
            Token::KAssert => "assert",
            Token::KElse => "else",
            Token::KIf => "if",
            Token::KIn => "in",
            Token::KInherit => "inherit",
            Token::KLet => "let",
            Token::KOr => "or",
            Token::KRec => "rec",
            Token::KThen => "then",
            Token::KWith => "with",
            Token::TBraceOpen => "{",
            Token::TBraceClose => "}",
            Token::TBrackOpen => "[",
            Token::TBrackClose => "]",
            Token::TInterOpen => "${",
            Token::TInterClose => "}",
            Token::TParenOpen => "(",
            Token::TParenClose => ")",
            Token::TAssign => "=",
            Token::TAt => "@",
            Token::TColon => ":",
            Token::TComma => ",",
            Token::TDot => ".",
            Token::TDoubleQuote => "\"",
            Token::TDoubleSingleQuote => "''",
            Token::TEllipsis => "...",
            Token::TQuestion => "?",
            Token::TSemicolon => ";",
            Token::TPlus => "+",
            Token::TMinus => "-",
            Token::TMul => "*",
            Token::TDiv => "/",
            Token::TConcat => "++",
            Token::TNegate => "-",
            Token::TUpdate => "//",
            Token::TAnd => "&&",
            Token::TOr => "||",
            Token::TEqual => "==",
            Token::TGreater => ">",
            Token::TGreaterEqual => ">=",
            Token::TImplies => "->",
            Token::TLess => "<",
            Token::TLessEqual => "<=",
            Token::TNot => "!",
            Token::TUnequal => "!=",
            Token::TPipeForward => "|>",
            Token::TPipeBackward => "<|",
            Token::Sof => "end of file",
            _ => "",
        }
    }
}

impl Token {
    /// Check if this is an update, concat, or plus operator (for special formatting)
    pub fn is_update_concat_plus(&self) -> bool {
        matches!(self, Token::TUpdate | Token::TConcat | Token::TPlus)
    }
}
