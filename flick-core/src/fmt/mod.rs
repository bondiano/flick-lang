//! `flick fmt`: rewrites whitespace only — tokens, comments and their order are
//! kept, so the AST cannot change, except that empty rows of an arrow are
//! dropped: `->{none; E}` becomes `->{; E}` and `->{none; none}` becomes
//! `->{}`, and a list broken across lines gets a trailing separator.
//! Indentation is renormalized to two spaces per level while
//! preserving how each line's column compares with the columns before it (that comparison is what layout means), spacing inside a
//! line follows fixed rules, and blank-line runs collapse to one. The result is
//! re-parsed and compared with the input AST before it is returned.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::FileId;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::{ast, parse};

mod trailing;
mod wrap;

/// Formats a file; syntax errors are returned instead of output.
pub fn format(text: &str, file: FileId) -> Result<String, Vec<Diagnostic>> {
    let mut parsed = parse(text, file);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    drop_empty_rows(&mut parsed.tree);
    let expected = ast::lower(&parsed.tree, text);
    let same_program = |out: &str| {
        let again = parse(out, file);
        again.diagnostics.is_empty() && ast::lower(&again.tree, out) == expected
    };
    let pass = |text: &str, tree: &Node| {
        let toks = flatten(tree);
        wrap::wrap_long_lines(text, &layout(text, &toks), same_program)
    };
    let mut out = pass(text, &parsed.tree);
    // a trailing separator may push a line past the width, and wrapping may
    // break another list; a few passes reach the fixed point
    for _ in 0..3 {
        let tree = parse(&out, file).tree;
        let Some(next) = trailing::add(&out, &tree, same_program) else {
            break;
        };
        out = pass(&next, &parse(&next, file).tree);
    }
    if !same_program(&out) {
        return Err(vec![Diagnostic::new(
            codes::P003,
            Severity::Error,
            "the formatter would change the meaning of this file; it was left unchanged",
        )]);
    }
    Ok(out)
}

/// Drops empty rows of an arrow: `->{none; E}` becomes `->{; E}` and
/// `->{none; none}` becomes `->{}`; a row holding a comment is kept.
fn drop_empty_rows(node: &mut Node) {
    if node.kind == K::EffectAnnot {
        let empty = |n: &Node| is_bare_none(n) || n.children.iter().all(is_whitespace);
        let rows: Vec<&Node> = node.nodes().filter(|n| n.kind == K::Row).collect();
        if rows.len() == 2 && rows.iter().all(|r| empty(r)) {
            node.children
                .retain(|c| matches!(c, Element::Token(t) if t.kind != K::Semi));
        } else if let Some(Element::Node(services)) = node
            .children
            .iter_mut()
            .find(|c| matches!(c, Element::Node(n) if n.kind == K::Row))
            && is_bare_none(services)
        {
            services.children.clear();
        }
    }
    for child in &mut node.children {
        if let Element::Node(n) = child {
            drop_empty_rows(n);
        }
    }
}

fn is_whitespace(element: &Element) -> bool {
    matches!(element, Element::Token(t) if t.kind == K::Whitespace)
}

fn is_bare_none(row: &Node) -> bool {
    let solid = |n: &Node| n.children.iter().filter(|c| !is_whitespace(c)).count() == 1;
    solid(row)
        && row
            .nodes()
            .next()
            .is_some_and(|n| n.kind == K::NameType && solid(n) && n.has_token(K::NoneKw))
}

#[derive(Clone, Copy)]
struct Tok {
    token: Token,
    parent: K,
    /// First token of an argument of a function application.
    arg: bool,
}

/// One output line; `None` in a layout is a blank line.
struct Line {
    indent: usize,
    toks: Vec<Tok>,
}

