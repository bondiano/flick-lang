use std::ptr;

use super::{Ide, as_written, tree};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Kind;

const ROW_KIND_DESCRIPTION: &str = "The kind of rows whose entries have kind `k`. `Row Type` is the kind of record field rows (also written `FieldRow`); it is separate from service, error and capture rows.";
const ARROW_KIND_DESCRIPTION: &str = "A kind arrow describes a type constructor: it takes an argument of the kind on the left and produces a result of the kind on the right. `List` has kind `Type -> Type`.";

/// Adding a `Kind` variant must also give it a hover description.
pub(super) fn kind_description(kind: &Kind) -> &'static str {
    match kind {
        Kind::Type => {
            "The kind of ordinary value types, such as `String` and `List String`. A type constructor can take type arguments: `List` has kind `Type -> Type`."
        }
        Kind::ServiceRow => {
            "A row of service identities, used for the requirements of computations such as `Eff r e a` and for the inputs and outputs of layers. `none` is the empty row."
        }
        Kind::ErrorRow => {
            "A row of errors a computation may fail with. Its entries come from `type error` declarations; `none` means no declared errors."
        }
        Kind::CaptureRow => {
            "A row of regions a value may capture, written in a qualifier such as `T^c`. Captures are checked statically and are separate from effect rows."
        }
        Kind::Symbol => {
            "The kind of type-level string labels such as `\"name\"`, used to identify fields and other named entries."
        }
        Kind::Region => {
            "The kind of a region identity, used to tie a resource to its lifetime, as in `File s` where `s : Region`."
        }
        Kind::Constraint => {
            "The kind of type-class constraints, such as `Eq a`, which can appear before `=>` in a signature."
        }
        Kind::Service => {
            "The kind of a service identity. Service rows contain these identities; a service such as `Clock` contributes one to a row."
        }
        Kind::Row(element) if **element == Kind::Type => {
            "The kind of record field rows, equivalent to `Row Type`. An open record can have a field-row tail such as `{ name : String | rest }`."
        }
        Kind::Row(_) => ROW_KIND_DESCRIPTION,
        Kind::Named { .. } => {
            "A finite kind defined by the listed members. Use one of them as a type-level argument."
        }
        Kind::Arrow(_, _) => ARROW_KIND_DESCRIPTION,
        Kind::Meta(_) => {
            "A kind still being inferred from how its type variable is used. If nothing determines it, the kind becomes `Type`."
        }
    }
}

impl Ide<'_> {
    /// Kinds in `(parameter : Kind)` annotations. Kind syntax uses
    /// type nodes, so keep this separate from ordinary type-name hover.
    pub(super) fn kind_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let param = path.iter().find(|n| n.kind == K::TypeParam)?;
        let annotation = param.nodes().next()?;
        if !(annotation.range().0 <= offset && offset < annotation.range().1) {
            return None;
        }
        if let Some((arrow, token)) = path.iter().rev().find_map(|node| {
            if node.kind != K::ArrowType {
                return None;
            }
            node.child_tokens()
                .find(|t| t.kind == K::Arrow && t.start <= offset && offset < t.end)
                .map(|token| (*node, token))
        }) {
            return Some((
                format!(
                    "```flick\n{}\n```\n\n{ARROW_KIND_DESCRIPTION}",
                    as_written(arrow, self.text(file)),
                ),
                (token.start, token.end),
            ));
        }
        let name = path.iter().rev().find(|n| n.kind == K::NameType)?;
        let token = name
            .child_tokens()
            .find(|t| t.kind == K::Con && t.start <= offset && offset < t.end)?;
        if name.child_tokens().filter(|t| t.kind == K::Con).count() != 1 {
            return None;
        }
        let at = (token.start, token.end);
        let word = token.text(self.text(file));
        let (code, note) = if word == "Row"
            && path.iter().any(|n| {
                n.kind == K::AppType && n.nodes().next().is_some_and(|head| ptr::eq(head, *name))
            }) {
            ("Row k".to_owned(), ROW_KIND_DESCRIPTION)
        } else {
            let kind = Kind::builtin(word)?;
            let note = kind_description(&kind);
            (word.to_owned(), note)
        };
        Some((format!("```flick\n{code}\n```\n\n{note}"), at))
    }
}
