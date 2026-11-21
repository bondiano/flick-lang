//! Exact `orelse` <-> `match` rewrites:
//! - "expand `orelse` to `match`" turns `left orelse right` into its canonical
//!   `match left with | Some value -> value | None -> right` with a hygienic
//!   binder and preserved comments.
//! - "fold `match` to `orelse`" turns a two-arm `Option` match whose `Some`
//!   arm returns its single variable unchanged and whose `None` arm is a
//!   single expression into `left orelse right`.

use std::collections::HashSet;
use std::fmt::Write;

use super::super::{Ide, locals, tree};
use super::{Action, edit, indent_at};
use crate::diagnostics::Severity;
use crate::iface::Namespace;
use crate::project::Analyzed;
use crate::resolve::{ItemKind, declarations, module_syntax};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Ref;
use crate::types::lang::{self, Lang};

impl Ide<'_> {
    pub(super) fn expand_orelse(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let index = path
            .iter()
            .rposition(|n| n.kind == K::BinExpr && n.has_token(K::OrElse))?;
        let node = path[index];
        if has_error(node) || self.has_error_in(file, node.range()) {
            return None;
        }
        let op = node.child_tokens().find(|t| t.kind == K::OrElse)?;
        let children: Vec<&Node> = node.nodes().collect();
        let [lhs, rhs] = children[..] else {
            return None;
        };
        let prefix = self.option_constructor_prefix(m, text, file)?;
        let binder = fresh_binder(m, text, file, node.range().0, lhs, rhs);

        let base_indent = indent_at(text, node.range().0);
        let arm_indent = format!("{base_indent}  ");
        let lhs_text = lhs.text(text);
        let rhs_text = rhs.text(text);

        let before_op = extract_comments(&text[lhs.range().1 as usize..op.start as usize]);
        let after_op = extract_comments(&text[op.end as usize..rhs.range().0 as usize]);

        let mut expanded = format!("match {lhs_text} with");
        for comment in before_op {
            expanded.push('\n');
            expanded.push_str(&arm_indent);
            expanded.push_str(&comment);
        }
        let _ = write!(
            expanded,
            "\n{arm_indent}| {prefix}Some {binder} -> {binder}"
        );
        for comment in after_op {
            expanded.push('\n');
            expanded.push_str(&arm_indent);
            expanded.push_str(&comment);
        }
        if rhs_text.contains('\n') {
            let body_indent = format!("{arm_indent}  ");
            let indented = reindent_multiline(&rhs_text, &body_indent);
            let _ = write!(expanded, "\n{arm_indent}| {prefix}None ->\n{indented}");
        } else {
            let _ = write!(expanded, "\n{arm_indent}| {prefix}None -> {rhs_text}");
        }

        let parent = index.checked_sub(1).map(|p| path[p].kind);
        let replacement = if matches!(parent, Some(K::Block | K::ParenExpr)) {
            expanded
        } else {
            format!("({expanded})")
        };
        let (start, end) = node.range();
        Some(Action {
            title: "expand `orelse` to `match`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }

    pub(super) fn fold_match_to_orelse(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let index = path.iter().rposition(|n| n.kind == K::MatchExpr)?;
        let node = path[index];
        if has_error(node) || self.has_error_in(file, node.range()) {
            return None;
        }
        let children: Vec<&Node> = node.nodes().collect();
        let [lhs, first_arm, second_arm] = children[..] else {
            return None;
        };
        if first_arm.kind != K::MatchArm || second_arm.kind != K::MatchArm {
            return None;
        }

        let mut saw_some = false;
        let mut fallback: Option<&Node> = None;
        for arm in [first_arm, second_arm] {
            if arm.nodes().any(|c| c.kind == K::Guard) {
                return None;
            }
            let arm_children: Vec<&Node> = arm.nodes().collect();
            let [pat, block] = arm_children[..] else {
                return None;
            };
            if pat.kind != K::ConPat || block.kind != K::Block {
                return None;
            }
            let Some(Ref::Constructor(ctor)) = m.typing.names.get(&pat.range()) else {
                return None;
            };
            let body_expr = single_expr(block)?;
            if lang::is(Lang::Some, &ctor.module, &ctor.name) {
                if saw_some {
                    return None;
                }
                let pat_args: Vec<&Node> = pat.nodes().collect();
                let [var_pat] = pat_args[..] else {
                    return None;
                };
                if var_pat.kind != K::VarPat {
                    return None;
                }
                if body_expr.kind != K::NameExpr
                    || body_expr.text(text) != var_pat.text(text)
                    || !matches!(m.typing.names.get(&body_expr.range()), Some(Ref::Local))
                {
                    return None;
                }
                saw_some = true;
            } else if lang::is(Lang::None, &ctor.module, &ctor.name) {
                if fallback.is_some() || pat.nodes().next().is_some() {
                    return None;
                }
                fallback = Some(body_expr);
            } else {
                return None;
            }
        }
        if !saw_some {
            return None;
        }
        let rhs = fallback?;
        if has_dropped_comments(text, node.range(), lhs.range(), rhs.range()) {
            return None;
        }

        let lhs_text = lhs.text(text);
        let rhs_text = rhs.text(text);
        let lhs_shown = if needs_lhs_parens(lhs) {
            format!("({lhs_text})")
        } else {
            lhs_text
        };
        let rhs_shown = if needs_rhs_parens(rhs) {
            format!("({rhs_text})")
        } else {
            rhs_text
        };
        let folded = format!("{lhs_shown} orelse {rhs_shown}");
        let parent = index.checked_sub(1).map(|p| path[p].kind);
        let replacement = if matches!(parent, Some(K::Block | K::ParenExpr)) {
            folded
        } else {
            format!("({folded})")
        };
        let (start, end) = node.range();
        Some(Action {
            title: "fold `match` to `orelse`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }

    fn has_error_in(&self, file: FileId, (start, end): (u32, u32)) -> bool {
        self.analysis.diagnostics.iter().any(|d| {
            d.severity == Severity::Error
                && d.span
                    .is_some_and(|s| s.file == file && s.start < end && start < s.end)
        })
    }

    /// Prefix for `Some` / `None` in expanded `match`: empty when Prelude's
    /// constructors are unshadowed, `"Option."` when shadowed locally or via
    /// another import while `Option` stays available, or `None` if unsafe.
    fn option_constructor_prefix(
        &self,
        m: &Analyzed,
        text: &str,
        file: FileId,
    ) -> Option<&'static str> {
        let local_shadow = declarations(&m.tree, text, file).into_iter().any(|d| {
            d.namespace == Namespace::Constructor && matches!(d.name.as_str(), "Some" | "None")
        });
        let syntax = module_syntax(&m.tree, text, file);
        let import_shadow = syntax.imports.iter().any(|imp| {
            imp.module != "Option"
                && imp.module != lang::prelude()
                && imp.exposing.iter().any(|item| {
                    matches!(item.name.as_str(), "Some" | "None")
                        || (item.kind == ItemKind::ConAll && item.name == "Option")
                        || (item.kind == ItemKind::All
                            && self.interface(&imp.module).is_some_and(|iface| {
                                iface.find(Namespace::Constructor, "Some").is_some()
                                    || iface.find(Namespace::Constructor, "None").is_some()
                            }))
                })
        });
        if !local_shadow && !import_shadow {
            return Some("");
        }
        let option_shadowed = syntax
            .imports
            .iter()
            .any(|imp| imp.qualifier() == "Option" && imp.module != "Option");
        (!option_shadowed).then_some("Option.")
    }
}

fn has_error(node: &Node) -> bool {
    node.kind == K::Error || node.nodes().any(has_error)
}

fn single_expr(block: &Node) -> Option<&Node> {
    let mut nodes = block.nodes();
    match (nodes.next(), nodes.next()) {
        (Some(expr), None)
            if !matches!(
                expr.kind,
                K::LetDecl | K::Signature | K::AssignStmt | K::Error | K::TypeDecl | K::ImportDecl
            ) =>
        {
            Some(expr)
        }
        _ => None,
    }
}

fn needs_lhs_parens(node: &Node) -> bool {
    match node.kind {
        K::BinExpr => node
            .child_tokens()
            .any(|t| matches!(t.kind, K::OrElse | K::LtPipe | K::PipeGt)),
        K::FunExpr
        | K::IfExpr
        | K::MatchExpr
        | K::TryExpr
        | K::ProvideExpr
        | K::ScopeExpr
        | K::WhileExpr
        | K::ForExpr
        | K::LoopExpr => true,
        _ => false,
    }
}

fn needs_rhs_parens(node: &Node) -> bool {
    node.kind == K::BinExpr
        && node
            .child_tokens()
            .any(|t| matches!(t.kind, K::LtPipe | K::PipeGt))
}

fn fresh_binder(m: &Analyzed, text: &str, file: FileId, at: u32, lhs: &Node, rhs: &Node) -> String {
    let mut taken: HashSet<String> = locals::in_scope(&m.tree, at, text)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for decl in declarations(&m.tree, text, file) {
        taken.insert(decl.name);
    }
    collect_idents(lhs, text, &mut taken);
    collect_idents(rhs, text, &mut taken);
    (1..=taken.len() + 1)
        .map(|i| {
            if i == 1 {
                "value".to_owned()
            } else {
                format!("value{i}")
            }
        })
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| "value".to_owned())
}

fn collect_idents(node: &Node, text: &str, out: &mut HashSet<String>) {
    for token in node.child_tokens() {
        if token.kind == K::Ident {
            out.insert(token.text(text).to_owned());
        }
    }
    for child in node.nodes() {
        collect_idents(child, text, out);
    }
}

fn has_comment(slice: &str) -> bool {
    slice.contains("//") || slice.contains("/*")
}

fn has_dropped_comments(text: &str, whole: (u32, u32), lhs: (u32, u32), rhs: (u32, u32)) -> bool {
    let mut keep = [lhs, rhs];
    keep.sort_unstable();
    let mut cursor = whole.0 as usize;
    for (s, e) in keep {
        let s = s as usize;
        if cursor < s && has_comment(&text[cursor..s]) {
            return true;
        }
        cursor = cursor.max(e as usize);
    }
    cursor < whole.1 as usize && has_comment(&text[cursor..whole.1 as usize])
}

fn extract_comments(slice: &str) -> Vec<String> {
    if !has_comment(slice) {
        return Vec::new();
    }
    slice
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn reindent_multiline(source: &str, indent: &str) -> String {
    let mut lines = source.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let rest: Vec<&str> = lines.collect();
    let min_indent = rest
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let mut out = format!("{indent}{first}");
    for line in rest {
        out.push('\n');
        if !line.trim().is_empty() {
            let trimmed = &line[min_indent.min(line.len())..];
            out.push_str(indent);
            out.push_str(trimmed);
        }
    }
    out
}
