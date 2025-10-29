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
        Expression::Abstraction(Parameter::ID(_), _, body) => match &**body {
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
fn push_absorb_rhs(doc: &mut Doc, expr: &Expression) {
    match expr {
        // Special case: set with single inherit
        Expression::Term(Term::Set(_, _, binders, _)) => {
            if binders.0.len() == 1 {
                if let Item::Item(Binder::Inherit(_, _, _, _)) = &binders.0[0] {
                    push_nested(doc, |d| {
                        d.push(hardspace());
                        push_group(d, |inner| expr.pretty(inner));
                    });
                    return;
                }
            }
            // Absorbable set: force expand
            if is_absorbable_expr(expr) {
                push_nested(doc, |d| {
                    d.push(hardspace());
                    push_group(d, |inner| expr.pretty(inner));
                });
                return;
            }
            // Non-absorbable: new line
            push_nested(doc, |d| {
                d.push(line());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
        // Absorbable expressions
        _ if is_absorbable_expr(expr) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
        // Parenthesized expressions
        Expression::Term(Term::Parenthesized(_, _, _)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                expr.pretty(d);
            });
        }
        // Strings and paths: always keep on same line
        Expression::Term(Term::SimpleString(_))
        | Expression::Term(Term::IndentedString(_))
        | Expression::Term(Term::Path(_)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
        // Non-absorbable terms: start on new line
        Expression::Term(_) => {
            push_nested(doc, |d| {
                d.push(line());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
        // Function application: try to absorb
        Expression::Application(_, _) => {
            push_nested(doc, |d| {
                d.push(line());
                expr.pretty(d);
            });
        }
        // Everything else: new line
        _ => {
            push_nested(doc, |d| {
                d.push(line());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
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
    fn pretty(&self, doc: &mut Doc) {
        doc.push(hardspace());
        push_trailing_comment(doc, format!("# {}", self.0));
        doc.push(hardline());
    }
}

impl Pretty for Trivium {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Trivium::EmptyLine() => doc.push(emptyline()),
            Trivium::LineComment(c) => {
                push_comment(doc, format!("#{}", c));
                doc.push(hardline());
            }
            Trivium::BlockComment(is_doc, lines) => {
                push_comment(doc, if *is_doc { "/**" } else { "/*" });
                doc.push(hardline());
                // TODO: implement offset for block comment indentation
                for line in lines {
                    if line.is_empty() {
                        doc.push(emptyline());
                    } else {
                        push_comment(doc, line);
                        doc.push(hardline());
                    }
                }
                push_comment(doc, "*/");
                doc.push(hardline());
            }
            Trivium::LanguageAnnotation(lang) => {
                push_comment(doc, format!("/* {} */", lang));
                doc.push(hardspace());
            }
        }
    }
}

impl Pretty for Trivia {
    fn pretty(&self, doc: &mut Doc) {
        if self.0.is_empty() {
            return;
        }

        // Special case: single language annotation renders inline
        if self.0.len() == 1 {
            if let Trivium::LanguageAnnotation(_) = &self.0[0] {
                self.0[0].pretty(doc);
                return;
            }
        }

        doc.push(hardline());
        for trivium in &self.0 {
            trivium.pretty(doc);
        }
    }
}

impl<T: Pretty> Pretty for Ann<T> {
    fn pretty(&self, doc: &mut Doc) {
        self.pre_trivia.pretty(doc);
        self.value.pretty(doc);
        self.trail_comment.pretty(doc);
    }
}

// Pretty for Item - wraps items in groups, passes through comments
impl<T: Pretty> Pretty for Item<T> {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Item::Comments(trivia) => trivia.pretty(doc),
            Item::Item(x) => push_group(doc, |d| x.pretty(d)),
        }
    }
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
fn push_pretty_set(
    doc: &mut Doc,
    wide: bool,
    krec: &Option<Ann<Token>>,
    open: &Ann<Token>,
    items: &Items<Binder>,
    close: &Ann<Token>,
) {
    // Empty attribute set
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.0.is_empty() {
        // Pretty print optional `rec` keyword with hardspace
        if let Some(rec) = krec {
            rec.pretty(doc);
            doc.push(hardspace());
        }
        open.pretty(doc);
        doc.push(hardspace());
        close.pretty(doc);
        return;
    }

    // General set with items
    // Pretty print optional `rec` keyword with hardspace
    if let Some(rec) = krec {
        rec.pretty(doc);
        doc.push(hardspace());
    }

    // Open brace without trailing comment
    let open_without_trail = Ann {
        pre_trivia: open.pre_trivia.clone(),
        span: open.span,
        trail_comment: None,
        value: open.value.clone(),
    };
    open_without_trail.pretty(doc);

    // Separator: use hardline if wide and has items, else use line
    let sep = if wide && !items.0.is_empty() {
        vec![hardline()]
    } else {
        vec![line()]
    };

    push_surrounded(doc, &sep, |d| {
        push_nested(d, |inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items(inner, items);
        });
    });
    close.pretty(doc);
}

/// Format a list of items with interleaved comments
/// Based on Haskell prettyItems (Pretty.hs:108-120)
fn push_pretty_items<T: Pretty>(doc: &mut Doc, items: &Items<T>) {
    let items = &items.0;
    match items.as_slice() {
        [] => {}
        [item] => item.pretty(doc),
        items => {
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
                                    Trivium::LanguageAnnotation(lang.clone()).pretty(doc);
                                    doc.push(hardspace());
                                    push_group(doc, |d| string_item.pretty(d));
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                    }
                }

                items[i].pretty(doc);
                i += 1;
            }
        }
    }
}

impl Pretty for Binder {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Binder::Inherit(inherit, source, ids, semicolon) => {
                push_group(doc, |d| {
                    push_nested(d, |inner| {
                        inherit.pretty(inner);

                        // If there's a source expression like (foo), add it
                        if let Some(src) = source {
                            inner.push(line());
                            push_group(inner, |g| src.pretty(g));
                        }

                        // Add the identifiers
                        if !ids.is_empty() {
                            inner.push(hardline());
                            let hardline_doc = vec![hardline()];
                            push_sep_by(inner, &hardline_doc, ids.clone());
                        }

                        inner.push(hardline());
                        semicolon.pretty(inner);
                    });
                });
            }
            Binder::Assignment(selectors, assign, expr, semicolon) => {
                push_group(doc, |d| {
                    push_hcat(d, selectors.clone());
                    d.push(hardspace());
                    assign.pretty(d);
                    push_absorb_rhs(d, expr);
                    semicolon.pretty(d);
                });
            }
        }
    }
}

impl Pretty for Token {
    fn pretty(&self, doc: &mut Doc) {
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
            Sof => "",
            TTilde => "~",
        };
        push_text(doc, s);
    }
}

