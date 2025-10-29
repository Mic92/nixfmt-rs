//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs

use crate::predoc::*;
use crate::types::*;

// Helper functions

/// Check if a term is absorbable (can stay on same line after =)
/// Based on Haskell isAbsorbable (Pretty.hs:581-595)
fn is_absorbable_term(term: &Term) -> bool {
    match term {
        // Multi-line indented string
        Term::IndentedString(s) if s.value.len() >= 2 => true,
        // Paths are not absorbable
        Term::Path(_) => false,
        // Non-empty sets and lists
        Term::Set(_, _, items, _) if !items.0.is_empty() => true,
        Term::List(_, items, _) if !items.0.is_empty() => true,
        // Empty sets/lists - always absorbable for now (TODO: check line breaks)
        Term::Set(_, _, items, _) if items.0.is_empty() => true,
        Term::List(_, items, _) if items.0.is_empty() => true,
        // Parenthesized absorbable terms
        Term::Parenthesized(_, expr, _) => is_absorbable_expr(expr),
        _ => false,
    }
}

/// Check if an expression is absorbable
/// Based on Haskell isAbsorbableExpr (Pretty.hs:572-579)
fn is_absorbable_expr(expr: &Expression) -> bool {
    match expr {
        Expression::Term(t) => is_absorbable_term(t),
        // with expr; term where term is absorbable
        Expression::With(_, _, _, inner) => {
            if let Expression::Term(t) = &**inner {
                is_absorbable_term(t)
            } else {
                false
            }
        }
        // Simple lambda with absorbable body: x: { }
        Expression::Abstraction(Parameter::IDParameter(_), _, body) => match &**body {
            Expression::Term(t) => is_absorbable_term(t),
            Expression::Abstraction(_, _, _) => is_absorbable_expr(body),
            _ => false,
        },
        Expression::Abstraction(_, _, _) => false,
        _ => false,
    }
}

/// Format the right-hand side of an assignment with absorption rules
/// Based on Haskell absorbRHS (Pretty.hs:631-658)
fn absorb_rhs(expr: &Expression) -> Doc {
    match expr {
        // Special case: set with single inherit
        Expression::Term(Term::Set(_, _, binders, _)) => {
            if binders.0.len() == 1 {
                if let Item::Item(Binder::Inherit(_, _, _, _)) = &binders.0[0] {
                    return nest({
                        let mut doc = vec![hardspace()];
                        doc.extend(group(expr.pretty()));
                        doc
                    });
                }
            }
            // Absorbable set: force expand
            if is_absorbable_expr(expr) {
                return nest({
                    let mut doc = vec![hardspace()];
                    doc.extend(group(expr.pretty()));
                    doc
                });
            }
            // Non-absorbable: new line
            nest({
                let mut doc = vec![line()];
                doc.extend(group(expr.pretty()));
                doc
            })
        }
        // Absorbable expressions
        _ if is_absorbable_expr(expr) => nest({
            let mut doc = vec![hardspace()];
            doc.extend(group(expr.pretty()));
            doc
        }),
        // Parenthesized expressions
        Expression::Term(Term::Parenthesized(_, _, _)) => nest({
            let mut doc = vec![hardspace()];
            doc.extend(expr.pretty());
            doc
        }),
        // Strings and paths: always keep on same line
        Expression::Term(Term::SimpleString(_))
        | Expression::Term(Term::IndentedString(_))
        | Expression::Term(Term::Path(_)) => nest({
            let mut doc = vec![hardspace()];
            doc.extend(group(expr.pretty()));
            doc
        }),
        // Non-absorbable terms: start on new line
        Expression::Term(_) => nest({
            let mut doc = vec![line()];
            doc.extend(group(expr.pretty()));
            doc
        }),
        // Function application: try to absorb
        Expression::Application(_, _) => nest({
            let mut doc = vec![line()];
            doc.extend(expr.pretty());
            doc
        }),
        // Everything else: new line
        _ => nest({
            let mut doc = vec![line()];
            doc.extend(group(expr.pretty()));
            doc
        }),
    }
}

/// Calculate the display width of a text string (simple character count for now)
fn text_width(s: &str) -> usize {
    s.chars().count()
}

/// Check if a string contains only whitespace
fn is_spaces(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace())
}

// Pretty instances

impl Pretty for TrailingComment {
    fn pretty(&self) -> Doc {
        let mut doc = vec![hardspace()];
        doc.extend(trailing_comment(format!("# {}", self.0)));
        doc.push(hardline());
        doc
    }
}

