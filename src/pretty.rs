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
        // Empty sets/lists - absorbable only if braces/brackets span multiple lines
        Term::Set(_, open, items, close) if items.0.is_empty() => {
            open.span.start_line != close.span.start_line
        }
        Term::List(open, items, close) if items.0.is_empty() => {
            open.span.start_line != close.span.start_line
        }
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
                push_group(d, |inner| {
                    inner.push(line());
                    expr.pretty(inner);
                });
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
                push_group(d, |inner| {
                    inner.push(line());
                    expr.pretty(inner);
                });
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

fn is_lone_ann<T>(ann: &Ann<T>) -> bool {
    ann.pre_trivia.0.is_empty() && ann.trail_comment.is_none()
}

fn is_simple_selector(selector: &Selector) -> bool {
    matches!(selector.selector, SimpleSelector::ID(_))
}

fn is_simple_term(term: &Term) -> bool {
    match term {
        Term::SimpleString(s) | Term::IndentedString(s) => is_lone_ann(s),
        Term::Path(p) => is_lone_ann(p),
        Term::Token(leaf)
            if is_lone_ann(leaf)
                && matches!(
                    leaf.value,
                    Token::Identifier(_) | Token::Integer(_) | Token::Float(_) | Token::EnvPath(_)
                ) =>
        {
            true
        }
        Term::Selection(term, selectors, def) => {
            is_simple_term(term) && selectors.iter().all(is_simple_selector) && def.is_none()
        }
        Term::Parenthesized(open, expr, close) => {
            is_lone_ann(open) && is_lone_ann(close) && is_simple_expression(expr)
        }
        _ => false,
    }
}

fn application_arity(expr: &Expression) -> usize {
    match expr {
        Expression::Application(f, _) => 1 + application_arity(f),
        _ => 0,
    }
}

fn collect_application_parts<'a>(expr: &'a Expression, parts: &mut Vec<&'a Expression>) {
    match expr {
        Expression::Application(f, a) => {
            collect_application_parts(f, parts);
            parts.push(a);
        }
        _ => parts.push(expr),
    }
}

fn flatten_operation_chain<'a>(
    target: &'a Leaf,
    expr: &'a Expression,
    current_op: Option<&'a Leaf>,
    out: &mut Vec<(Option<&'a Leaf>, &'a Expression)>,
) {
    match expr {
        Expression::Operation(left, op_leaf, right) if op_leaf.value == target.value => {
            flatten_operation_chain(target, left, current_op, out);
            flatten_operation_chain(target, right, Some(op_leaf), out);
        }
        _ => out.push((current_op, expr)),
    }
}

fn push_absorb_operation(doc: &mut Doc, expr: &Expression) {
    match expr {
        Expression::Term(term) if is_absorbable_term(term) => {
            doc.push(hardspace());
            term.pretty(doc);
        }
        Expression::Operation(_, _, _) => {
            push_group(doc, |group_doc| {
                group_doc.push(line());
                expr.pretty(group_doc);
            });
        }
        Expression::Application(_, _) => {
            doc.push(hardspace());
            push_group(doc, |group_doc| {
                expr.pretty(group_doc);
            });
        }
        _ => {
            doc.push(hardspace());
            expr.pretty(doc);
        }
    }
}

fn push_pretty_operation(
    doc: &mut Doc,
    force_first_term_wide: bool,
    operation: &Expression,
    op: &Leaf,
) {
    let mut parts: Vec<(Option<&Leaf>, &Expression)> = Vec::new();
    flatten_operation_chain(op, operation, None, &mut parts);

    push_group(doc, |group_doc| {
        for (maybe_op, expr) in parts.iter() {
            match maybe_op {
                None => {
                    if force_first_term_wide {
                        if let Expression::Term(term) = expr {
                            if is_absorbable_term(term) {
                                // TODO: implement wide rendering parity
                            }
                        }
                    }
                    expr.pretty(group_doc);
                }
                Some(op_leaf) => {
                    group_doc.push(line());
                    op_leaf.pretty(group_doc);
                    push_nested(group_doc, |nested| {
                        push_absorb_operation(nested, expr);
                    });
                }
            }
        }
    });
}

