//! "Add a signature" for a `let` of a value, whose type is inferred.

use super::super::{Ide, tree};
use super::{Action, edit, indent_at, line_start};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Kind, Type, show_type, without_value_captures};

impl Ide<'_> {
    /// For the header of a `let name = …` with no signature above it; a
    /// function without one gets this from its `FLICK-T004` fix.
    pub(super) fn add_signature(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let i = path.iter().rposition(|n| n.kind == K::LetDecl)?;
        let (decl, parent) = (path[i], *path.get(i.checked_sub(1)?)?);
        let eq = decl.child_tokens().find(|t| t.kind == K::Eq)?;
        if offset > eq.start || decl.nodes().any(|n| n.kind == K::Params) {
            return None;
        }
        let pattern = decl.nodes().find(|n| n.kind == K::VarPat)?;
        let name = pattern.child_tokens().find(|t| t.kind == K::Ident)?;
        let siblings: Vec<_> = parent.nodes().collect();
        let at = siblings.iter().position(|n| n.range() == decl.range())?;
        let signed = at > 0
            && siblings[at - 1].kind == K::Signature
            && tree::name_of(siblings[at - 1], K::Ident, text) == Some(name.text(text));
        if signed {
            return None;
        }
        let ty = m.typing.types.get(&(name.start, name.end))?;
        // a capture row cannot name a type variable; the written signature
        // implies those projections through the parameters
        let types: Vec<&str> = match ty {
            Type::Forall { vars, .. } => vars
                .iter()
                .filter(|v| matches!(v.kind, Kind::Type))
                .map(|v| v.name.as_str())
                .collect(),
            _ => Vec::new(),
        };
        let shown = show_type(&without_value_captures(ty, &types));
        // a type the checker did not finish has holes; it is no signature
        if shown
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == "_")
        {
            return None;
        }
        let name = name.text(text);
        let start = line_start(text, decl.range().0);
        let indent = indent_at(text, decl.range().0);
        Some(Action {
            title: format!("add the signature `{name} : {shown}`"),
            kind: "refactor.rewrite",
            edits: vec![edit(
                file,
                start,
                start,
                format!("{indent}{name} : {shown}\n"),
            )],
        })
    }
}