impl Pretty for Trivium {
    fn pretty(&self) -> Doc {
        match self {
            Trivium::EmptyLine() => vec![emptyline()],
            Trivium::LineComment(c) => {
                let mut doc = comment(format!("#{}", c));
                doc.push(hardline());
                doc
            }
            Trivium::BlockComment(is_doc, lines) => {
                let mut doc = comment(if *is_doc { "/**" } else { "/*" });
                doc.push(hardline());
                // TODO: implement offset for block comment indentation
                for line in lines {
                    if line.is_empty() {
                        doc.push(emptyline());
                    } else {
                        doc.extend(comment(line));
                        doc.push(hardline());
                    }
                }
                doc.extend(comment("*/"));
                doc.push(hardline());
                doc
            }
            Trivium::LanguageAnnotation(lang) => {
                let mut doc = comment(format!("/* {} */", lang));
                doc.push(hardspace());
                doc
            }
        }
    }
}

impl Pretty for Trivia {
    fn pretty(&self) -> Doc {
        if self.0.is_empty() {
            return Vec::new();
        }

        // Special case: single language annotation renders inline
        if self.0.len() == 1 {
            if let Trivium::LanguageAnnotation(_) = &self.0[0] {
                return self.0[0].pretty();
            }
        }

        let mut doc = vec![hardline()];
        for trivium in &self.0 {
            doc.extend(trivium.pretty());
        }
        doc
    }
}

impl<T: Pretty> Pretty for Ann<T> {
    fn pretty(&self) -> Doc {
        let mut doc = self.pre_trivia.pretty();
        doc.extend(self.value.pretty());
        doc.extend(self.trail_comment.pretty());
        doc
    }
}

