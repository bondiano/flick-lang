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
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Type, show_type};

impl Ide<'_> {
    /// The type of the field `label` of a value of type `record`.
    pub(super) fn field_type(&self, file: FileId, record: &Type, label: &str) -> Option<Type> {
        let mut fields = Vec::new();
        self.record_fields(file, record, &mut fields, 0);
        fields.into_iter().find(|(l, _)| l == label).map(|(_, t)| t)
    }

    /// The type of the value a path's node gives, as far as the checker
    /// keeps it: a name, a projection or one in parentheses.
    fn value_type(&self, m: &Analyzed, node: &Node) -> Option<Type> {
        match node.kind {
            K::ParenExpr => self.value_type(m, node.nodes().next()?),
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
        let text = self.text(m.file);
        let mut receiver = match m.typing.types.get(&accessor.range()).map(strip) {
            Some(Type::Fun { param, .. }) => Some((**param).clone()),
            _ => None,
        };
        let mut out = Vec::new();
        for step in accessor.nodes().filter(|n| n.kind == K::AccessorStep) {
            let label = step.child_tokens().find(|t| t.kind == K::Ident);
            if let Some(label) = label
                && let Some(record) = m.typing.fields.get(&(label.start, label.end))
            {
                receiver = Some(record.clone());
            }
            out.push(receiver.clone());
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
                K::FieldExpr if dot_at(node) => return self.value_type(m, node.nodes().next()?),
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
}
