//! Code actions for `let ... else`:
//! - expand `let pattern = initializer else fallback` to an exact `match` over
//!   the entire remainder of its enclosing block, preserving comments, local
//!   signatures, shadowing and block boundaries;
//! - rewrite a tail-position two-arm `match` (`P -> remainder | _ -> fallback`,
//!   without guards) as `let P = scrutinee else fallback`;
//! - explicit refactor replacing an irrefutable `let ... else` (with an
//!   unreachable `else` warning) with an ordinary `let`, offered only after
//!   full re-typechecking of the module confirms the edit checks cleanly.

use std::fmt::Write;

use super::super::{Ide, tree};
use super::{Action, edit, indent_at, starts_line};
use crate::diagnostics::{Severity, codes};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{self, Watched, is_pattern, let_parts};
use crate::{resolve, syntax};

impl Ide<'_> {
    pub(super) fn let_else_actions(&self, file: FileId, offset: u32) -> Vec<Action> {
        let mut out = Vec::new();
        out.extend(self.expand_let_else(file, offset));
        out.extend(self.collapse_match_to_let_else(file, offset));
        out.extend(self.replace_irrefutable_let_else(file, offset));
        out
    }

    /// Expands `let pattern = initializer else fallback` into
    /// `match initializer with | pattern -> remainder | _ -> fallback`,
    /// moving the entire remainder of the nearest enclosing block and
    /// verifying that the rewritten module typechecks.
    fn expand_let_else(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let i = path
            .iter()
            .rposition(|n| n.kind == K::LetDecl && n.has_token(K::ElseKw))?;
        let decl = path[i];
        if i == 0 || path[i - 1].kind != K::Block || !starts_line(text, decl.range().0) {
            return None;
        }
        if has_error_node(decl) {
            return None;
        }
        let block = path[i - 1];
        let siblings: Vec<&Node> = block.nodes().collect();
        let pos = siblings.iter().position(|n| n.range() == decl.range())?;
        let rest = &siblings[pos + 1..];
        let rest_end = rest.last()?.range().1;

        let parts = let_parts(decl, text);
        let (pattern, init, fallback) = (parts.pattern?, parts.body?, parts.fallback?);
        let eq = decl.child_tokens().find(|t| t.kind == K::Eq)?;
        let else_kw = decl.child_tokens().find(|t| t.kind == K::ElseKw)?;

        // Initializer must be a single expression (multi-statement blocks cannot
        // stand directly as a `match` scrutinee without introducing a synthetic local).
        let init_stmts: Vec<&Node> = init.nodes().collect();
        let [init_expr] = init_stmts[..] else {
            return None;
        };
        if matches!(
            init_expr.kind,
            K::LetDecl | K::Signature | K::AssignStmt | K::DeferStmt
        ) {
            return None;
        }
        // Do not silently drop comments sitting outside `init_expr` between `=` and `else`.
        let before_init = &text[eq.end as usize..init_expr.range().0 as usize];
        let after_init = &text[init_expr.range().1 as usize..else_kw.start as usize];
        if contains_comment(before_init) || contains_comment(after_init) {
            return None;
        }

        let (start, decl_end) = decl.range();
        let after = &text[decl_end as usize..rest_end as usize];
        let fallback_raw = &text[else_kw.end as usize..fallback.range().1 as usize];
        if contains_verbatim_multiline_string(after)
            || contains_verbatim_multiline_string(fallback_raw)
        {
            return None;
        }

        let indent = indent_at(text, start);
        let init_src = init_expr.text(text);
        if contains_verbatim_multiline_string(&init_src) {
            return None;
        }
        let scrutinee = if matches!(
            init_expr.kind,
            K::MatchExpr | K::TryExpr | K::CatchExpr | K::IfExpr | K::FunExpr
        ) || init_src.contains('\n')
        {
            format!("({init_src})")
        } else {
            init_src
        };

        // Any trailing comment on the `let-else` line belongs with the fallback arm,
        // while lines after the first newline form the success remainder.
        let (same_line_tail, rem_tail) = match after.find('\n') {
            Some(nl) => (&after[..nl], &after[nl + 1..]),
            None => return None,
        };
        let mut replacement = format!(
            "match {scrutinee} with\n{indent}| {} ->",
            pattern.text(text)
        );
        for line in rem_tail.split('\n') {
            replacement.push('\n');
            if !line.trim().is_empty() {
                replacement.push_str("  ");
            }
            replacement.push_str(line);
        }

        let fallback_arm =
            format_branch_body(text, indent, else_kw.start, fallback_raw, same_line_tail);
        let _ = write!(replacement, "\n{indent}| _ ->{fallback_arm}");

        let mut candidate = text.to_owned();
        candidate.replace_range(start as usize..rest_end as usize, &replacement);
        if !self.let_else_candidate_checks(file, &candidate, None) {
            return None;
        }
        Some(Action {
            title: "expand `let ... else` to `match`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, rest_end, replacement)],
        })
    }

    /// Rewrites a tail-position two-arm `match` (`| P -> remainder | _ -> fallback`,
    /// without guards) into `let P = scrutinee else fallback\nremainder`.
    fn collapse_match_to_let_else(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let i = path.iter().rposition(|n| n.kind == K::MatchExpr)?;
        let matched = path[i];
        if i == 0 || path[i - 1].kind != K::Block || !starts_line(text, matched.range().0) {
            return None;
        }
        let block = path[i - 1];
        // Must be the tail statement of an ordinary block (never `do` or `for` clauses).
        if block.nodes().last().map(Node::range) != Some(matched.range()) {
            return None;
        }
        if i >= 2 {
            let block_owner = path[i - 2];
            if block_owner.kind == K::DoExpr {
                return None;
            }
            if block_owner.kind == K::ForExpr
                && block_owner.nodes().next().map(Node::range) == Some(block.range())
            {
                return None;
            }
        }
        let mut children = matched.nodes();
        let scrutinee = children.next()?;
        let arms: Vec<&Node> = children.filter(|n| n.kind == K::MatchArm).collect();
        let [success_arm, fallback_arm] = arms[..] else {
            return None;
        };
        if success_arm.nodes().any(|n| n.kind == K::Guard)
            || fallback_arm.nodes().any(|n| n.kind == K::Guard)
        {
            return None;
        }
        let success_pat = success_arm.nodes().find(|n| is_pattern(n.kind))?;
        let success_body = success_arm.nodes().find(|n| n.kind == K::Block)?;
        let fallback_pat = fallback_arm.nodes().find(|n| is_pattern(n.kind))?;
        let fallback_body = fallback_arm.nodes().find(|n| n.kind == K::Block)?;
        if matches!(success_pat.kind, K::WildcardPat | K::VarPat) {
            return None;
        }
        if fallback_pat.kind != K::WildcardPat {
            let match_has_t009 = self.analysis.diagnostics.iter().any(|d| {
                d.code == codes::T009
                    && d.span.is_some_and(|s| {
                        s.file == file && s.start >= matched.range().0 && s.end <= matched.range().1
                    })
            });
            if match_has_t009 || !pat_binds_nothing(fallback_pat) {
                return None;
            }
        }
        let success_arrow = success_arm.child_tokens().find(|t| t.kind == K::Arrow)?;
        let fallback_arrow = fallback_arm.child_tokens().find(|t| t.kind == K::Arrow)?;

        // Do not silently drop comments between arm1's end and arm2's `|`.
        let between_arms = &text[success_body.range().1 as usize..fallback_arm.range().0 as usize];
        if contains_comment(between_arms) {
            return None;
        }

        let raw_rem = &text[success_arrow.end as usize..success_body.range().1 as usize];
        let raw_fb = &text[fallback_arrow.end as usize..fallback_body.range().1 as usize];
        let scrutinee_src = scrutinee.text(text);
        if contains_verbatim_multiline_string(raw_rem)
            || contains_verbatim_multiline_string(raw_fb)
            || contains_verbatim_multiline_string(&scrutinee_src)
        {
            return None;
        }

        let (start, end) = matched.range();
        let indent = indent_at(text, start);
        let pat_src = if success_pat.kind == K::OrPat {
            format!("({})", success_pat.text(text))
        } else {
            success_pat.text(text)
        };
        let init_src = if scrutinee_src.contains('\n') {
            format!("({scrutinee_src})")
        } else {
            scrutinee_src
        };

        let fb_part = format_branch_body(text, indent, fallback_arm.range().0, raw_fb, "");
        let rem_part = dedent_arm_body(raw_rem, indent)?;
        let replacement = format!("let {pat_src} = {init_src} else{fb_part}\n{rem_part}");

        let mut candidate = text.to_owned();
        candidate.replace_range(start as usize..end as usize, &replacement);
        let rewritten_end = start + u32::try_from(replacement.len()).ok()?;
        // Require zero errors AND zero T010 warnings inside the rewritten `let-else`.
        if !self.let_else_candidate_checks(file, &candidate, Some((start, rewritten_end))) {
            return None;
        }
        Some(Action {
            title: "rewrite `match` as `let ... else`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }

    /// Explicit refactor replacing an irrefutable `let ... else` (one carrying
    /// an unreachable `else` `FLICK-T010` warning) with an ordinary `let`.
    /// Offered only after full re-typechecking of the rewritten module succeeds.
    fn replace_irrefutable_let_else(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let decl = *path
            .iter()
            .rev()
            .find(|n| n.kind == K::LetDecl && n.has_token(K::ElseKw))?;
        let else_kw = decl.child_tokens().find(|t| t.kind == K::ElseKw)?;
        let parts = let_parts(decl, text);
        let init = parts.body?;
        let has_t010 = self.analysis.diagnostics.iter().any(|d| {
            d.code == codes::T010
                && d.span.is_some_and(|s| {
                    s.file == file && s.start == else_kw.start && s.end <= decl.range().1
                })
        });
        if !has_t010 {
            return None;
        }
        let remove_start = init.range().1;
        let remove_end = decl.range().1;
        if contains_comment(&text[remove_start as usize..remove_end as usize]) {
            return None;
        }
        let mut candidate = text.to_owned();
        candidate.replace_range(remove_start as usize..remove_end as usize, "");
        if !self.let_else_candidate_checks(file, &candidate, None) {
            return None;
        }
        Some(Action {
            title: "replace with `let` (removes unreachable `else`)".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, remove_start, remove_end, "")],
        })
    }

    /// Re-parses, resolves and typechecks `candidate` in the current project's
    /// interface environment. Returns `true` when it produces no errors (and,
    /// when `forbid_unreachable_in` is set, no `FLICK-T010` warnings in that range).
    fn let_else_candidate_checks(
        &self,
        file: FileId,
        candidate: &str,
        forbid_unreachable_in: Option<(u32, u32)>,
    ) -> bool {
        let Some(m) = self.module(file) else {
            return false;
        };
        let parsed = syntax::parse(candidate, file);
        if parsed
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return false;
        }
        let syntax = resolve::module_syntax(&parsed.tree, candidate, file);
        let module = resolve::Module {
            name: &m.name,
            file,
            text: candidate,
            tree: &parsed.tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let imported = |name: &str| self.interface(name).cloned();
        let (mut interface, resolved) = resolve::resolve(&module, &imported);
        if resolved.iter().any(|d| d.severity == Severity::Error) {
            return false;
        }
        let local = |_: &str| true;
        let options = types::Options {
            local: &local,
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
            foreign_backend: &self.analysis.foreign_backend,
            validate_foreign_metadata: true,
        };
        let diagnostics = types::check(
            &module,
            &imported,
            types::builtins(),
            options,
            &mut interface,
        );
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return false;
        }
        if let Some((from, to)) = forbid_unreachable_in
            && diagnostics.iter().any(|d| {
                d.code == codes::T010
                    && d.span
                        .is_some_and(|s| s.file == file && s.start >= from && s.end <= to)
            })
        {
            return false;
        }
        true
    }
}

