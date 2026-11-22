//! Explicit refactors between `defer { … }` / `errdefer { … }` and canonical
//! `Scope.defer` calls. Offered as `refactor.rewrite` actions only when the
//! lexical owner is preserved without ambiguity or shadowing.

use super::super::{Ide, locals, tree};
use super::{Action, edit, indent_at};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Owner, Ref};

impl Ide<'_> {
    pub(super) fn defer_actions(&self, file: FileId, offset: u32) -> Vec<Action> {
        let mut out = Vec::new();
        if let Some(action) = self.expand_defer(file, offset) {
            out.push(action);
        }
        if let Some(action) = self.simplify_scope_defer(file, offset) {
            out.push(action);
        }
        out
    }

    /// Rewrites `defer { cleanup }` / `errdefer { cleanup }` to canonical
    /// `Scope.defer` when its lexical owner matches the nearest enclosing `scope`.
    fn expand_defer(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let stmt = path.iter().rev().find(|n| n.kind == K::DeferStmt)?;
        let kw = stmt
            .child_tokens()
            .find(|t| matches!(t.kind, K::DeferKw | K::ErrdeferKw))?;
        let info = m.typing.defers.get(&(kw.start, kw.end))?;
        // Only offer when the exact same lexical `scope` would own `Scope.defer` here.
        if nearest_explicit_scope(&path, stmt)? != info.owner_node {
            return None;
        }
        let body = stmt.nodes().next()?;
        let body_range = body.range();
        let body_text = text[body_range.0 as usize..body_range.1 as usize].trim();
        if body_text.is_empty() {
            return None;
        }
        let stmt_range = stmt.range();
        let indent = indent_at(text, stmt_range.0);
        let multiline = body_text.contains('\n');

        let (title, replacement) = if info.is_errdefer {
            let exit = fresh_exit_binder(&m.tree, body, offset, text);
            let rewritten = if multiline {
                let inner = reindent(body_text, &format!("{indent}    "));
                format!(
                    "Scope.defer (fun {exit} ->\n{indent}  if Exit.is_failure {exit} then {{\n{inner}\n{indent}  }} else ())"
                )
            } else {
                format!(
                    "Scope.defer (fun {exit} -> if Exit.is_failure {exit} then {{ {body_text} }} else ())"
                )
            };
            ("expand `errdefer` to `Scope.defer`".to_owned(), rewritten)
        } else {
            let rewritten = if multiline {
                let inner = reindent(body_text, &format!("{indent}  "));
                format!("Scope.defer (fun _ ->\n{inner})")
            } else {
                format!("Scope.defer (fun _ -> {body_text})")
            };
            ("expand `defer` to `Scope.defer`".to_owned(), rewritten)
        };

        Some(Action {
            title,
            kind: "refactor.rewrite",
            edits: vec![edit(file, stmt_range.0, stmt_range.1, replacement)],
        })
    }

    /// Rewrites `Scope.defer (fun _ -> body)` to `defer { body }` when it
    /// targets the nearest enclosing explicit `scope` in valid statement position.
    fn simplify_scope_defer(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let app_idx = path.iter().rposition(|n| n.kind == K::AppExpr)?;
        let app = path[app_idx];
        let children: Vec<&Node> = app.nodes().collect();
        let [callee, arg] = children.as_slice() else {
            return None;
        };
        if !matches!(
            m.typing.names.get(&callee.range()),
            Some(Ref::Value { module, name }) if module == "Scope" && name == "defer"
        ) {
            return None;
        }
        // Only simplify when targeting the exact nearest enclosing explicit `scope`,
        // never generic `Scope s` helpers or outer scopes across boundaries.
        let Owner::Scope(owner_node) = m.typing.owners.get(&app.range()).copied()? else {
            return None;
        };
        if nearest_explicit_scope(&path[..=app_idx], app)? != owner_node {
            return None;
        }
        if !valid_defer_statement_position(&path[..=app_idx]) {
            return None;
        }
        let fun = unwrap_paren(arg);
        if fun.kind != K::FunExpr {
            return None;
        }
        let params: Vec<&Node> = fun.nodes().filter(|n| n.kind != K::Block).collect();
        let [param] = params.as_slice() else {
            return None;
        };
        if param.kind != K::WildcardPat {
            return None;
        }
        let body = fun.nodes().find(|n| n.kind == K::Block)?;
        let body_range = body.range();
        let body_text = text[body_range.0 as usize..body_range.1 as usize].trim();
        if body_text.is_empty() {
            return None;
        }
        let app_range = app.range();
        let indent = indent_at(text, app_range.0);
        let replacement = if body_text.contains('\n') {
            let inner = reindent(body_text, &format!("{indent}  "));
            format!("defer {{\n{inner}\n{indent}}}")
        } else {
            format!("defer {{ {body_text} }}")
        };
        Some(Action {
            title: "simplify `Scope.defer` to `defer`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, app_range.0, app_range.1, replacement)],
        })
    }
}

