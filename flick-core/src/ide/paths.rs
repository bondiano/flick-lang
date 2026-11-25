//! Field paths: the steps of an accessor `.address.city` and projections
//! `user.address`, and the record each `.` reads a field of. The checker
//! keeps, at each label, the record it belongs to, settled once the module
//! is solved, and at an accessor its whole type. A step still without its
//! label reads the field of the step before it; the first step reads the
//! accessor's parameter, which the accessor hides: it has no name.

use super::hints::strip;
use super::{Ide, tree};
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::fields;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{Type, show_type};

/// The item of an `Option` type: the record an optional step reads.
pub(super) fn option_item(ty: &Type) -> Option<Type> {
    let (head, args) = strip(ty).spine();
    match args[..] {
        [item] if head.is_lang(Lang::Option) => Some(item.clone()),
        _ => None,
    }
}

impl Ide<'_> {
    /// The type of the field `label` of a value of type `record`.
    pub(super) fn field_type(&self, file: FileId, record: &Type, label: &str) -> Option<Type> {
        let mut fields = Vec::new();
        self.record_fields(file, record, &mut fields, 0);
        fields.into_iter().find(|(l, _)| l == label).map(|(_, t)| t)
    }

    /// The type of the value a path's node gives, as far as the checker
    /// keeps it: a name, a projection or one in parentheses.
    pub(super) fn value_type(&self, m: &Analyzed, node: &Node) -> Option<Type> {
        match node.kind {
            K::ParenExpr => {
                let inner = node.nodes().next()?;
                let ty = self.value_type(m, inner)?;
                // parentheses end a path, which an optional step makes an `Option`
                let (_, steps) = fields::steps(inner);
                if inner.kind == K::FieldExpr && steps.iter().any(|s| fields::is_optional(s)) {
                    Some(Type::app(Type::lang(Lang::Option), ty))
                } else {
                    Some(ty)
                }
            }
            K::FieldExpr => {
                let label = node.child_tokens().find(|t| t.kind == K::Ident)?;
                let record = m.typing.fields.get(&(label.start, label.end))?;
                self.field_type(m.file, record, label.text(self.text(m.file)))
            }
            _ => m.typing.types.get(&node.range()).cloned(),
        }
    }

    /// The record each step of `accessor` reads a field of, in order: the
    /// accessor's parameter, then the field the step before it reads.
    pub(super) fn step_receivers(&self, m: &Analyzed, accessor: &Node) -> Vec<Option<Type>> {
        (self.step_reads(m, accessor).into_iter())
            .map(|(_, record)| record)
            .collect()
    }

    /// For each step of `accessor`, the value before it and the record it
    /// reads the field of: an optional step reads inside that value's `Option`.
    pub(super) fn step_reads(
        &self,
        m: &Analyzed,
        accessor: &Node,
    ) -> Vec<(Option<Type>, Option<Type>)> {
        let text = self.text(m.file);
        let mut receiver = match m.typing.types.get(&accessor.range()).map(strip) {
            Some(Type::Fun { param, .. }) => Some((**param).clone()),
            _ => None,
        };
        let mut out = Vec::new();
        for step in accessor.nodes().filter(|n| n.kind == K::AccessorStep) {
            let label = step.child_tokens().find(|t| t.kind == K::Ident);
            let before = receiver.clone();
            if fields::is_optional(step) {
                receiver = receiver.as_ref().and_then(option_item);
            }
            if let Some(label) = label
                && let Some(record) = m.typing.fields.get(&(label.start, label.end))
            {
                receiver = Some(record.clone());
            }
            out.push((before, receiver.clone()));
            receiver = match (receiver, label) {
                (Some(record), Some(label)) => self.field_type(m.file, &record, label.text(text)),
                _ => None,
            };
        }
        out
    }

    /// The record whose fields may follow the `.` at `offset`: of a step of
    /// an accessor or of a projection, its label written or not yet.
    pub(super) fn receiver_at_dot(&self, m: &Analyzed, offset: u32) -> Option<Type> {
        let dot_at = |n: &Node| {
            n.child_tokens()
                .any(|t| t.kind == K::Dot && t.start == offset)
        };
        for node in tree::path_to(&m.tree, offset).into_iter().rev() {
            match node.kind {
                K::FieldExpr if dot_at(node) => {
                    let value = self.value_type(m, node.nodes().next()?);
                    return if fields::is_optional(node) {
                        value.as_ref().and_then(option_item)
                    } else {
                        value
                    };
                }
                K::AccessorExpr => {
                    let at = node
                        .nodes()
                        .filter(|n| n.kind == K::AccessorStep)
                        .position(dot_at)?;
                    return self.step_receivers(m, node).swap_remove(at);
                }
                _ => {}
            }
        }
        None
    }

    /// Hover on an accessor outside its labels: its type as a function and
    /// what it gives.
    pub(super) fn accessor_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let accessor = path.iter().rev().find(|n| n.kind == K::AccessorExpr)?;
        let ty = m.typing.types.get(&accessor.range())?;
        let Type::Fun { result, .. } = strip(ty) else {
            return None;
        };
        let (start, end) = accessor.range();
        let written = &self.text(file)[start as usize..end as usize];
        Some((
            format!(
                "```flick\n{written} : {}\n```\n\nA function reading `{written}` of its argument: a `{}`.",
                m.typing.shown(accessor.range(), ty),
                show_type(result)
            ),
            accessor.range(),
        ))
    }

    /// Hover on the `.?` of an optional step: the `Option` it takes off, and
    /// what a `None` there does.
    pub(super) fn optional_step_hover(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let step = *path.last()?;
        if !matches!(step.kind, K::FieldExpr | K::AccessorStep) {
            return None;
        }
        let dot = step.child_tokens().find(|t| t.kind == K::Dot)?;
        let mark = step.child_tokens().find(|t| t.kind == K::Question)?;
        if !(dot.start <= offset && offset <= mark.end) {
            return None;
        }
        let label = step.child_tokens().find(|t| t.kind == K::Ident);
        let label_text = label.map_or("", |l| l.text(self.text(file)));
        let record =
            label.and_then(|l| Some(((l.start, l.end), m.typing.fields.get(&(l.start, l.end))?)));
        // the record by the alias it is declared as, as a field's hover names it
        let layer = match record {
            Some((range, r)) => match self.owner_at(m, range, r, label_text) {
                Ok((_, alias)) => format!("Option {alias}"),
                Err(_) => show_type(&Type::app(Type::lang(Lang::Option), r.clone())),
            },
            None => "Option _".to_owned(),
        };
        Some((
            format!(
                "```flick\n.?{label_text} : {layer}\n```\n\nTakes one `Option` off: on `Some r` it reads `r.{label_text}`; \
                 on `None` the whole path is `None`, and no step after it is read. The path is the `Option` \
                 of its last field. Unlike `x?.{label_text}`, it never returns from the function."
            ),
            (dot.start, mark.end),
        ))
    }
}
