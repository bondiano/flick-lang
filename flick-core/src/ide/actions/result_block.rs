//! Safe bidirectional refactorings between `result { BODY }` and its
//! canonical `try BODY with | return x -> Ok x | e -> Err e` form.

use std::collections::HashSet;

use super::super::{Ide, tree};
use super::{Action, edit, indent_at};
use crate::iface::Namespace;
use crate::project::Analyzed;
use crate::resolve::{ModuleSyntax, module_syntax};
use crate::source::FileId;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Ref;
use crate::types::handle::{arm_pattern, failed_pattern};
use crate::types::is_pattern;
use crate::types::lang::{self, Lang};

impl Ide<'_> {
    /// Offers rewriting canonical `try … with | return x -> Ok x | e -> Err e`
    /// as `result { … }`, or expanding `result { … }` back to `try … with`.
    pub(super) fn result_block(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        for (i, &node) in path.iter().enumerate().rev() {
            match node.kind {
                K::TryExpr => return rewrite_try_to_result(file, m, text, node),
                K::ResultExpr => {
                    let parent = i.checked_sub(1).map(|p| path[p]);
                    let grandparent = i.checked_sub(2).map(|p| path[p]);
                    return expand_result_to_try(file, m, text, node, parent, grandparent);
                }
                _ => {}
            }
        }
        None
    }
}

/// Forward rewrite: `try BODY with | return x -> Ok x | e -> Err e` -> `result { BODY }`.
fn rewrite_try_to_result(
    file: FileId,
    m: &Analyzed,
    text: &str,
    try_node: &Node,
) -> Option<Action> {
    if has_error(try_node) {
        return None;
    }
    let try_kw = try_node.child_tokens().find(|t| t.kind == K::TryKw)?;
    let with_kw = try_node.child_tokens().find(|t| t.kind == K::WithKw)?;
    let mut children = try_node.nodes();
    let body = children.next()?;
    if body.kind != K::Block || body.nodes().next().is_none() {
        return None;
    }
    let arms: Vec<&Node> = children.collect();
    if arms.len() != 2 || arms.iter().any(|n| n.kind != K::MatchArm) {
        return None;
    }
    // Never silently drop comments attached to `with` or either arm.
    if has_comment_from(try_node, with_kw.start) {
        return None;
    }
    let (ret_arm, err_arm) = match (
        arms[0].has_token(K::ReturnKw),
        arms[1].has_token(K::ReturnKw),
    ) {
        (true, false) => (arms[0], arms[1]),
        (false, true) => (arms[1], arms[0]),
        _ => return None,
    };
    if !is_canonical_arm(m, text, ret_arm, Lang::Ok, false)
        || !is_canonical_arm(m, text, err_arm, Lang::Err, true)
    {
        return None;
    }
    let raw_body = &text[try_kw.end as usize..with_kw.start as usize];
    let body_has_comment = has_comment_in(try_node, try_kw.end, with_kw.start);
    let body_text = body.text(text);
    let trimmed = body_text.trim();
    let (start, end) = try_node.range();
    let replacement = if !body_has_comment && !trimmed.contains('\n') && body.nodes().count() == 1 {
        format!("result {{ {trimmed} }}")
    } else {
        let base_indent = indent_at(text, start);
        let formatted = reindent_block(raw_body, &format!("{base_indent}  "));
        format!("result {{\n{formatted}\n{base_indent}}}")
    };
    Some(Action {
        title: "rewrite as `result { … }`".to_owned(),
        kind: "refactor.rewrite",
        edits: vec![edit(file, start, end, replacement)],
    })
}