fn push_absorb_abs(doc: &mut Doc, depth: usize, expr: &Expression) {
    match expr {
        Expression::Abstraction(Parameter::ID(param), colon, body) => {
            doc.push(hardspace());
            param.pretty(doc);
            colon.pretty(doc);
            push_absorb_abs(doc, depth + 1, body);
        }
        _ => {
            let separator = if depth <= 2 { line() } else { hardline() };
            doc.push(separator);
            push_group(doc, |group_doc| {
                expr.pretty(group_doc);
            });
        }
    }
}

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Term(term) => is_simple_term(term),
        Expression::Application(f, a) => {
            if application_arity(expr) >= 3 {
                return false;
            }
            is_simple_expression(f) && is_simple_expression(a)
        }
        _ => false,
    }
}

/// Render the nested document that appears between parentheses.
/// Mirrors `inner` in nixfmt's `prettyTerm (Parenthesized ...)`.
fn push_parenthesized_inner(doc: &mut Doc, expr: &Expression) {
    match expr {
        _ if is_absorbable_expr(expr) => {
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
        }
        Expression::Application(_, _) => {
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
        }
        Expression::Term(Term::Selection(term, _, _)) if is_absorbable_term(term) => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        Expression::Term(Term::Selection(_, _, _)) => {
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        _ => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
    }
}

/// Pretty print a parenthesized expression following nixfmt's structure.
fn push_pretty_parenthesized(
    doc: &mut Doc,
    open: &Ann<Token>,
    expr: &Expression,
    close: &Ann<Token>,
) {
    let mut open_clean = open.clone();
    let trailing = open_clean.trail_comment.take();

    let close_pre = close.pre_trivia.clone();
    let mut close_clean = close.clone();
    close_clean.pre_trivia = Trivia::new();

    push_group(doc, |group_doc| {
        open_clean.pretty(group_doc);

        push_nested(group_doc, |nested| {
            if let Some(trailing_comment) = trailing {
                let comment: Trivia =
                    vec![Trivium::LineComment(format!(" {}", trailing_comment.0))].into();
                comment.pretty(nested);
            }
            push_parenthesized_inner(nested, expr);
            close_pre.pretty(nested);
        });

        close_clean.pretty(group_doc);
    });
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

    // Separator: prefer hardline before close when items start with an empty line
    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => trivia.0.iter().any(|t| matches!(t, Trivium::EmptyLine())),
        _ => false,
    };

    // Separator: use hardline if wide, or when starting with an empty line; else use line
    let sep = if !items.0.is_empty() && (wide || starts_with_emptyline) {
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
                // Determine spacing strategy based on original layout
                let same_line = inherit.span.start_line == semicolon.span.start_line;
                let few_ids = ids.len() < 4;
                let (sep, nosep) = if same_line && few_ids {
                    (line(), line_prime())
                } else {
                    (hardline(), hardline())
                };
                let sep_doc = vec![sep.clone()];

                push_group(doc, |d| {
                    inherit.pretty(d);
                    match source {
                        None => {
                            d.push(sep.clone());
                            push_nested(d, |nested| {
                                if !ids.is_empty() {
                                    push_sep_by(nested, &sep_doc, ids.clone());
                                }
                                nested.push(nosep.clone());
                                semicolon.pretty(nested);
                            });
                        }
                        Some(src) => {
                            push_nested(d, |nested| {
                                push_group(nested, |g| {
                                    g.push(line());
                                    src.pretty(g);
                                });
                                nested.push(sep);
                                if !ids.is_empty() {
                                    push_sep_by(nested, &sep_doc, ids.clone());
                                }
                                nested.push(nosep.clone());
                                semicolon.pretty(nested);
                            });
                        }
                    }
                });
            }
            Binder::Assignment(selectors, assign, expr, semicolon) => {
                push_group(doc, |d| {
                    push_hcat(d, selectors.clone());
                    push_nested(d, |inner| {
                        inner.push(hardspace());
                        assign.pretty(inner);
                        push_absorb_rhs(inner, expr);
                    });
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
                let trailing_empty = whole.trailing_trivia.0.is_empty();
                let value = &whole.value;

                let absorbable_term = if trailing_empty {
                    if let Expression::Term(term) = value {
                        if is_absorbable_term(term) {
                            Some(term)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let simple_value = trailing_empty && is_simple_expression(value);

                if let Some(term) = absorbable_term {
                    push_group(doc, |group_doc| {
                        push_text(group_doc, "${");
                        term.pretty(group_doc);
                        push_text(group_doc, "}");
                    });
                    return;
                }

                if simple_value {
                    push_text(doc, "${");
                    value.pretty(doc);
                    push_text(doc, "}");
                    return;
                }

                push_group(doc, |group_doc| {
                    push_text(group_doc, "${");
                    push_nested(group_doc, |nested| {
                        nested.push(line_prime());
                        whole.pretty(nested);
                        nested.push(line_prime());
                    });
                    push_text(group_doc, "}");
                });
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
                push_pretty_parenthesized(doc, open, expr, close);
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
            Expression::Application(_, _) => {
                let mut parts = Vec::new();
                collect_application_parts(self, &mut parts);

                if parts.len() >= 2 {
                    let (first, tail) = parts.split_first().unwrap();
                    let (leading_args, last_arg) = tail.split_at(tail.len() - 1);

                    push_group(doc, |group_doc| {
                        push_group_ann(group_doc, GroupAnn::Transparent, |outer| {
                            push_group_ann(outer, GroupAnn::Transparent, |func_group| {
                                first.pretty(func_group);
                            });

                            for (_idx, arg) in leading_args.iter().enumerate() {
                                outer.push(line());
                                let arg_expr = *arg;
                                push_group_ann(outer, GroupAnn::Priority, |priority_group| {
                                    push_group(priority_group, |arg_group| {
                                        push_nested(arg_group, |nested| {
                                            arg_expr.pretty(nested);
                                        });
                                    });
                                });
                            }
                        });

                        if let Some(last) = last_arg.first() {
                            let last_expr = *last;
                            group_doc.push(line());
                            push_group(group_doc, |last_group| {
                                push_nested(last_group, |nested| {
                                    last_expr.pretty(nested);
                                });
                            });
                        }
                    });
                } else if let Some(only) = parts.first() {
                    only.pretty(doc);
                }
            }
            Expression::Operation(left, op, right) => {
                // Special case: absorbable RHS with update/concat/plus operator
                // Based on Haskell prettyApp (Pretty.hs:665-667)
                if let Expression::Term(t) = &**right {
                    if is_absorbable_term(t) && op.value.is_update_concat_plus() {
                        push_group(doc, |inner| {
                            left.pretty(inner);
                            inner.push(line());
                            op.pretty(inner);
                            inner.push(hardspace());
                            push_nested(inner, |nested| {
                                t.pretty(nested);
                            });
                        });
                        return;
                    }
                }

                // Default case
                push_pretty_operation(doc, false, self, op);
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
                let mut in_kw_clean = in_kw.clone();
                in_kw_clean.pre_trivia = Trivia::new();
                in_kw_clean.trail_comment = None;

                let mut moved_trivia_vec: Vec<Trivium> = in_kw.pre_trivia.clone().into();
                if let Some(trailing) = &in_kw.trail_comment {
                    moved_trivia_vec.push(Trivium::LineComment(format!(" {}", trailing.0)));
                }
                let moved_trivia: Trivia = moved_trivia_vec.into();

                push_group(doc, |doc| {
                    let_kw.pretty(doc);
                    doc.push(hardline());
                    push_nested(doc, |inner| {
                        push_pretty_items(inner, binders);
                    });
                });
                doc.push(hardline());
                push_group(doc, |doc| {
                    in_kw_clean.pretty(doc);
                    doc.push(hardline());
                    moved_trivia.pretty(doc);
                    expr.pretty(doc);
                });
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
                push_group(doc, |doc| {
                    push_group(doc, |inner| {
                        push_group_ann(inner, GroupAnn::Transparent, |transparent| {
                            assert_kw.pretty(transparent);
                        });
                        inner.push(line());
                        push_group(inner, |arg_group| {
                            push_nested(arg_group, |nested| {
                                cond.pretty(nested);
                            });
                        });
                    });
                    semicolon.pretty(doc);
                    doc.push(hardline());
                    expr.pretty(doc);
                });
            }
            Expression::With(with_kw, env, semicolon, expr) => {
                push_group(doc, |doc| {
                    push_group(doc, |inner| {
                        with_kw.pretty(inner);
                        inner.push(hardspace());
                        push_group(inner, |grouped_env| {
                            push_nested(grouped_env, |nested| {
                                env.pretty(nested);
                            });
                        });
                        semicolon.pretty(inner);
                    });
                    doc.push(line());
                    expr.pretty(doc);
                });
            }
            Expression::Abstraction(Parameter::ID(param), colon, body) => {
                push_group(doc, |group_doc| {
                    group_doc.push(line_prime());
                    param.pretty(group_doc);
                    colon.pretty(group_doc);
                    push_absorb_abs(group_doc, 1, body);
                });
            }
            Expression::Abstraction(param, colon, body) => {
                param.pretty(doc);
                colon.pretty(doc);
                doc.push(line());
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

fn param_attr_without_default(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::ParamAttr(_, default, _) if default.is_none())
}

fn param_attr_is_ellipsis(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::ParamEllipsis(_))
}

fn parameter_separator(open: &Leaf, attrs: &[ParamAttr], close: &Leaf) -> DocE {
    if open.span.start_line != close.span.start_line {
        return hardline();
    }

    match attrs {
        [attr] if param_attr_is_ellipsis(attr) => line(),
        [attr] if param_attr_without_default(attr) => line(),
        [a, b] if param_attr_without_default(a) && param_attr_is_ellipsis(b) => line(),
        [a, b] if param_attr_without_default(a) && param_attr_without_default(b) => line(),
        [a, b, c]
            if param_attr_without_default(a)
                && param_attr_without_default(b)
                && param_attr_is_ellipsis(c) =>
        {
            line()
        }
        _ => hardline(),
    }
}

fn render_param_attrs(attrs: &[ParamAttr]) -> Vec<Doc> {
    attrs
        .iter()
        .enumerate()
        .map(|(idx, attr)| {
            let mut rendered = Vec::new();
            let is_last = idx + 1 == attrs.len();

            if is_last {
                if let ParamAttr::ParamAttr(name, default, _) = attr {
                    ParamAttr::ParamAttr(name.clone(), default.clone(), None).pretty(&mut rendered);
                    push_trailing(&mut rendered, ",");
                    return rendered;
                }
            }

            attr.pretty(&mut rendered);
            rendered
        })
        .collect()
}

impl Pretty for Parameter {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Parameter::ID(id) => id.pretty(doc),
            Parameter::Set(open, attrs, close) => {
                if attrs.is_empty() {
                    let sep = if open.span.start_line != close.span.start_line {
                        hardline()
                    } else {
                        hardspace()
                    };

                    push_group(doc, |doc| {
                        open.pretty(doc);
                        doc.push(sep);
                        close.pretty(doc);
                    });
                    return;
                }

                let sep = parameter_separator(open, attrs, close);
                let sep_doc = vec![sep.clone()];

                push_group(doc, |doc| {
                    open.pretty(doc);
                    doc.push(sep.clone());
                    let sep_after = sep.clone();
                    push_nested(doc, |inner| {
                        let attr_docs = render_param_attrs(attrs);
                        push_sep_by(inner, &sep_doc, attr_docs);
                    });
                    doc.push(sep_after);
                    close.pretty(doc);
                });
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
        // Wrap the entire content in a group
        // This matches nixfmt's: pretty (Whole x finalTrivia) = group $ pretty x <> pretty finalTrivia
        push_group(doc, |doc| {
            self.value.pretty(doc);
            self.trailing_trivia.pretty(doc);
        });
        // Do not force a final Hardline; reference nixfmt IR does not
        // add a trailing newline at the top level in --ir output.
        // Keeping parity avoids extra Spacing Hardline in diffs.
    }
}
