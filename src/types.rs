//! AST types matching nixfmt Haskell's Types.hs

/// A byte offset range in the source with line information
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,      // byte offset
    pub end: usize,        // byte offset
    pub start_line: usize, // line number (1-indexed)
    pub end_line: usize,   // line number (1-indexed)
}

impl Span {
    /// Create a span from byte offsets, with line numbers defaulting to 1.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Create a new span with line information
    pub fn with_lines(start: usize, end: usize, start_line: usize, end_line: usize) -> Self {
        Self {
            start,
            end,
            start_line,
            end_line,
        }
    }

    /// Create a zero-length span at the given offset
    pub fn point(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
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
    /// BlockComment(is_doc, lines)
    /// is_doc = true for /** */ comments
    BlockComment(bool, Vec<String>),
    LanguageAnnotation(String),
}

/// Wrapper around a list of trivia items (comments/whitespace)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trivia(pub Vec<Trivium>);

impl Trivia {
    /// Empty trivia list.
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl std::ops::Deref for Trivia {
    type Target = Vec<Trivium>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Trivia {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<Trivium>> for Trivia {
    fn from(value: Vec<Trivium>) -> Self {
        Self(value)
    }
}

impl From<Trivia> for Vec<Trivium> {
    fn from(val: Trivia) -> Self {
        val.0
    }
}

impl IntoIterator for Trivia {
    type Item = Trivium;
    type IntoIter = std::vec::IntoIter<Trivium>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Trivia {
    type Item = &'a Trivium;
    type IntoIter = std::slice::Iter<'a, Trivium>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Trivia {
    type Item = &'a mut Trivium;
    type IntoIter = std::slice::IterMut<'a, Trivium>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

/// Trailing comment on same line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingComment(pub String);

/// Annotated wrapper - every AST node has:
/// - pre_trivia: Comments/whitespace before the token
/// - span: Byte range in source
/// - value: The actual value
/// - trail_comment: Optional trailing comment on same line
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
    Integer(String),
    Float(String),
    Identifier(String),
    EnvPath(String),

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