/// Whether `arm` is an exact, unguarded canonical identity wrapper:
/// `| return x -> Ok x` (`allow_fail_pat = false`, `ctor = Lang::Ok`) or
/// `| e -> Err e` / `| Fail.fail e -> Err e` (`allow_fail_pat = true`, `ctor = Lang::Err`).
fn is_canonical_arm(
    m: &Analyzed,
    text: &str,
    arm: &Node,
    ctor: Lang,
    allow_fail_pat: bool,
) -> bool {
    if arm.nodes().any(|n| n.kind == K::Guard) {
        return false;
    }
    let Some(pat) = arm_pattern(arm) else {
        return false;
    };
    let Some(binder) = canonical_binder(pat, text, allow_fail_pat) else {
        return false;
    };
    let Some(block) = arm.nodes().last().filter(|n| n.kind == K::Block) else {
        return false;
    };
    let mut stmts = block.nodes();
    let (Some(expr), None) = (stmts.next(), stmts.next()) else {
        return false;
    };
    let expr = unparen_expr(expr);
    if expr.kind != K::AppExpr {
        return false;
    }
    let mut parts = expr.nodes();
    let (Some(head), Some(arg), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let head = unparen_expr(head);
    let arg = unparen_expr(arg);
    if head.kind != K::NameExpr || arg.kind != K::NameExpr {
        return false;
    }
    let Some(Ref::Constructor(c)) = m.typing.names.get(&head.range()) else {
        return false;
    };
    if !lang::is(ctor, &c.module, &c.name) {
        return false;
    }
    if !matches!(m.typing.names.get(&arg.range()), Some(Ref::Local)) {
        return false;
    }
    let Some(arg_token) = arg.child_tokens().find(|t| t.kind == K::Ident) else {
        return false;
    };
    arg_token.text(text) == binder
}

/// Extracts a plain unannotated variable binder from `pat` (or `Fail.fail pat`
/// when `allow_fail_pat` is true). Rejects wildcards, annotations, and patterns
/// that narrow or destructure the error/value.
fn canonical_binder<'t>(mut pat: &Node, text: &'t str, allow_fail_pat: bool) -> Option<&'t str> {
    while pat.kind == K::ParenPat {
        pat = pat.nodes().find(|n| is_pattern(n.kind))?;
    }
    if allow_fail_pat && let Some(inner) = failed_pattern(pat, text) {
        pat = inner;
        while pat.kind == K::ParenPat {
            pat = pat.nodes().find(|n| is_pattern(n.kind))?;
        }
    }
    if pat.kind != K::VarPat {
        return None;
    }
    let tok = pat.child_tokens().find(|t| t.kind == K::Ident)?;
    Some(tok.text(text))
}

fn unparen_expr(mut node: &Node) -> &Node {
    while node.kind == K::ParenExpr {
        let Some(inner) = node.nodes().next() else {
            break;
        };
        node = inner;
    }
    node
}

/// Reverse rewrite: `result { BODY }` -> `try BODY with | return v -> Ok v | e -> Err e`.
fn expand_result_to_try(
    file: FileId,
    m: &Analyzed,
    text: &str,
    result_node: &Node,
    parent: Option<&Node>,
    grandparent: Option<&Node>,
) -> Option<Action> {
    if has_error(result_node) {
        return None;
    }
    let lbrace = result_node.child_tokens().find(|t| t.kind == K::LBrace)?;
    let rbrace = result_node.child_tokens().find(|t| t.kind == K::RBrace)?;
    let body = result_node.nodes().find(|n| n.kind == K::Block)?;
    body.nodes().next()?;
    let (ok_con, err_con) = resolve_result_constructors(file, m, text)?;
    let used = collect_idents(result_node, text);
    let val_name = fresh_name(&used, &["value", "v"]);
    let mut used_err = used;
    used_err.insert(val_name.clone());
    let err_name = fresh_name(&used_err, &["err", "e"]);

    let raw_body = &text[lbrace.end as usize..rbrace.start as usize];
    let body_has_comment = has_comment_in(result_node, lbrace.end, rbrace.start);
    let body_text = body.text(text);
    let trimmed = body_text.trim();
    let (start, end) = result_node.range();
    let base_indent = indent_at(text, start);
    let wrapped = needs_parens(result_node, parent, grandparent);
    let arm_indent = if wrapped {
        format!("{base_indent}  ")
    } else {
        base_indent.to_owned()
    };
    let expanded = if !body_has_comment && !trimmed.contains('\n') && body.nodes().count() == 1 {
        format!(
            "try {trimmed} with\n\
             {arm_indent}| return {val_name} -> {ok_con} {val_name}\n\
             {arm_indent}| {err_name} -> {err_con} {err_name}"
        )
    } else {
        let inner_indent = format!("{arm_indent}  ");
        let formatted = reindent_block(raw_body, &inner_indent);
        format!(
            "try\n\
             {formatted}\n\
             {arm_indent}with\n\
             {arm_indent}| return {val_name} -> {ok_con} {val_name}\n\
             {arm_indent}| {err_name} -> {err_con} {err_name}"
        )
    };
    let replacement = if wrapped {
        format!("(\n{arm_indent}{expanded}\n{base_indent})")
    } else {
        expanded
    };
    Some(Action {
        title: "expand `result { … }` to `try … with`".to_owned(),
        kind: "refactor.rewrite",
        edits: vec![edit(file, start, end, replacement)],
    })
}