/// Returns the nearest lexically enclosing explicit `ScopeExpr` range without
/// crossing a deferred execution or cleanup boundary.
fn nearest_explicit_scope(path: &[&Node], target: &Node) -> Option<(u32, u32)> {
    let target_range = target.range();
    for (i, &node) in path.iter().enumerate().rev() {
        if node.range() == target_range {
            continue;
        }
        match node.kind {
            K::ScopeExpr => return Some(node.range()),
            K::FunExpr | K::TaskExpr | K::LazyExpr | K::DeferStmt => return None,
            K::LetDecl
                if node.nodes().any(|c| c.kind == K::Params) || node.has_token(K::LazyKw) =>
            {
                return None;
            }
            K::ForExpr if !node.has_token(K::DoKw) => return None,
            K::MatchArm
                if i > 0
                    && path[i - 1].kind == K::TryExpr
                    && node
                        .nodes()
                        .next()
                        .is_some_and(|p| p.kind == K::ConPat && p.has_token(K::Dot)) =>
            {
                return None;
            }
            _ => {}
        }
    }
    None
}

fn valid_defer_statement_position(path: &[&Node]) -> bool {
    let Some(&node) = path.last() else {
        return false;
    };
    let Some(&parent) = path.len().checked_sub(2).and_then(|i| path.get(i)) else {
        return false;
    };
    if parent.kind != K::Block {
        return false;
    }
    let Some(&grand) = path.len().checked_sub(3).and_then(|i| path.get(i)) else {
        return true;
    };
    match grand.kind {
        K::DoExpr => false,
        K::ForExpr => grand
            .nodes()
            .next()
            .is_none_or(|clauses| clauses.range() != parent.range()),
        K::LetDecl if !grand.nodes().any(|c| c.kind == K::Params) => parent
            .nodes()
            .last()
            .is_some_and(|last| last.range() != node.range()),
        K::AssignStmt | K::RecordField | K::ProofDecl => parent
            .nodes()
            .last()
            .is_some_and(|last| last.range() != node.range()),
        _ => true,
    }
}

fn unwrap_paren(mut node: &Node) -> &Node {
    while node.kind == K::ParenExpr {
        let Some(inner) = node.nodes().next() else {
            break;
        };
        node = inner;
    }
    node
}

/// Picks a binder name for `Exit` that does not shadow locals in scope or names
/// referenced inside `body`.
fn fresh_exit_binder(tree: &Node, body: &Node, offset: u32, text: &str) -> String {
    let in_scope: Vec<String> = locals::in_scope(tree, offset, text)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let body_text = &text[body.range().0 as usize..body.range().1 as usize];
    for candidate in ["exit", "scope_exit", "_scope_exit"] {
        if !in_scope.iter().any(|n| n == candidate) && !contains_ident(body_text, candidate) {
            return candidate.to_owned();
        }
    }
    for i in 1.. {
        let candidate = format!("_scope_exit_{i}");
        if !in_scope.iter().any(|n| n == &candidate) && !contains_ident(body_text, &candidate) {
            return candidate;
        }
    }
    unreachable!("fresh names never run out")
}

fn contains_ident(haystack: &str, ident: &str) -> bool {
    haystack
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|word| word == ident)
}

fn reindent(block: &str, target_indent: &str) -> String {
    let lines: Vec<&str> = block.lines().collect();
    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let trimmed = if i == 0 {
                line.trim_start()
            } else {
                let strip = min_indent.min(line.len() - line.trim_start_matches(' ').len());
                &line[strip..]
            };
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("{target_indent}{trimmed}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