fn flatten(tree: &Node) -> Vec<Tok> {
    fn walk(node: &Node, out: &mut Vec<Tok>) {
        for (index, child) in node.children.iter().enumerate() {
            match child {
                Element::Node(n) => {
                    let first = out.len();
                    walk(n, out);
                    // children after the head of an application are its arguments
                    let head = node.children[..index]
                        .iter()
                        .any(|c| matches!(c, Element::Node(_)));
                    if node.kind == K::AppExpr && head && first < out.len() {
                        out[first].arg = true;
                    }
                }
                Element::Token(t) if t.kind == K::Whitespace => {}
                Element::Token(t) => out.push(Tok {
                    token: *t,
                    parent: node.kind,
                    arg: false,
                }),
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &mut out);
    out
}

/// Whether two tokens on one line are separated by a space.
fn space(a: Tok, b: Tok) -> bool {
    let (ak, bk) = (a.token.kind, b.token.kind);
    if ak.is_comment() || bk.is_comment() {
        return true;
    }
    let tight_braces = |t: Tok| matches!(t.parent, K::EffectAnnot | K::RowType);
    match (ak, bk) {
        (K::LParen | K::LBracket | K::Caret | K::Dollar, _)
        | (_, K::RParen | K::RBracket | K::Comma | K::Semi | K::Caret | K::Dot)
        | (K::LBrace, K::RBrace) => false,
        (K::Dot, _) => a.parent == K::ForallType,
        (K::Arrow, K::LBrace) if b.parent == K::EffectAnnot => false,
        (K::LBrace, _) => !tight_braces(a),
        (_, K::RBrace) => !tight_braces(b),
        (K::Minus, _) => !matches!(a.parent, K::PrefixExpr | K::LitPat),
        (K::Con, K::LParen) => b.parent != K::ExposeItem,
        _ => true,
    }
}

/// Maps an original indentation to the new one; `stack` holds `(original,
/// new)` columns of enclosing lines and mid-line block anchors, increasing.
/// `later` gives the original columns of the code lines after this one: a
/// line one level deeper than its parent goes two columns further when a
/// later line, before the block ends, sits strictly between the two.
fn indent(stack: &mut Vec<(usize, usize)>, orig: usize, keep: bool, later: &[usize]) -> usize {
    let mut depth = stack.len();
    while depth > 1 && stack[depth - 1].0 > orig {
        depth -= 1;
    }
    let (top_orig, top_new) = stack[depth - 1];
    let between = || {
        later
            .iter()
            .take_while(|&&c| c > top_orig)
            .any(|&c| c < orig)
    };
    let new = if top_orig == orig {
        top_new
    } else if between() {
        top_new + 4
    } else {
        top_new + 2
    };
    if keep {
        stack.truncate(depth);
        if top_orig != orig {
            stack.push((orig, new));
        }
    }
    new
}

/// Splits tokens into lines with their new indentation.
fn layout(text: &str, toks: &[Tok]) -> Vec<Option<Line>> {
    let column = |at: u32| {
        let at = at as usize;
        let begin = text[..at].rfind('\n').map_or(0, |i| i + 1);
        text[begin..at].chars().count()
    };
    let mut lines = Vec::with_capacity(toks.len());
    let (mut line, mut seen) = (0, 0);
    for t in toks {
        line += text[seen..t.token.start as usize].matches('\n').count();
        seen = t.token.start as usize;
        lines.push(line);
    }
    // line of the `}` closing each `{`, and the brace a record update's `with` is in
    let mut closes = vec![None; toks.len()];
    let mut owner = vec![None; toks.len()];
    let mut open = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        match t.token.kind {
            K::LBrace => open.push(i),
            K::RBrace => {
                if let Some(o) = open.pop() {
                    closes[o] = Some(lines[i]);
                }
            }
            K::WithKw if t.parent == K::RecordUpdate => owner[i] = open.last().copied(),
            _ => {}
        }
    }

    // the original column of each code line, in order
    let mut code_columns = Vec::new();
    let mut at_start = true;
    for t in toks {
        if t.token.kind == K::Newline {
            at_start = true;
        } else if at_start {
            if !t.token.kind.is_comment() {
                code_columns.push(column(t.token.start));
            }
            at_start = false;
        }
    }
    let mut code_line = 0;

    let mut out = Vec::new();
    let mut stack = vec![(0, 0)];
    let mut blank = false;
    let mut i = 0;
    while i < toks.len() {
        let end = toks[i..]
            .iter()
            .position(|t| t.token.kind == K::Newline)
            .map_or(toks.len(), |n| i + n);
        let line = &toks[i..end];
        let first = i;
        i = end + 1;
        if line.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if blank {
            out.push(None);
            blank = false;
        }
        let code = line.iter().any(|t| !t.token.kind.is_comment());
        if code && !line[0].token.kind.is_comment() {
            code_line += 1;
        }
        let later = &code_columns[code_line.min(code_columns.len())..];
        let new_indent = indent(&mut stack, column(line[0].token.start), code, later);
        let mut width = new_indent;
        let mut anchors = Vec::new();
        for (k, &t) in line.iter().enumerate() {
            if k > 0 && space(line[k - 1], t) {
                width += 1;
            }
            let at = first + k;
            // first item of a multi-line block that starts mid-line
            let opens_block = k > 0
                && match line[k - 1].token.kind {
                    K::LBrace => closes[at - 1],
                    K::WithKw => owner[at - 1].and_then(|o| closes[o]),
                    _ => None,
                }
                .is_some_and(|close| close > lines[at]);
            if opens_block && code {
                anchors.push((column(t.token.start), width));
            }
            width += t.token.text(text).chars().count();
        }
        stack.extend(anchors);
        out.push(Some(Line {
            indent: new_indent,
            toks: line.to_vec(),
        }));
    }
    out
}

/// The text of one line, without its line break.
fn join(text: &str, indent: usize, toks: &[Tok]) -> String {
    let mut out = " ".repeat(indent);
    for (k, &t) in toks.iter().enumerate() {
        if k > 0 && space(toks[k - 1], t) {
            out.push(' ');
        }
        out.push_str(t.token.text(text));
    }
    out
}

#[cfg(test)]
mod tests;