// Pretty for Item - wraps items in groups, passes through comments
impl<T: Pretty> Pretty for Item<T> {
    fn pretty(&self) -> Doc {
        match self {
            Item::Comments(trivia) => trivia.pretty(),
            Item::Item(x) => group(x.pretty()),
        }
    }
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
fn pretty_set(wide: bool, krec: &Option<Ann<Token>>, open: &Ann<Token>, items: &Items<Binder>, close: &Ann<Token>) -> Doc {
    // Empty attribute set
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.0.is_empty() {
        let mut doc = Vec::new();
        // Pretty print optional `rec` keyword with hardspace
        if let Some(rec) = krec {
            doc.extend(rec.pretty());
            doc.push(hardspace());
        }
        doc.extend(open.pretty());
        doc.push(hardspace());
        doc.extend(close.pretty());
        return doc;
    }

    // General set with items
    let mut doc = Vec::new();

    // Pretty print optional `rec` keyword with hardspace
    if let Some(rec) = krec {
        doc.extend(rec.pretty());
        doc.push(hardspace());
    }

    // Open brace without trailing comment
    let open_without_trail = Ann {
        pre_trivia: open.pre_trivia.clone(),
        span: open.span,
        trail_comment: None,
        value: open.value.clone(),
    };
    doc.extend(open_without_trail.pretty());

    // Separator: use hardline if wide and has items, else use line
    let sep = if wide && !items.0.is_empty() {
        vec![hardline()]
    } else {
        vec![line()]
    };

    doc.extend(surround_with(sep, nest({
        let mut inner = open.trail_comment.pretty();
        inner.extend(pretty_items(items));
        inner
    })));
    doc.extend(close.pretty());
    doc
}

/// Format a list of items with interleaved comments
/// Based on Haskell prettyItems (Pretty.hs:108-120)
fn pretty_items<T: Pretty>(items: &Items<T>) -> Doc {
    let items = &items.0;
    match items.as_slice() {
        [] => Vec::new(),
        [item] => item.pretty(),
        items => {
            let mut doc = Vec::new();
            let mut i = 0;
            while i < items.len() {
                if i > 0 {
                    doc.push(hardline());
                }

                // Special case: language annotation comment followed by string item
                if i + 1 < items.len() {
                    if let Item::Comments(trivia) = &items[i] {
                        if trivia.0.len() == 1 {
                            if let Trivium::LanguageAnnotation(lang) = &trivia.0[0] {
                                if let Item::Item(string_item) = &items[i + 1] {
                                    // Language annotation + string on same line
                                    doc.extend(Trivium::LanguageAnnotation(lang.clone()).pretty());
                                    doc.push(hardspace());
                                    doc.extend(group(string_item.pretty()));
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                    }
                }

                doc.extend(items[i].pretty());
                i += 1;
            }
            doc
        }
    }
}

impl Pretty for Binder {
    fn pretty(&self) -> Doc {
        match self {
            Binder::Inherit(inherit, source, ids, semicolon) => {
                let mut doc = inherit.pretty();

                // If there's a source expression like (foo), add it
                if let Some(src) = source {
                    doc.push(line());
                    doc.extend(group(src.pretty()));
                }

                // Add the identifiers
                if !ids.is_empty() {
                    doc.push(hardline());
                    doc.extend(sep_by(vec![hardline()], ids.clone()));
                }

                doc.push(hardline());
                doc.extend(semicolon.pretty());
                group(nest(doc))
            }
            Binder::Assignment(selectors, assign, expr, semicolon) => {
                let mut doc = hcat(selectors.clone());
                doc.push(hardspace());
                doc.extend(assign.pretty());
                doc.extend(absorb_rhs(expr));
                doc.extend(semicolon.pretty());
                group(doc)
            }
        }
    }
}

impl Pretty for Token {
    fn pretty(&self) -> Doc {
        use Token::*;
        let s = match self {
            Integer(s) => s.as_str(),
            Float(s) => s.as_str(),
            Identifier(s) => s.as_str(),
            EnvPath(s) => s.as_str(),
            KAssert => "assert",
            KElse => "else",
            KIf => "if",
            KIn => "in",
            KInherit => "inherit",
            KLet => "let",
            KOr => "or",
            KRec => "rec",
            KThen => "then",
            KWith => "with",
            TBraceOpen => "{",
            TBraceClose => "}",
            TBrackOpen => "[",
            TBrackClose => "]",
            TInterOpen => "${",
            TInterClose => "}",
            TParenOpen => "(",
            TParenClose => ")",
            TAssign => "=",
            TAt => "@",
            TColon => ":",
            TComma => ",",
            TDot => ".",
            TDoubleQuote => "\"",
            TDoubleSingleQuote => "''",
            TEllipsis => "...",
            TQuestion => "?",
            TSemicolon => ";",
            TConcat => "++",
            TNegate => "-",
            TUpdate => "//",
            TPlus => "+",
            TMinus => "-",
            TMul => "*",
            TDiv => "/",
            TAnd => "&&",
            TOr => "||",
            TEqual => "==",
            TGreater => ">",
            TGreaterEqual => ">=",
            TImplies => "->",
            TLess => "<",
            TLessEqual => "<=",
            TNot => "!",
            TUnequal => "!=",
            TPipeForward => "|>",
            TPipeBackward => "<|",
            SOF => "",
            TTilde => "~",
        };
        text(s)
    }
}

impl Pretty for SimpleSelector {
    fn pretty(&self) -> Doc {
        match self {
            SimpleSelector::IDSelector(id) => id.pretty(),
            SimpleSelector::StringSelector(ann) => {
                let mut doc = ann.pre_trivia.pretty();
                // TODO: implement prettySimpleString
                doc.extend(text("\"...\""));
                doc.extend(ann.trail_comment.pretty());
                doc
            }
            SimpleSelector::InterpolSelector(interp) => interp.pretty(),
        }
    }
}

impl Pretty for Selector {
    fn pretty(&self) -> Doc {
        let mut doc = Vec::new();
        if let Some(dot) = &self.dot {
            doc.extend(dot.pretty());
        }
        doc.extend(self.selector.pretty());
        doc
    }
}

impl Pretty for StringPart {
    fn pretty(&self) -> Doc {
        match self {
            StringPart::TextPart(s) => text(s),
            StringPart::Interpolation(whole) => {
                // For now, use a simple approach
                // TODO: implement absorption and isSimple checks
                let inner = {
                    let mut doc = vec![line_prime()];
                    doc.extend(group(whole.value.clone()));
                    doc.push(line_prime());
                    doc
                };
                let mut doc = text("${");
                doc.extend(group_ann(GroupAnn::RegularG, nest(inner)));
                doc.extend(text("}"));
                doc
            }
        }
    }
}

impl Pretty for Vec<StringPart> {
    fn pretty(&self) -> Doc {
        // Handle special case: single interpolation with leading whitespace
        if self.len() == 2 {
            if let (StringPart::TextPart(pre), StringPart::Interpolation(whole)) =
                (&self[0], &self[1])
            {
                if is_spaces(pre) && whole.trailing_trivia.0.is_empty() {
                    let indentation = text_width(pre);
                    let mut doc = text(pre);
                    let inner = {
                        let mut doc = vec![line_prime()];
                        doc.extend(group(whole.value.clone()));
                        doc.push(line_prime());
                        doc
                    };
                    doc.extend(offset(
                        indentation,
                        group_ann(GroupAnn::RegularG, {
                            let mut d = text("${");
                            d.extend(nest(inner));
                            d.extend(text("}"));
                            d
                        }),
                    ));
                    return doc;
                }
            }
        }

        // Handle leading TextPart with offset
        if !self.is_empty() {
            if let StringPart::TextPart(t) = &self[0] {
                let indentation = text_width(
                    &t.chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>(),
                );
                let mut doc = text(t);
                doc.extend(offset(indentation, hcat(self[1..].to_vec())));
                return doc;
            }
        }

        // Default: just concatenate
        hcat(self.clone())
    }
}

/// Format a simple string (with double quotes)
fn pretty_simple_string(parts: &[Vec<StringPart>]) -> Doc {
    let mut doc = text("\"");
    // Use literal \n instead of newline() to avoid indentation
    doc.extend(sep_by(text("\n"), parts.to_vec()));
    doc.extend(text("\""));
    group(doc)
}

/// Format an indented string (with '')
fn pretty_indented_string(parts: &[Vec<StringPart>]) -> Doc {
    let mut doc = text("''");
    // For multi-line strings, add a potential line break after opening ''
    if parts.len() > 1 {
        doc.push(line_prime());
    }
    doc.extend(nest(sep_by(vec![newline()], parts.to_vec())));
    doc.extend(text("''"));
    group(doc)
}

impl Pretty for Term {
    fn pretty(&self) -> Doc {
        match self {
            Term::Token(t) => t.pretty(),
            Term::SimpleString(s) => {
                let mut doc = s.pre_trivia.pretty();
                doc.extend(pretty_simple_string(&s.value));
                doc.extend(s.trail_comment.pretty());
                doc
            }
            Term::IndentedString(s) => {
                let mut doc = s.pre_trivia.pretty();
                doc.extend(pretty_indented_string(&s.value));
                doc.extend(s.trail_comment.pretty());
                doc
            }
            Term::Path(p) => {
                // Path is Ann<Vec<StringPart>>
                let mut doc = p.pre_trivia.pretty();
                for part in &p.value {
                    doc.extend(part.pretty());
                }
                doc.extend(p.trail_comment.pretty());
                doc
            }
            Term::Parenthesized(open, expr, close) => {
                let mut doc = open.pretty();
                doc.extend(expr.pretty());
                doc.extend(close.pretty());
                doc
            }
            Term::List(open, items, close) => {
                // Empty list
                if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.0.is_empty() {
                    let mut doc = open.pretty();
                    doc.push(hardspace());
                    doc.extend(close.pretty());
                    return doc;
                }

                // General list with items
                let open_without_trail = Ann {
                    pre_trivia: open.pre_trivia.clone(),
                    span: open.span,
                    trail_comment: None,
                    value: open.value.clone(),
                };

                let mut doc = open_without_trail.pretty();
                doc.extend(surround_with(vec![line()], nest({
                    let mut inner = open.trail_comment.pretty();
                    inner.extend(pretty_items(items));
                    inner
                })));
                doc.extend(close.pretty());
                doc
            }
            Term::Set(krec, open, binders, close) => {
                pretty_set(false, krec, open, binders, close)
            }
            Term::Selection(term, selectors, default) => {
                let mut doc = term.pretty();

                // Add separator based on term type
                let sep = match &**term {
                    // If it is an ident, keep it all together
                    Term::Token(_) => Vec::new(),
                    // If it is a parenthesized expression, maybe add a line break
                    Term::Parenthesized(_, _, _) => vec![softline_prime()],
                    // Otherwise, very likely add a line break
                    _ => vec![line_prime()],
                };
                doc.extend(sep);

                // Add selectors
                doc.extend(hcat(selectors.clone()));

                // Add optional "or default" clause
                if let Some((or_kw, def)) = default {
                    doc.push(softline());
                    doc.extend(nest({
                        let mut inner = or_kw.pretty();
                        inner.push(hardspace());
                        inner.extend(def.pretty());
                        inner
                    }));
                }
                doc
            }
        }
    }
}

impl Pretty for Expression {
    fn pretty(&self) -> Doc {
        match self {
            Expression::Term(t) => t.pretty(),
            Expression::Application(f, a) => {
                // Simplified for now
                let mut doc = f.pretty();
                doc.push(hardspace());
                doc.extend(a.pretty());
                doc
            }
            Expression::Operation(left, op, right) => {
                let mut doc = left.pretty();
                doc.push(hardspace());
                doc.extend(op.pretty());
                doc.push(hardspace());
                doc.extend(right.pretty());
                doc
            }
            Expression::MemberCheck(expr, question, selectors) => {
                let mut doc = expr.pretty();
                doc.push(hardspace());
                doc.extend(question.pretty());
                doc.push(hardspace());
                for sel in selectors {
                    doc.extend(sel.pretty());
                }
                doc
            }
            Expression::Negation(minus, expr) => {
                let mut doc = minus.pretty();
                doc.extend(expr.pretty());
                doc
            }
            Expression::Inversion(bang, expr) => {
                let mut doc = bang.pretty();
                doc.extend(expr.pretty());
                doc
            }
            Expression::Let(let_kw, binders, in_kw, expr) => {
                let mut doc = let_kw.pretty();
                doc.extend(nest({
                    let mut inner = vec![hardline()];
                    inner.extend(pretty_items(binders));
                    inner
                }));
                doc.push(hardline());
                doc.extend(in_kw.pretty());
                doc.push(hardline());
                doc.extend(expr.pretty());
                doc
            }
            Expression::If(if_kw, cond, then_kw, then_expr, else_kw, else_expr) => {
                let mut doc = if_kw.pretty();
                doc.push(hardspace());
                doc.extend(cond.pretty());
                doc.push(hardspace());
                doc.extend(then_kw.pretty());
                doc.push(hardspace());
                doc.extend(then_expr.pretty());
                doc.push(hardspace());
                doc.extend(else_kw.pretty());
                doc.push(hardspace());
                doc.extend(else_expr.pretty());
                doc
            }
            Expression::Assert(assert_kw, cond, semicolon, expr) => {
                let mut doc = assert_kw.pretty();
                doc.push(hardspace());
                doc.extend(cond.pretty());
                doc.extend(semicolon.pretty());
                doc.push(hardspace());
                doc.extend(expr.pretty());
                doc
            }
            Expression::With(with_kw, env, semicolon, expr) => {
                let mut doc = with_kw.pretty();
                doc.push(hardspace());
                doc.extend(env.pretty());
                doc.extend(semicolon.pretty());
                doc.push(hardspace());
                doc.extend(expr.pretty());
                doc
            }
            Expression::Abstraction(param, colon, body) => {
                let mut doc = param.pretty();
                doc.extend(colon.pretty());
                doc.push(hardspace());
                doc.extend(body.pretty());
                doc
            }
        }
    }
}

impl Pretty for ParamAttr {
    fn pretty(&self) -> Doc {
        match self {
            ParamAttr::ParamAttr(name, default, maybe_comma) => {
                let mut doc = name.pretty();

                // If there's a default value (? expr)
                if let Some((qmark, def)) = default.as_ref() {
                    doc.push(hardspace());
                    doc.extend(nest({
                        let mut inner = qmark.pretty();
                        inner.extend(absorb_rhs(def));
                        inner
                    }));
                }

                // Add optional comma
                if let Some(comma) = maybe_comma {
                    doc.extend(comma.pretty());
                }

                if default.is_some() {
                    group(doc)
                } else {
                    doc
                }
            }
            ParamAttr::ParamEllipsis(ellipsis) => ellipsis.pretty(),
        }
    }
}

impl Pretty for Parameter {
    fn pretty(&self) -> Doc {
        match self {
            Parameter::IDParameter(id) => id.pretty(),
            Parameter::SetParameter(open, attrs, close) => {
                let mut doc = open.pretty();

                if !attrs.is_empty() {
                    doc.push(hardspace());
                    for (i, attr) in attrs.iter().enumerate() {
                        if i > 0 {
                            doc.push(hardspace());
                        }
                        doc.extend(attr.pretty());
                    }
                    doc.push(hardspace());
                }

                doc.extend(close.pretty());
                doc
            }
            Parameter::ContextParameter(left, at, right) => {
                let mut doc = left.pretty();
                doc.push(hardspace());
                doc.extend(at.pretty());
                doc.push(hardspace());
                doc.extend(right.pretty());
                doc
            }
        }
    }
}

impl<T: Pretty> Pretty for Whole<T> {
    fn pretty(&self) -> Doc {
        let mut doc = self.value.pretty();
        doc.extend(self.trailing_trivia.pretty());
        doc
    }
}
