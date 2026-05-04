//! The Nix expression grammar: terms, expressions, binders, parameters and
//! the string/selector substructures they reference.

use super::annotated::{Annotated, Trailed};
use super::items::Items;
use super::token::{Leaf, Token};

/// String parts - either text or interpolation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    TextPart(Box<str>),
    Interpolation(Box<Trailed<Expression>>),
}

/// A path literal: a single line of text / interpolation parts (Haskell: `Path`).
pub type Path = Annotated<Vec<StringPart>>;

/// A string consists of lines, each of which consists of text elements and interpolations
pub type NixString = Annotated<Vec<Vec<StringPart>>>;

/// Simple selector (no dot prefix)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    ID(Leaf),
    Interpol(Annotated<StringPart>),
    String(NixString),
}

/// Selector with optional dot
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub dot: Option<Leaf>,
    pub selector: SimpleSelector,
}

impl Selector {
    /// Haskell `isSimpleSelector` (Pretty.hs).
    pub const fn is_simple(&self) -> bool {
        matches!(self.selector, SimpleSelector::ID(_))
    }
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

impl Term {
    /// Haskell `isSimple` (Pretty.hs), `Term` arm; split out so list items can be
    /// classified without wrapping them in an `Expression`.
    pub fn is_simple(&self) -> bool {
        match self {
            Self::SimpleString(s) | Self::IndentedString(s) => !s.has_trivia(),
            Self::Path(p) => !p.has_trivia(),
            Self::Token(leaf)
                if !leaf.has_trivia()
                    && matches!(
                        leaf.value,
                        Token::Identifier(_)
                            | Token::Integer(_)
                            | Token::Float(_)
                            | Token::EnvPath(_)
                    ) =>
            {
                true
            }
            Self::Selection {
                base,
                selectors,
                default,
            } => base.is_simple() && selectors.iter().all(Selector::is_simple) && default.is_none(),
            Self::Parenthesized { open, expr, close } => {
                !open.has_trivia() && !close.has_trivia() && expr.is_simple()
            }
            _ => false,
        }
    }
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

impl ParamAttr {
    pub const fn has_no_default(&self) -> bool {
        matches!(self, Self::Attr { default, .. } if default.is_none())
    }

    pub const fn is_ellipsis(&self) -> bool {
        matches!(self, Self::Ellipsis(_))
    }
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
    Lambda {
        param: Parameter,
        colon: Leaf,
        body: Box<Self>,
    },
    /// function application
    Apply {
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

impl Expression {
    /// Haskell `isSimple` (Pretty.hs).
    pub fn is_simple(&self) -> bool {
        match self {
            Self::Term(term) => term.is_simple(),
            Self::Apply { func: f, arg: a } => Self::app_is_simple(f, a),
            _ => false,
        }
    }

    pub fn app_is_simple(f: &Self, a: &Self) -> bool {
        // No more than two arguments.
        if let Self::Apply { func: f2, .. } = f
            && matches!(**f2, Self::Apply { .. })
        {
            return false;
        }
        f.is_simple() && a.is_simple()
    }
}

/// A complete source file: top-level expression plus trailing trivia (Haskell: `File`).
pub type File = Trailed<Expression>;

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
    Self::Lambda { param, .. } => recurse param,
    Self::Apply { func: g, .. }
    | Self::Operation { lhs: g, .. }
    | Self::MemberCheck { lhs: g, .. } => recurse g,
}
