//! FLICK-T059: what a signature writes and would get anyway.

use super::Checker;
use super::is_type;
use super::kinds::Kind;
use super::show_type;
use super::ty::{Binder, Type};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::keeps_for_owner;

impl Checker<'_> {
    /// Warns of each constraint in `node`, the signature of type `scheme` of the
    /// foreign declaration `decl`, that its `retention owner` gives anyway.
    pub(super) fn redundant_owner_constraints(&mut self, decl: &Node, node: &Node, scheme: &Type) {
        let Type::Forall {
            vars,
            constraints,
            body,
        } = scheme
        else {
            return;
        };
        let Some(constrained) = constrained_type(node) else {
            return;
        };
        if !keeps_for_owner(decl, self.text) {
            return;
        }
        let (Some(context), Some(signed)) = (constrained.nodes().next(), last_type(constrained))
        else {
            return;
        };
        let items = constraint_items(context);
        if items.len() != constraints.len() {
            return;
        }
        let redundant = |i: usize| {
            let mut rest = constraints.clone();
            let one = rest.remove(i);
            let without = Type::Forall {
                vars: vars.clone(),
                constraints: rest,
                body: body.clone(),
            };
            matches!(
                self.with_owner_constraints(decl, without),
                Type::Forall { constraints, .. } if constraints.contains(&one)
            )
        };
        let (gone, kept): (Vec<usize>, Vec<usize>) = (0..items.len()).partition(|&i| redundant(i));
        if gone.is_empty() {
            return;
        }
        let names = |of: &[usize]| {
            of.iter()
                .map(|&i| self.text_of(items[i]))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let replacement = match kept[..] {
            [] => String::new(),
            [one] => format!("{} => ", self.text_of(items[one])),
            _ => format!("({}) => ", names(&kept)),
        };
        let warning = Diagnostic::new(
            codes::T059,
            Severity::Warning,
            format!(
                "`retention owner` gives this signature `{}` anyway",
                names(&gone)
            ),
        )
        .with_span(self.span(context));
        let edits = vec![Edit {
            span: Span::new(self.file, constrained.range().0, signed.range().0),
            replacement,
        }];
        self.diagnostics
            .push(self.removing(warning, "remove what is given", edits));
    }

    /// Warns of the signature of the instance method `member` that says what
    /// its class says: `scheme` is `expected` but for the names of its binders.
    /// One whose binders the method's body names stays: they are in scope there.
    pub(super) fn redundant_method_signature(
        &mut self,
        signature: &Node,
        member: &Node,
        scheme: &Type,
        expected: &Type,
    ) {
        if signature.range().0 >= self.generated {
            return;
        }
        let own: Vec<&str> = match scheme {
            Type::Forall { vars, .. } => vars.iter().map(|v| v.name.as_str()).collect(),
            _ => Vec::new(),
        };
        if names_a_type(member, self.text, &own)
            || self.show(&canonical(scheme)) != self.show(&canonical(expected))
        {
            return;
        }
        let warning = Diagnostic::new(
            codes::T059,
            Severity::Warning,
            "this method has the type its class gives it, so the signature says nothing".to_owned(),
        )
        .with_span(self.span(signature));
        let edits = vec![Edit {
            span: Span::new(self.file, signature.range().0, member.range().0),
            replacement: String::new(),
        }];
        self.diagnostics
            .push(self.removing(warning, "remove the signature", edits));
    }

    /// `warning` with the safe fix `edits`, unless they would take a comment away.
    fn removing(&self, warning: Diagnostic, help: &str, edits: Vec<Edit>) -> Diagnostic {
        let comment = (edits.iter()).any(|e| self.text[e.span.range()].contains("//"));
        if comment {
            warning
        } else {
            warning.with_safe_fix(|| (help, edits))
        }
    }
}

/// `t` with each binder named by where it first appears and its constraints in
/// order, so two signatures that differ in those alone read the same.
fn canonical(t: &Type) -> Type {
    match t {
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut seen: Vec<String> = Vec::new();
            body.visit(&mut |t| {
                if let Type::Var(name) = t
                    && !seen.contains(name)
                    && vars.iter().any(|v| v.name == *name)
                {
                    seen.push(name.clone());
                }
            });
            let unused = (vars.iter()).filter(|v| !seen.contains(&v.name));
            seen.extend(unused.map(|v| v.name.clone()).collect::<Vec<_>>());
            let subst: Vec<(String, Type)> = (seen.iter().enumerate())
                .map(|(i, name)| (name.clone(), Type::Var(format!("v{i}"))))
                .collect();
            let binders = (subst.iter()).map(|(name, new)| {
                let kind = (vars.iter())
                    .find(|v| v.name == *name)
                    .map(|v| v.kind.clone());
                let Type::Var(new) = new else {
                    unreachable!("a variable")
                };
                Binder {
                    name: new.clone(),
                    kind: kind.unwrap_or(Kind::Type),
                }
            });
            let mut written: Vec<Type> = (constraints.iter())
                .map(|c| canonical(&c.substitute(&subst)))
                .collect();
            written.sort_by_key(show_type);
            Type::Forall {
                vars: binders.collect(),
                constraints: written,
                body: Box::new(canonical(&body.substitute(&subst))),
            }
        }
        other => other.map_children(&mut canonical),
    }
}

/// Whether a type written under `node` names one of `names`.
fn names_a_type(node: &Node, text: &str, names: &[&str]) -> bool {
    if is_type(node.kind) {
        return (node.tokens().into_iter())
            .any(|t| t.kind == K::Ident && names.contains(&t.text(text)));
    }
    node.nodes().any(|child| names_a_type(child, text, names))
}

/// The last type under `node`: the body of a `forall` or of a context, or what
/// parentheses hold.
fn last_type(node: &Node) -> Option<&Node> {
    node.nodes().filter(|n| is_type(n.kind)).last()
}

/// The `Context => body` of a signature, under its `forall` and parentheses.
fn constrained_type(node: &Node) -> Option<&Node> {
    match node.kind {
        K::ConstrainedType => Some(node),
        K::ForallType | K::ParenType => constrained_type(last_type(node)?),
        _ => None,
    }
}

/// The constraints a context writes, as `Checker::constraints` reads them.
fn constraint_items(node: &Node) -> Vec<&Node> {
    match node.kind {
        K::TupleType | K::ParenType => node.nodes().flat_map(constraint_items).collect(),
        _ => vec![node],
    }
}

#[cfg(all(test, feature = "diagnostic-fixes"))]
#[path = "signature_lints_tests.rs"]
mod tests;
