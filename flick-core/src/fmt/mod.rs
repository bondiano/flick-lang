//! `flick fmt`: the canonical layout of a file. The layout depends on the
//! code and the line width ([`WIDTH`]) only, not on how the source was laid
//! out, apart from the few choices below that the author makes on purpose.
//! Each declaration becomes a [`flick_pretty`] document built from the
//! concrete tree; the printer keeps a group on one line when it fits and
//! breaks the outermost groups first. Indentation is two spaces.
//!
//! The rules, as `gleam format` has them where Flick has the construct:
//!
//! - a value after `=` (`let`, record fields, type aliases), `->` (`fun`,
//!   match arms) or `<-` stays on the line when it fits, else moves to the
//!   next line one level in. Records, lists, tuples, `fun` and blocks in
//!   braces (`task`, `ui`, `scope`, `provide`, `do`, `for`, `sql`, `cli`,
//!   `codec`) hug the `=` instead: they stay on its line and break inside.
//!   Several statements always go on their own lines, and so do `match`,
//!   `try` and an `if` that does not fit;
//! - statements go one per line, without `;`; a block in braces holding one
//!   expression may stay on one line, `task { load id }`;
//! - an application breaks before each argument, one level in. A last
//!   argument that is a function in parentheses, a list, a record, a tuple
//!   or a block in braces hugs the call when what precedes its first break
//!   fits: `f a (fun x ->`, the body, then `)` on its own line;
//! - an operator chain breaks before each operator of its weakest
//!   precedence, one level in; a pipeline breaks one step per line, its `|>`
//!   under the first operand when the pipeline starts its line;
//! - `if c then a else b` stays on one line or puts each branch on its own
//!   lines; `match` and `try` put each arm on its own line, under `match`;
//! - lists, tuples and `exposing (…)` stay on one line or put one item per
//!   line followed by `,`; records (also record types, patterns and updates)
//!   put one field per line followed by `;`, a row tail `| r` last;
//! - a function type breaks after each arrow, `->{r; e}` staying whole; an
//!   empty service row is dropped: `->{none; E}` becomes `->{; E}` and
//!   `->{none; none}` becomes `->{}`;
//! - an outer `forall` of a nonrecursive `let` signature is dropped when its
//!   variables stay implicitly quantified and no comment would be lost;
//! - variants stay on one line when the declaration fits, else one per line
//!   after `|`; `deriving` ends the line or takes its own; members of
//!   classes, instances, services and `rec`, foreign metadata, columns of a
//!   `sql table` always go one per line; `on` stays after its `join` when it
//!   fits.
//!
//! What the author decides: a trailing separator (`[a, b,]`, `{ a = 1; }`)
//! or a blank line between items keeps a bracket broken one item per line
//! (a list of constants written several per line is then filled); a
//! pipeline written with a step on its own line keeps one step per line;
//! blank lines between declarations, statements, members, arms, fields and
//! items are kept, at most one. Comments stay where they are: a comment on
//! its own line before what follows it, a comment ending a line at the end
//! of that line, which then breaks.
//!
//! Every declaration is re-parsed after layout and compared with the source
//! (see [`program`]); a declaration whose layout would change its meaning, or
//! that holds a line comment where the layout has no place for it, is left
//! as written. The whole file is compared again at the end.

use flick_pretty::DocumentArena;

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::FileId;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::{parse, parse_with_heads};

mod block;
mod comments;
mod decl;
mod expr;
pub(crate) mod foralls;
mod plugins;
mod printer;
mod program;
mod record;
mod types;

use comments::Comments;
use foralls::drop_redundant_foralls;
pub use plugins::PluginOf;
use printer::{Printer, nodes};
use program::{holds_set, program};

/// The line width the layout aims at.
pub const WIDTH: usize = 100;

/// Formats a file; syntax errors are returned instead of output. Blocks
/// of `std`'s plugin are laid out as it says, others as written.
pub fn format(text: &str, file: FileId) -> Result<String, Vec<Diagnostic>> {
    format_with(text, file, &|_| None)
}

/// [`format`], the blocks of each head `plugin_of` knows laid out by its
/// plugin.
pub fn format_with(
    text: &str,
    file: FileId,
    plugin_of: PluginOf,
) -> Result<String, Vec<Diagnostic>> {
    format_reporting(text, file, plugin_of).map(|(out, _)| out)
}

/// The byte ranges of the declarations left as written.
type Kept = Vec<(u32, u32)>;

/// The formatted file, and the declarations left as written.
fn format_reporting(
    text: &str,
    file: FileId,
    plugin_of: PluginOf,
) -> Result<(String, Kept), Vec<Diagnostic>> {
    let mut parsed = parse(text, file);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    // This syntax rewrite is narrower than AST normalization: explicit
    // `forall` changes polymorphic recursion and may shadow an outer variable.
    drop_redundant_foralls(&mut parsed.tree, text);
    let expected = program(&parsed.tree, text);
    drop_empty_rows(&mut parsed.tree);
    let (out, kept, failed) = source_file(&parsed.tree, text, file, plugin_of);
    // a plugin that failed leaves its block as written, and the file with it
    if !failed.is_empty() {
        return Err(failed);
    }
    let again = parse(&out, file);
    let same = again.diagnostics.is_empty()
        && program(&again.tree, &out) == expected
        && comments(&again.tree, &out) == comments(&parsed.tree, text);
    if !same {
        return Err(vec![Diagnostic::new(
            codes::P003,
            Severity::Error,
            "the formatter would change the meaning of this file; it was left unchanged",
        )]);
    }
    Ok((out, kept))
}