/// Whether `result { … }` expanded to `try … with` needs enclosing parentheses
/// so surrounding syntax keeps the exact same parse tree.
fn needs_parens(result_node: &Node, parent: Option<&Node>, grandparent: Option<&Node>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    match parent.kind {
        K::ParenExpr | K::AnnotExpr => false,
        K::Block => {
            let is_last = parent
                .nodes()
                .last()
                .is_some_and(|last| last.range() == result_node.range());
            if !is_last {
                return true;
            }
            // In `if c then result { … } else …`, an unparenthesized `try`
            // before `else` can conflict with layout.
            grandparent.is_some_and(|gp| {
                gp.kind == K::IfExpr
                    && gp
                        .nodes()
                        .last()
                        .is_some_and(|last| last.range() != parent.range())
            })
        }
        _ => true,
    }
}

/// Spellings of Prelude's `Ok` and `Err` safe in this module: unqualified
/// unless locally shadowed, in which case `Prelude.Ok` / `Prelude.Err` (unless
/// `Prelude` itself is shadowed as an import alias).
fn resolve_result_constructors(
    file: FileId,
    m: &Analyzed,
    text: &str,
) -> Option<(&'static str, &'static str)> {
    let syntax = module_syntax(&m.tree, text, file);
    let prelude_shadowed = syntax
        .imports
        .iter()
        .any(|imp| imp.qualifier() == "Prelude" && imp.module != "Prelude");
    let ok_shadowed = constructor_shadowed(m, &syntax, "Ok");
    let err_shadowed = constructor_shadowed(m, &syntax, "Err");
    if (ok_shadowed || err_shadowed) && prelude_shadowed {
        return None;
    }
    let ok = if ok_shadowed { "Prelude.Ok" } else { "Ok" };
    let err = if err_shadowed { "Prelude.Err" } else { "Err" };
    Some((ok, err))
}

fn constructor_shadowed(m: &Analyzed, syntax: &ModuleSyntax, name: &str) -> bool {
    if m.typing
        .declarations
        .find(Namespace::Constructor, name)
        .is_some()
    {
        return true;
    }
    for imp in &syntax.imports {
        if imp.module == "Prelude" {
            continue;
        }
        if imp.exposing.iter().any(|item| item.name == name) {
            return true;
        }
    }
    false
}

fn collect_idents(node: &Node, text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_idents_rec(node, text, &mut out);
    out
}

fn collect_idents_rec(node: &Node, text: &str, out: &mut HashSet<String>) {
    for child in &node.children {
        match child {
            Element::Token(t) if t.kind == K::Ident => {
                out.insert(t.text(text).to_owned());
            }
            Element::Node(n) => collect_idents_rec(n, text, out),
            Element::Token(_) => {}
        }
    }
}

fn fresh_name(used: &HashSet<String>, preferred: &[&str]) -> String {
    for &candidate in preferred {
        if !used.contains(candidate) {
            return candidate.to_owned();
        }
    }
    let base = preferred.last().copied().unwrap_or("x");
    for i in 1.. {
        let candidate = format!("{base}{i}");
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("fresh names are infinite")
}

fn reindent_block(raw: &str, target_indent: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return String::new();
    };
    let slice = &lines[first..=last];
    // When the first line came directly after `try` or `{` on the same line,
    // its leading whitespace is just the space after the opener, not block
    // indentation; measure common indent across lines that actually start on
    // their own line.
    let starts_inline = first == 0 && !raw.starts_with('\n') && !raw.starts_with("\r\n");
    let common = slice
        .iter()
        .enumerate()
        .filter(|&(i, l)| !(l.trim().is_empty() || starts_inline && i == 0))
        .map(|(_, l)| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let mut out = Vec::with_capacity(slice.len());
    for (i, line) in slice.iter().enumerate() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            out.push(String::new());
        } else if starts_inline && i == 0 {
            out.push(format!("{target_indent}{}", trimmed_end.trim_start()));
        } else {
            let strip = common.min(trimmed_end.len() - trimmed_end.trim_start_matches(' ').len());
            out.push(format!("{target_indent}{}", &trimmed_end[strip..]));
        }
    }
    out.join("\n")
}

fn has_error(node: &Node) -> bool {
    if node.kind == K::Error {
        return true;
    }
    node.nodes().any(has_error)
}

fn has_comment_from(node: &Node, from: u32) -> bool {
    for child in &node.children {
        match child {
            Element::Token(t) if t.kind.is_comment() && t.start >= from => return true,
            Element::Node(n) if n.range().1 >= from && has_comment_from(n, from) => return true,
            _ => {}
        }
    }
    false
}

fn has_comment_in(node: &Node, start: u32, end: u32) -> bool {
    for child in &node.children {
        match child {
            Element::Token(t) if t.kind.is_comment() && t.start >= start && t.end <= end => {
                return true;
            }
            Element::Node(n)
                if n.range().1 >= start && n.range().0 <= end && has_comment_in(n, start, end) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}
