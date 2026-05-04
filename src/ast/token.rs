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
    Assert,
    Else,
    If,
    In,
    Inherit,
    Let,
    OrDefault,
    Rec,
    Then,
    With,

    // Delimiters
    BraceOpen,
    BraceClose,
    BrackOpen,
    BrackClose,
    InterOpen,  // ${
    InterClose, // }
    ParenOpen,
    ParenClose,

    // Operators
    Assign,            // =
    At,                // @
    Colon,             // :
    Comma,             // ,
    Dot,               // .
    DoubleQuote,       // "
    DoubleSingleQuote, // ''
    Ellipsis,          // ...
    Question,          // ?
    Semicolon,         // ;
    Concat,            // ++
    Negate,            // - (as operator)
    Update,            // //
    Plus,              // +
    Minus,             // -
    Mul,               // *
    Div,               // /
    And,               // &&
    Or,                // ||
    Equal,             // ==
    Greater,           // >
    GreaterEqual,      // >=
    Implies,           // ->
    Less,              // <
    LessEqual,         // <=
    Not,               // !
    Unequal,           // !=
    PipeForward,       // |>
    PipeBackward,      // <|

    Sof,   // Start of file
    Tilde, // ~ (for paths)
}

impl Token {
    /// Source text for keyword / operator tokens (Haskell: `tokenText`).
    pub fn text(&self) -> &str {
        match self {
            Self::Identifier(s) | Self::Integer(s) | Self::Float(s) | Self::EnvPath(s) => {
                s.as_str()
            }
            Self::Assert => "assert",
            Self::Else => "else",
            Self::If => "if",
            Self::In => "in",
            Self::Inherit => "inherit",
            Self::Let => "let",
            Self::OrDefault => "or",
            Self::Rec => "rec",
            Self::Then => "then",
            Self::With => "with",
            Self::BraceOpen => "{",
            Self::BraceClose | Self::InterClose => "}",
            Self::BrackOpen => "[",
            Self::BrackClose => "]",
            Self::InterOpen => "${",
            Self::ParenOpen => "(",
            Self::ParenClose => ")",
            Self::Assign => "=",
            Self::At => "@",
            Self::Colon => ":",
            Self::Comma => ",",
            Self::Dot => ".",
            Self::DoubleQuote => "\"",
            Self::DoubleSingleQuote => "''",
            Self::Ellipsis => "...",
            Self::Question => "?",
            Self::Semicolon => ";",
            Self::Plus => "+",
            Self::Minus | Self::Negate => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Concat => "++",
            Self::Update => "//",
            Self::And => "&&",
            Self::Or => "||",
            Self::Equal => "==",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Implies => "->",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Not => "!",
            Self::Unequal => "!=",
            Self::PipeForward => "|>",
            Self::PipeBackward => "<|",
            Self::Sof => "end of file",
            Self::Tilde => "~",
        }
    }

    /// Check if this is an update, concat, or plus operator (for special formatting)
    pub const fn is_update_concat_plus(&self) -> bool {
        matches!(self, Self::Update | Self::Concat | Self::Plus)
    }
}
