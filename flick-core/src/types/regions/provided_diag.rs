//! Diagnostics and signature fix edits for `Fiber.Provided` (`FLICK-G003`).

use super::Scope;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::Elem;
use crate::types::convert::is_type;
use crate::types::ty::Type;

impl Checker<'_> {
    pub(super) fn outlived_provide(
        &mut self,
        service: &Elem,
        provide: usize,
        owner: &Type,
        span: Span,
        owner_related: Option<Span>,
    ) {
        let Scope { span: at, .. } = self.cx.regions.scopes[provide];
        let (region, _) = self.describe(owner);
        let service = service.to_string();
        let name = service.rsplit('.').next().unwrap_or(&service);
        let mut diagnostic = Diagnostic::error(
            codes::G003,
            format!(
                "this keeps work that uses `{name}` until {region} closes, but the `provide` that builds `{name}` releases it earlier"
            ),
            span,
        )
        .with_related(at, "the service is released when this `provide` ends");
        if let Some(owner_span) = owner_related {
            diagnostic =
                diagnostic.with_related(owner_span, "cleanup is registered on this `scope`");
        }
        self.diagnostics.push(diagnostic.with_fix(|| {
            (
                "give the work an owner inside the `provide`, or provide the service outside the owner's `scope`",
                Vec::new(),
            )
        }));
    }

    /// `FLICK-G003` when a `Provided` obligation reaches a `Layer.run` body row
    /// that excludes it through a higher-order wrapper or partial application.
    pub fn outlived_layer_run(&mut self, service: &Type, owner: &Type, span: Span) {
        let (region, _) = self.describe(owner);
        let service = self.show(service);
        let service = service.trim_matches(['{', '}', ' ']);
        let name = service.rsplit('.').next().unwrap_or(service);
        let diagnostic = Diagnostic::error(
            codes::G003,
            format!(
                "this keeps work that uses `{name}` until {region} closes, but the `provide` that builds `{name}` releases it earlier"
            ),
            span,
        )
        .with_fix(|| {
            (
                "give the work an owner inside the `provide`, or provide the service outside the owner's `scope`",
                Vec::new(),
            )
        });
        self.diagnostics.push(diagnostic);
    }

    /// Whether `ty` is a variable of a top-level signature being checked,
    /// which a constraint added to it can name; the region of a `forall`
    /// in an argument's type, as that of `Layer.from_build`, is not.
    pub fn signed(&self, ty: &Type) -> bool {
        matches!(self.store.resolve(ty), Type::Skolem(s) if self.cx.regions.signed.contains(&s))
    }

    /// An obligation that reached a signature, or a row variable none
    /// settles. Only when the signature `named` the region and the row is
    /// there a constraint to add to it.
    pub fn unprovided(&mut self, service: &str, region: &str, named: bool, span: Span) {
        // a row variable is the row itself; a service is an element of one
        let row = if service.starts_with(char::is_lowercase) {
            service.to_owned()
        } else {
            format!("{{{service}}}")
        };
        let diagnostic = Diagnostic::error(
            codes::G003,
            format!(
                "this keeps work that uses `{service}` until `{region}` closes, but the service comes from the caller, which may release it earlier"
            ),
            span,
        );
        let diagnostic = if named {
            let constraint = format!("Fiber.Provided {row} {region}");
            let edits = self.constraint_edits(&constraint, span);
            diagnostic.with_fix(|| (format!("add `{constraint}` to the signature"), edits))
        } else {
            diagnostic
        };
        self.diagnostics.push(diagnostic);
    }

    /// The edits that add `constraint` to the signature of the top-level
    /// function around `span`; none when it has no signature to edit.
    fn constraint_edits(&self, constraint: &str, span: Span) -> Vec<Edit> {
        let Some(mut ty) = self
            .signature_around(span)
            .and_then(|sig| sig.nodes().find(|n| is_type(n.kind)))
        else {
            return Vec::new();
        };
        if ty.kind == K::ForallType {
            let Some(body) = ty.nodes().next() else {
                return Vec::new();
            };
            ty = body;
        }
        let insert = |at: u32, replacement: String| Edit {
            span: Span::new(self.file, at, at),
            replacement,
        };
        match (ty.kind == K::ConstrainedType)
            .then(|| ty.nodes().next())
            .flatten()
        {
            Some(context) => {
                let (start, end) = context.range();
                if context.kind == K::TupleType {
                    vec![insert(end - 1, format!(", {constraint}"))]
                } else {
                    vec![
                        insert(start, "(".to_owned()),
                        insert(end, format!(", {constraint})")),
                    ]
                }
            }
            None => vec![insert(ty.range().0, format!("{constraint} => "))],
        }
    }
}