impl Pretty for SimpleSelector {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            SimpleSelector::ID(id) => id.pretty(doc),
            SimpleSelector::String(ann) => {
                ann.pre_trivia.pretty(doc);
                // TODO: implement prettySimpleString
                push_text(doc, "\"...\"");
                ann.trail_comment.pretty(doc);
            }
            SimpleSelector::Interpol(interp) => interp.pretty(doc),
        }
    }
}

impl Pretty for Selector {
    fn pretty(&self, doc: &mut Doc) {
        if let Some(dot) = &self.dot {
            dot.pretty(doc);
        }
        self.selector.pretty(doc);
    }
}

impl Pretty for StringPart {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            StringPart::TextPart(s) => push_text(doc, s),
            StringPart::Interpolation(whole) => {
                // For now, use a simple approach
                // TODO: implement absorption and isSimple checks
                push_text(doc, "${");
                push_group_ann(doc, GroupAnn::RegularG, |d| {
                    push_nested(d, |inner| {
                        inner.push(line_prime());
                        push_group(inner, |g| whole.value.pretty(g));
                        inner.push(line_prime());
                    });
                });
                push_text(doc, "}");
            }
        }
    }
}

impl Pretty for Vec<StringPart> {
    fn pretty(&self, doc: &mut Doc) {
        // Handle special case: single interpolation with leading whitespace
        if self.len() == 2 {
            if let (StringPart::TextPart(pre), StringPart::Interpolation(whole)) =
                (&self[0], &self[1])
            {
                if is_spaces(pre) && whole.trailing_trivia.0.is_empty() {
                    let indentation = text_width(pre);
                    push_text(doc, pre);
                    push_offset(doc, indentation, |d| {
                        push_group_ann(d, GroupAnn::RegularG, |g| {
                            push_text(g, "${");
                            push_nested(g, |inner| {
                                inner.push(line_prime());
                                push_group(inner, |ig| whole.value.pretty(ig));
                                inner.push(line_prime());
                            });
                            push_text(g, "}");
                        });
                    });
                    return;
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
                push_text(doc, t);
                let rest = self[1..].to_vec();
                push_offset(doc, indentation, |d| {
                    for part in &rest {
                        part.pretty(d);
                    }
                });
                return;
            }
        }

        // Default: just concatenate
        push_hcat(doc, self.clone());
    }
}

/// Format a simple string (with double quotes)
fn push_pretty_simple_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    push_group(doc, |d| {
        push_text(d, "\"");
        // Use literal \n instead of newline() to avoid indentation
        let newline_doc = vec![DocE::Text(0, 0, TextAnn::RegularT, "\n".to_string())];
        push_sep_by(d, &newline_doc, parts.to_vec());
        push_text(d, "\"");
    });
}

