use std::ptr;

use super::{Ide, as_written, tree};
use crate::iface::Namespace;
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Kind, Type, show_type};

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
    /// The checked kind of a declaration parameter or a signature variable,
    /// both where it is bound and where it is used in that declaration.
    fn variable_kind_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let (node, token) = path.iter().rev().find_map(|node| {
            if !matches!(
                node.kind,
                K::TypeParam | K::ForallType | K::NameType | K::Fundep
            ) {
                return None;
            }
            node.child_tokens()
                .find(|t| t.kind == K::Ident && t.start <= offset && offset < t.end)
                .map(|t| (*node, t))
        })?;
        // An annotation's kind syntax is handled by `kind_hover` itself.
        if path.iter().any(|n| n.kind == K::TypeParam) && node.kind == K::NameType {
            return None;
        }
        let name = token.text(self.text(file));
        let signature = path.iter().rev().find(|n| n.kind == K::Signature);
        let (kind, constraints) = signature
            .and_then(|sig| self.signature_variable_info(m, &path, sig, name))
            .or_else(|| Self::instance_parameter_info(m, &path, name))
            .or_else(|| self.declaration_parameter_info(m, &path, name))?;
        let prefix = match constraints.as_slice() {
            [] => String::new(),
            [constraint] => format!("{constraint} => "),
            _ => format!("({}) => ", constraints.join(", ")),
        };
        let code = format!("{prefix}{name} : {kind}");
        let explanation = (kind != Kind::Type).then(|| kind_description(&kind));
        let markdown = match explanation {
            Some(description) => format!("```flick\n{code}\n```\n\n{description}"),
            None => format!("```flick\n{code}\n```"),
        };
        Some((markdown, (token.start, token.end)))
    }

    fn signature_variable_info(
        &self,
        m: &Analyzed,
        path: &[&Node],
        signature: &Node,
        name: &str,
    ) -> Option<(Kind, Vec<String>)> {
        let member = tree::name_of(signature, K::Ident, self.text(m.file))?;
        if let Some(ty) = signature.nodes().next()
            && let Some(scheme) = m.typing.signature_types.get(&ty.range())
            && let Some(info) = forall_info(scheme, name)
        {
            return Some(info);
        }
        let parent = path
            .iter()
            .rev()
            .find(|n| matches!(n.kind, K::ClassDecl | K::ServiceDecl));
        let scheme = match parent {
            Some(decl) if decl.kind == K::ServiceDecl => {
                let owner = tree::name_of(decl, K::Con, self.text(m.file))?;
                let definition = m
                    .typing
                    .declarations
                    .find(Namespace::Type, owner)?
                    .definition
                    .as_ref()?;
                let Form::Service { operations, .. } = &definition.form else {
                    return None;
                };
                &operations.iter().find(|op| op.name == member)?.ty
            }
            _ => m
                .typing
                .declarations
                .find(Namespace::Value, member)?
                .scheme
                .as_ref()?,
        };
        forall_info(scheme, name)
    }

    fn instance_parameter_info(
        m: &Analyzed,
        path: &[&Node],
        name: &str,
    ) -> Option<(Kind, Vec<String>)> {
        let decl = path.iter().rev().find(|n| n.kind == K::InstanceDecl)?;
        let info = m.typing.instances.iter().find(|i| i.span == decl.range())?;
        let instance = m
            .typing
            .declarations
            .instances
            .iter()
            .find(|i| i.module == m.name && i.name == info.name)?;
        let var = instance.vars.iter().find(|v| v.name == name)?;
        Some((var.kind.clone(), constraints_for(&instance.context, name)))
    }

    fn declaration_parameter_info(
        &self,
        m: &Analyzed,
        path: &[&Node],
        name: &str,
    ) -> Option<(Kind, Vec<String>)> {
        let decl = path.iter().rev().find(|n| {
            matches!(
                n.kind,
                K::TypeDecl | K::ErrorDecl | K::ClassDecl | K::ServiceDecl
            )
        })?;
        let owner = tree::name_of(decl, K::Con, self.text(m.file))?;
        let definition = m
            .typing
            .declarations
            .find(Namespace::Type, owner)?
            .definition
            .as_ref()?;
        let mut kind = &definition.kind;
        for param in decl.nodes().filter(|n| n.kind == K::TypeParam) {
            let Kind::Arrow(arg, result) = kind else {
                return None;
            };
            if tree::name_of(param, K::Ident, self.text(m.file)) == Some(name) {
                let constraints = match &definition.form {
                    Form::Class { supers, .. } => constraints_for(supers, name),
                    _ => Vec::new(),
                };
                return Some(((**arg).clone(), constraints));
            }
            kind = result;
        }
        None
    }

    /// Kinds in `(parameter : Kind)` annotations. Kind syntax uses
    /// type nodes, so keep this separate from ordinary type-name hover.
    pub(super) fn kind_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        if let Some(hover) = self.variable_kind_hover(file, offset) {
            return Some(hover);
        }
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

fn forall_info(ty: &Type, name: &str) -> Option<(Kind, Vec<String>)> {
    let mut found = None;
    ty.visit(&mut |ty| {
        if found.is_none()
            && let Type::Forall {
                vars, constraints, ..
            } = ty
            && let Some(var) = vars.iter().find(|v| v.name == name)
        {
            found = Some((var.kind.clone(), constraints_for(constraints, name)));
        }
    });
    found
}

fn constraints_for(constraints: &[Type], name: &str) -> Vec<String> {
    constraints
        .iter()
        .filter(|constraint| constraint.contains(&mut |ty| matches!(ty, Type::Var(v) if v == name)))
        .map(show_type)
        .collect()
}
