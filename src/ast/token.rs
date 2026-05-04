use super::annotated::Annotated;
use super::span::TokenText;

/// A token annotated with trivia and span (Haskell: `Leaf`).
pub type Leaf = Annotated<Token>;

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

    /// Check if this is an update, concat, or plus operator (for special formatting)
    pub const fn is_update_concat_plus(&self) -> bool {
        matches!(self, Self::TUpdate | Self::TConcat | Self::TPlus)
    }
}