/// Format an indented string (with '')
fn push_pretty_indented_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    push_group(doc, |d| {
        push_text(d, "''");
        // For multi-line strings, add a potential line break after opening ''
        if parts.len() > 1 {
            d.push(line_prime());
        }
        push_nested(d, |inner| {
            let newline_doc = vec![newline()];
            push_sep_by(inner, &newline_doc, parts.to_vec());
        });
        push_text(d, "''");
    });
}

impl Pretty for Term {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Term::Token(t) => t.pretty(doc),
            Term::SimpleString(s) => {
                s.pre_trivia.pretty(doc);
                push_pretty_simple_string(doc, &s.value);
                s.trail_comment.pretty(doc);
            }
            Term::IndentedString(s) => {
                s.pre_trivia.pretty(doc);
                push_pretty_indented_string(doc, &s.value);
                s.trail_comment.pretty(doc);
            }
            Term::Path(p) => {
                // Path is Ann<Vec<StringPart>>
                p.pre_trivia.pretty(doc);
                for part in &p.value {
                    part.pretty(doc);
                }
                p.trail_comment.pretty(doc);
            }
            Term::Parenthesized(open, expr, close) => {
                open.pretty(doc);
                expr.pretty(doc);
                close.pretty(doc);
            }
            Term::List(open, items, close) => {
                // Empty list
                if items.0.is_empty()
                    && open.trail_comment.is_none()
                    && close.pre_trivia.0.is_empty()
                {
                    open.pretty(doc);
                    doc.push(hardspace());
                    close.pretty(doc);
                    return;
                }

                // General list with items
                let open_without_trail = Ann {
                    pre_trivia: open.pre_trivia.clone(),
                    span: open.span,
                    trail_comment: None,
                    value: open.value.clone(),
                };

                open_without_trail.pretty(doc);
                let line_doc = vec![line()];
                push_surrounded(doc, &line_doc, |d| {
                    push_nested(d, |inner| {
                        open.trail_comment.pretty(inner);
                        push_pretty_items(inner, items);
                    });
                });
                close.pretty(doc);
            }
            Term::Set(krec, open, binders, close) => {
                push_pretty_set(doc, false, krec, open, binders, close);
            }
            Term::Selection(term, selectors, default) => {
                term.pretty(doc);

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
                push_hcat(doc, selectors.clone());

                // Add optional "or default" clause
                if let Some((or_kw, def)) = default {
                    doc.push(softline());
                    push_nested(doc, |inner| {
                        or_kw.pretty(inner);
                        inner.push(hardspace());
                        def.pretty(inner);
                    });
                }
            }
        }
    }
}

