//! Optional steps `.?label` and the code they stand for.
//! - "expand `.?` to `match`" writes a path as the `match`es it means:
//!   `None` ends it, and `Some` of its last field is its value, with
//!   binders no name in scope has. The root is still read once.
//! - "read `.?label`" and "read `.label`" add or remove the `?` of a step
//!   where the value before it is, or is not, an `Option`. Both change the
//!   type of the path, so they are offered as rewrites, never as a fix.

use std::collections::HashSet;

use super::super::{Ide, locals, tree};
use super::orelse::{collect_idents, has_comment, has_error};
use super::{Action, edit, indent_at};
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::fields;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Type;

impl Ide<'_> {
    pub(super) fn expand_optional_path(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        // the outermost step of the innermost path at `offset`
        let index = (0..path.len()).rev().find(|&i| {
            path[i].kind == K::FieldExpr && (i == 0 || path[i - 1].kind != K::FieldExpr)
        })?;
        let node = path[index];
        let (root, steps) = fields::steps(node);
        let root = root?;
        if !steps.iter().any(|s| fields::is_optional(s))
            || has_error(node)
            || self.has_error_in(file, node.range())
            || has_comment(&node.text(text))
        {
            return None;
        }
        let prefix = self.option_constructor_prefix(m, text, file)?;
        let mut taken = taken_names(m, text, file, node);
        let labels: Vec<&str> = (steps.iter())
            .map(|s| label(s).map(|l| l.text(text)))
            .collect::<Option<_>>()?;
        let optional: Vec<bool> = steps.iter().map(|s| fields::is_optional(s)).collect();
        let arms = format!("{}  ", indent_at(text, node.range().0));
        let expanded = expand(
            root.text(text),
            &labels,
            &optional,
            prefix,
            &arms,
            &mut taken,
        );
        let parent = index.checked_sub(1).map(|p| path[p].kind);
        let replacement = if matches!(parent, Some(K::Block | K::ParenExpr)) {
            expanded
        } else {
            format!("({expanded})")
        };
        let (start, end) = node.range();
        Some(Action {
            title: "expand `.?` to `match`".to_owned(),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, replacement)],
        })
    }

    /// `.label` as `.?label` where the value before the step is an `Option`.
    pub(super) fn to_optional_step(&self, file: FileId, offset: u32) -> Option<Action> {
        let (step, before) = self.step_at(file, offset)?;
        let label = label(step)?.text(self.text(file));
        let dot = step.child_tokens().find(|t| t.kind == K::Dot)?;
        if fields::is_optional(step) || super::super::paths::option_item(&before).is_none() {
            return None;
        }
        Some(Action {
            title: format!("read `.?{label}`: the path becomes an `Option`"),
            kind: "refactor.rewrite",
            edits: vec![edit(file, dot.end, dot.end, "?")],
        })
    }

    /// `.?label` as `.label` where the value before the step is a record
    /// with the field, not an `Option`.
    pub(super) fn to_plain_step(&self, file: FileId, offset: u32) -> Option<Action> {
        let (step, before) = self.step_at(file, offset)?;
        let label = label(step)?.text(self.text(file));
        let mark = step.child_tokens().find(|t| t.kind == K::Question)?;
        if super::super::paths::option_item(&before).is_some()
            || self.field_type(file, &before, label).is_none()
        {
            return None;
        }
        Some(Action {
            title: format!("read `.{label}` without `.?`: the value before it is no `Option`"),
            kind: "refactor.rewrite",
            edits: vec![edit(file, mark.start, mark.end, "")],
        })
    }

    /// The step of a path whose `.` to label `offset` is in, and the value
    /// before it.
    fn step_at(&self, file: FileId, offset: u32) -> Option<(&Node, Type)> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let at = path.len().checked_sub(1)?;
        let step = path[at];
        let dot = step.child_tokens().find(|t| t.kind == K::Dot)?;
        if !(dot.start <= offset && offset <= label(step)?.end) {
            return None;
        }
        let before = match step.kind {
            K::FieldExpr => self.value_type(m, step.nodes().next()?)?,
            K::AccessorStep => {
                let accessor = path.get(at.checked_sub(1)?)?;
                let steps: Vec<&Node> = accessor.nodes().collect();
                let i = steps.iter().position(|s| s.range() == step.range())?;
                self.step_reads(m, accessor).swap_remove(i).0?
            }
            _ => return None,
        };
        Some((step, before))
    }
}

fn label(step: &Node) -> Option<Token> {
    step.child_tokens().find(|t| t.kind == K::Ident)
}

/// The names a binder of the expansion of `node` may not take.
fn taken_names(m: &Analyzed, text: &str, file: FileId, node: &Node) -> HashSet<String> {
    let mut taken: HashSet<String> = locals::in_scope(&m.tree, node.range().0, text)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    taken.extend(
        declarations(&m.tree, text, file)
            .into_iter()
            .map(|d| d.name),
    );
    collect_idents(node, text, &mut taken);
    taken
}

fn fresh(taken: &mut HashSet<String>) -> String {
    // one more candidate than names taken: one of them is free
    let name = (1..=taken.len() + 1)
        .map(|i| {
            if i == 1 {
                "value".to_owned()
            } else {
                format!("value{i}")
            }
        })
        .find(|n| !taken.contains(n))
        .unwrap_or_default();
    taken.insert(name.clone());
    name
}

/// The steps `labels` read from `value`, a `match` at each optional one,
/// its arms at `arms`.
fn expand(
    mut value: String,
    labels: &[&str],
    optional: &[bool],
    prefix: &str,
    arms: &str,
    taken: &mut HashSet<String>,
) -> String {
    for (i, label) in labels.iter().enumerate() {
        if optional[i] {
            let binder = fresh(taken);
            let inner = format!("{arms}    ");
            let rest = expand(
                format!("{binder}.{label}"),
                &labels[i + 1..],
                &optional[i + 1..],
                prefix,
                &format!("{inner}  "),
                taken,
            );
            return format!(
                "match {value} with\n{arms}| {prefix}None -> {prefix}None\n{arms}| {prefix}Some {binder} ->\n{inner}{rest}"
            );
        }
        value = format!("{value}.{label}");
    }
    format!("{prefix}Some {value}")
}
