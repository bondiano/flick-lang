//! Exact, behavior-preserving rewrites between postfix `catch` and `try … with`.

use super::super::{Ide, tree};
use super::{Action, edit, indent_at};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types::catch_to_try_text;
use crate::types::handle::{arm_pattern, failed_pattern, operation_path};

impl Ide<'_> {
    /// Expands `expr catch | …` into the equivalent `try … with` expression.
    pub(super) fn catch_to_try(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let node = *path.iter().rev().find(|n| n.kind == K::CatchExpr)?;
        // Only offer when the cursor is on the `catch` keyword or the head of the expression.
        let kw = node.child_tokens().find(|t| t.kind == K::CatchKw)?;
        let (start, end) = node.range();
        if offset < start || offset > kw.end {
            return None;
        }
        // Only expand valid typed-error-only `catch` expressions cleanly (diagnostic quick fix
        // covers invalid operation/return arms).
        for arm in node.nodes().filter(|n| n.kind == K::MatchArm) {
            if arm.has_token(K::ReturnKw)
                || arm_pattern(arm).is_some_and(|p| {
                    operation_path(p, text).is_some() || failed_pattern(p, text).is_some()
                })
            {
                return None;
            }
        }
        let replacement = catch_to_try_text(node, text)?;
        if !rewrites_cleanly(text, file, start, end, &replacement) {
            return None;
        }
        Some(Action {
            title: "rewrite `catch` as `try … with`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }

    /// Folds a typed-error-only single-expression `try … with` into postfix `catch`.
    pub(super) fn try_to_catch(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let pos = path.iter().rposition(|n| n.kind == K::TryExpr)?;
        let node = path[pos];
        let try_kw = node.child_tokens().find(|t| t.kind == K::TryKw)?;
        let with_kw = node.child_tokens().find(|t| t.kind == K::WithKw)?;
        if !(try_kw.start <= offset && offset <= with_kw.end) {
            return None;
        }
        let body = node.nodes().next().filter(|n| n.kind == K::Block)?;
        let stmts: Vec<&Node> = body.nodes().collect();
        let [expr] = stmts.as_slice() else {
            return None;
        };
        if matches!(expr.kind, K::LetDecl | K::Signature | K::AssignStmt) {
            return None;
        }
        let arms: Vec<&Node> = node.nodes().filter(|n| n.kind == K::MatchArm).collect();
        if arms.is_empty() {
            return None;
        }
        for arm in &arms {
            if arm.has_token(K::ReturnKw)
                || arm_pattern(arm).is_some_and(|p| {
                    operation_path(p, text).is_some() || failed_pattern(p, text).is_some()
                })
            {
                return None;
            }
        }
        let (start, end) = node.range();
        let base_indent = indent_at(text, start);
        let leading_between_try_and_expr = &text[try_kw.end as usize..expr.range().0 as usize];
        // Do not discard comments standing between `try` and its body expression.
        if leading_between_try_and_expr.contains("//")
            || leading_between_try_and_expr.contains("/*")
        {
            return None;
        }
        let raw_expr = &text[expr.range().0 as usize..expr.range().1 as usize];
        let mut expr_text = if leading_between_try_and_expr.contains('\n') {
            let expr_indent = indent_at(text, expr.range().0);
            let extra = expr_indent.len().saturating_sub(base_indent.len());
            dedent_tail(raw_expr, extra)
        } else {
            raw_expr.to_owned()
        };
        if expr.kind == K::CatchExpr {
            expr_text = format!("({expr_text})");
        }
        let between = &text[expr.range().1 as usize..with_kw.start as usize];
        let after = &text[with_kw.end as usize..end as usize];
        let sep = if matches!(
            expr.kind,
            K::IfExpr | K::MatchExpr | K::TryExpr | K::FunExpr | K::ForExpr | K::WhileExpr
        ) && !between.contains('\n')
        {
            format!("{}\n{base_indent}", between.trim_end())
        } else {
            between.to_owned()
        };
        let mut replacement = format!("{expr_text}{sep}catch{after}");
        let needs_outer_parens = pos > 0
            && matches!(
                path[pos - 1].kind,
                K::PrefixExpr
                    | K::BinExpr
                    | K::AppExpr
                    | K::RangeExpr
                    | K::FieldExpr
                    | K::QuestionExpr
            );
        if needs_outer_parens {
            replacement = format!("({replacement})");
        }
        if !rewrites_cleanly(text, file, start, end, &replacement) {
            return None;
        }
        Some(Action {
            title: "rewrite `try … with` as `catch`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }
}

fn dedent_tail(text: &str, strip: usize) -> String {
    if strip == 0 || !text.contains('\n') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            let spaces = line.bytes().take_while(|&b| b == b' ').count().min(strip);
            out.push_str(&line[spaces..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn rewrites_cleanly(text: &str, file: FileId, start: u32, end: u32, replacement: &str) -> bool {
    let mut updated = String::with_capacity(text.len() + replacement.len());
    updated.push_str(&text[..start as usize]);
    updated.push_str(replacement);
    updated.push_str(&text[end as usize..]);
    parse(&updated, file).diagnostics.is_empty()
}