impl Pretty for Expression {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Expression::Term(t) => t.pretty(doc),
            Expression::Application(f, a) => {
                // Simplified for now
                f.pretty(doc);
                doc.push(hardspace());
                a.pretty(doc);
            }
            Expression::Operation(left, op, right) => {
                // Special case: absorbable RHS with update/concat/plus operator
                // Based on Haskell prettyApp (Pretty.hs:665-667)
                if let Expression::Term(t) = &**right {
                    if is_absorbable_term(t) && op.value.is_update_concat_plus() {
                        push_nested(doc, |d| {
                            push_group(d, |inner| {
                                inner.push(line());
                                left.pretty(inner);
                                inner.push(line());
                                push_group_ann(inner, GroupAnn::Transparent, |trans| {
                                    op.pretty(trans);
                                    trans.push(hardspace());
                                    push_group_ann(trans, GroupAnn::Priority, |prio| {
                                        // prettyTermWide
                                        t.pretty(prio);
                                    });
                                });
                            });
                        });
                        return;
                    }
                }

                // Default case
                left.pretty(doc);
                doc.push(hardspace());
                op.pretty(doc);
                doc.push(hardspace());
                right.pretty(doc);
            }
            Expression::MemberCheck(expr, question, selectors) => {
                expr.pretty(doc);
                doc.push(hardspace());
                question.pretty(doc);
                doc.push(hardspace());
                for sel in selectors {
                    sel.pretty(doc);
                }
            }
            Expression::Negation(minus, expr) => {
                minus.pretty(doc);
                expr.pretty(doc);
            }
            Expression::Inversion(bang, expr) => {
                bang.pretty(doc);
                expr.pretty(doc);
            }
            Expression::Let(let_kw, binders, in_kw, expr) => {
                let_kw.pretty(doc);
                push_nested(doc, |inner| {
                    inner.push(hardline());
                    push_pretty_items(inner, binders);
                });
                doc.push(hardline());
                in_kw.pretty(doc);
                doc.push(hardline());
                expr.pretty(doc);
            }
            Expression::If(if_kw, cond, then_kw, then_expr, else_kw, else_expr) => {
                if_kw.pretty(doc);
                doc.push(hardspace());
                cond.pretty(doc);
                doc.push(hardspace());
                then_kw.pretty(doc);
                doc.push(hardspace());
                then_expr.pretty(doc);
                doc.push(hardspace());
                else_kw.pretty(doc);
                doc.push(hardspace());
                else_expr.pretty(doc);
            }
            Expression::Assert(assert_kw, cond, semicolon, expr) => {
                assert_kw.pretty(doc);
                doc.push(hardspace());
                cond.pretty(doc);
                semicolon.pretty(doc);
                doc.push(hardspace());
                expr.pretty(doc);
            }
            Expression::With(with_kw, env, semicolon, expr) => {
                with_kw.pretty(doc);
                doc.push(hardspace());
                env.pretty(doc);
                semicolon.pretty(doc);
                doc.push(hardspace());
                // Use Priority group for the body expression
                // Based on Haskell prettyWith (Pretty.hs:553-567)
                push_group_ann(doc, GroupAnn::Priority, |d| {
                    expr.pretty(d);
                });
            }
            Expression::Abstraction(param, colon, body) => {
                param.pretty(doc);
                colon.pretty(doc);
                doc.push(hardspace());
                body.pretty(doc);
            }
        }
    }
}

impl Pretty for ParamAttr {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            ParamAttr::ParamAttr(name, default, maybe_comma) => {
                let has_default = default.is_some();
                let make_pretty = |d: &mut Doc| {
                    name.pretty(d);

                    // If there's a default value (? expr)
                    if let Some((qmark, def)) = default.as_ref() {
                        d.push(hardspace());
                        push_nested(d, |inner| {
                            qmark.pretty(inner);
                            push_absorb_rhs(inner, def);
                        });
                    }

                    // Add optional comma
                    if let Some(comma) = maybe_comma {
                        comma.pretty(d);
                    }
                };

                if has_default {
                    push_group(doc, make_pretty);
                } else {
                    make_pretty(doc);
                }
            }
            ParamAttr::ParamEllipsis(ellipsis) => ellipsis.pretty(doc),
        }
    }
}

impl Pretty for Parameter {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Parameter::ID(id) => id.pretty(doc),
            Parameter::Set(open, attrs, close) => {
                open.pretty(doc);

                if !attrs.is_empty() {
                    doc.push(hardspace());
                    for (i, attr) in attrs.iter().enumerate() {
                        if i > 0 {
                            doc.push(hardspace());
                        }
                        attr.pretty(doc);
                    }
                    doc.push(hardspace());
                }

                close.pretty(doc);
            }
            Parameter::Context(left, at, right) => {
                left.pretty(doc);
                doc.push(hardspace());
                at.pretty(doc);
                doc.push(hardspace());
                right.pretty(doc);
            }
        }
    }
}

impl<T: Pretty> Pretty for Whole<T> {
    fn pretty(&self, doc: &mut Doc) {
        self.value.pretty(doc);
        self.trailing_trivia.pretty(doc);
    }
}