fn source_file(
    tree: &Node,
    text: &str,
    file: FileId,
    plugin_of: PluginOf,
) -> (String, Kept, Vec<Diagnostic>) {
    let (layouts, failed) = plugins::layouts(tree, text, file, plugin_of);
    let heads = plugins::heads(tree, text, file);
    let heads: Vec<&str> = heads.iter().map(String::as_str).collect();
    let arena = DocumentArena::new();
    let mut p = Printer::new(&arena, text, Comments::new(tree, text), &layouts);
    let declarations = nodes(tree);
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut kept = Vec::new();
    for (i, declaration) in declarations.iter().enumerate() {
        let (start, end) = declaration.range();
        for comment in p.comments.leading(start) {
            separate(&mut out, comment.blank);
            out.push_str(comment.text);
        }
        separate(&mut out, p.comments.blank_before(start));
        p.comments.misplaced = false;
        let saved = p.comments.save();
        let laid_out = p.node(declaration).to_pretty_string(WIDTH);
        let misplaced = p.comments.misplaced || p.comments.any_before(end);
        if !misplaced && verify(declaration, text, &laid_out, file, &heads) {
            out.push_str(&laid_out);
        } else {
            p.comments.restore(saved);
            p.comments.skip(end);
            out.push_str(&verbatim(declaration, text));
            kept.push((start, end));
        }
        let next = declarations.get(i + 1).map_or(u32::MAX, |d| d.range().0);
        if let Some(comment) = p.comments.trailing(next, None) {
            out.push(' ');
            out.push_str(comment);
        }
    }
    for comment in p.comments.leading(u32::MAX) {
        separate(&mut out, comment.blank);
        out.push_str(comment.text);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    (out, kept, failed)
}

/// Ends the current line, if any, leaving a blank line when asked.
fn separate(out: &mut String, blank: bool) {
    if !out.is_empty() {
        out.push('\n');
        if blank {
            out.push('\n');
        }
    }
}

/// Whether `laid_out`, parsed alone with the plugin heads `heads` of its
/// file, is the declaration it was made from.
fn verify(declaration: &Node, text: &str, laid_out: &str, file: FileId, heads: &[&str]) -> bool {
    let again = parse_with_heads(laid_out, file, heads);
    let items = nodes(&again.tree);
    again.diagnostics.is_empty()
        && matches!(items.as_slice(), [one] if program(one, laid_out) == program(declaration, text))
        && comments(&again.tree, laid_out) == comments(declaration, text)
}

/// The comments under `node`, in order: the AST has none, so they are
/// compared on their own.
fn comments<'t>(node: &Node, text: &'t str) -> Vec<&'t str> {
    node.tokens()
        .into_iter()
        .filter(|t| t.kind.is_comment())
        .map(|t| t.text(text).trim_end())
        .collect()
}

/// The source text of a declaration, without whitespace at line ends.
fn verbatim(declaration: &Node, text: &str) -> String {
    let tokens = declaration.tokens();
    let mut out = String::new();
    for (i, t) in tokens.iter().enumerate() {
        let before_newline = tokens.get(i + 1).is_none_or(|n| n.kind == K::Newline);
        if t.kind == K::Whitespace && before_newline {
            continue;
        }
        out.push_str(t.text(text));
    }
    out.trim_end().to_owned()
}

/// Drops empty rows of an arrow: `->{none; E}` becomes `->{; E}`,
/// `->{none; none}` becomes `->{}`, and `->{...S;}` becomes `->{...S}`; a row
/// holding a comment is kept.
fn drop_empty_rows(node: &mut Node) {
    if node.kind == K::EffectAnnot {
        let blank = |n: &Node| n.children.iter().all(is_whitespace);
        let empty = |n: &Node| is_bare_none(n) || blank(n);
        let rows: Vec<&Node> = node.nodes().filter(|n| n.kind == K::Row).collect();
        if rows.len() == 2 && rows.iter().all(|r| empty(r)) {
            node.children
                .retain(|c| matches!(c, Element::Token(t) if t.kind != K::Semi));
        } else if let [services, errors] = rows[..]
            && holds_set(services.nodes().map(|n| n.kind))
            && blank(errors)
        {
            let mut rows = 0;
            node.children.retain(|c| match c {
                Element::Token(t) => t.kind != K::Semi,
                Element::Node(n) => {
                    rows += usize::from(n.kind == K::Row);
                    rows < 2
                }
            });
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

#[cfg(test)]
mod tests;