fn pat_binds_nothing(node: &Node) -> bool {
    match node.kind {
        K::WildcardPat | K::LitPat => true,
        K::ConPat | K::TuplePat | K::ListPat | K::ParenPat | K::OrPat => node
            .nodes()
            .filter(|n| is_pattern(n.kind))
            .all(pat_binds_nothing),
        _ => false,
    }
}

fn has_error_node(node: &Node) -> bool {
    node.kind == K::Error || node.nodes().any(has_error_node)
}

fn contains_comment(text: &str) -> bool {
    text.contains("//") || text.contains("/*")
}

fn contains_verbatim_multiline_string(text: &str) -> bool {
    text.contains("\"\"\"") || text.contains("r\"") || text.contains("r#")
}

/// Formats `raw` (the slice immediately after `else` or `->`) for a target
/// branch headed at `target_indent`, preserving comments and normalizing
/// multiline indentation relative to `source_head_indent`.
fn format_branch_body(
    text: &str,
    target_indent: &str,
    source_head_offset: u32,
    raw: &str,
    trailing_comment: &str,
) -> String {
    if !raw.contains('\n') {
        let trimmed = raw.trim();
        return format!(" {trimmed}{trailing_comment}");
    }
    let source_indent = indent_at(text, source_head_offset);
    let mut lines = raw.split('\n');
    let first = lines.next().unwrap_or_default().trim_end();
    let mut out = String::new();
    if !first.is_empty() {
        out.push_str(first);
    }
    for line in lines {
        out.push('\n');
        if line.trim().is_empty() {
            continue;
        }
        let stripped = line.strip_prefix(source_indent).unwrap_or(line);
        let stripped = stripped.strip_prefix("  ").unwrap_or(stripped.trim_start());
        out.push_str(target_indent);
        out.push_str("  ");
        out.push_str(stripped);
    }
    if !trailing_comment.trim().is_empty() {
        out.push_str(trailing_comment);
    }
    out
}

/// Dedents an arm body (from after `->` to the end of its block) so its
/// statements align at `target_indent` in the enclosing block.
fn dedent_arm_body(raw: &str, target_indent: &str) -> Option<String> {
    if !raw.contains('\n') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(format!("{target_indent}{trimmed}"));
    }
    let mut lines = raw.split('\n');
    let first = lines.next().unwrap_or_default().trim();
    let mut out = Vec::new();
    if !first.is_empty() {
        out.push(format!("{target_indent}{first}"));
    }
    let deeper = format!("{target_indent}  ");
    for line in lines {
        if line.trim().is_empty() {
            out.push(String::new());
        } else if let Some(rest) = line.strip_prefix(&deeper) {
            out.push(format!("{target_indent}{rest}"));
        } else {
            return None;
        }
    }
    Some(out.join("\n"))
}
