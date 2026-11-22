//! Diagnostics and fixes reported after row constraints are solved.

use super::tails::less;
use super::{Excess, SubRow};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::fmt::foralls;
use crate::source::Span;
use crate::types::effects::Site;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Norm, Type};
use crate::types::{Checker, Elem};

impl Checker<'_> {
    /// E019: `tail` may hold `label` at other arguments than the instance of
    /// it `declared` holds, which would answer for it by its name.
    pub(super) fn instance_answered(
        &mut self,
        label: &Elem,
        declared: &Norm,
        tail: &Type,
        span: Span,
    ) {
        let held = declared
            .get(label)
            .map(|t| self.show(t))
            .unwrap_or_default();
        let variable = self.show(less(tail).0);
        let name = label.short();
        let message = format!(
            "`{variable}` may hold `{name}` at other arguments, and `{held}` here would answer for it: \
             take it out, `{{-{name} | {variable}}}`, or name the instance, `{{{held} | {variable}}}`"
        );
        self.diagnostics
            .push(Diagnostic::error(codes::E019, message, span));
    }

    /// A resource acquired where no `scope` owns it. The fix is not safe: the
    /// row says the caller provides the owner, which fixes a public boundary.
    pub(super) fn no_owner(&mut self, scope: &Type, c: &SubRow) {
        let region = match scope.spine().1[..] {
            [region] if !matches!(self.store.resolve(region), Type::Meta(_)) => self.show(region),
            // a region not known yet gets a name the signature does not use
            _ => {
                let taken = self
                    .signature_around(c.span)
                    .map(|sig| foralls::type_names(sig, self.text))
                    .unwrap_or_default();
                let fresh = (0..).map(|i| {
                    if i == 0 {
                        "s".to_owned()
                    } else {
                        format!("s{i}")
                    }
                });
                fresh
                    .into_iter()
                    .find(|name| !taken.contains(name))
                    .expect("names never run out")
            }
        };
        let added = format!("Scope {region}");
        let bound = self.local_place_at(&c.declared, c.span);
        let written = match (bound.0, &self.sites[c.site]) {
            (Some(_), _) => Some(bound),
            (None, Site::Signature(_, written) | Site::Expected(_, written)) => {
                Some(self.editable_place(*written, c.span))
            }
            (None, Site::Inferred | Site::DeferCleanup { .. }) => None,
        };
        let edits: Vec<Edit> = written
            .and_then(|w| w.adding(&added))
            .map(|(span, replacement)| Edit { span, replacement })
            .into_iter()
            .collect();
        let advice = if edits.is_empty() {
            format!(
                "call it inside `scope {{ … }}`, or add `{added}` to the services of the signature"
            )
        } else {
            format!("add `{added}` to the services of the signature")
        };
        self.diagnostics.push(
            Diagnostic::error(
                codes::G004,
                "this acquires a resource, which needs `Scope`: only `scope { … }` provides an owner",
                c.span,
            )
            .with_fix(|| (advice, edits)),
        );
    }

    pub(super) fn report_excess(&mut self, e: &Excess) {
        let named = |what: &str, names: &[String]| {
            let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
            match quoted.len() {
                0 => None,
                1 => Some(format!("{what} {}", quoted[0])),
                _ => Some(format!("{what}s {}", quoted.join(", "))),
            }
        };
        let list = [named("service", &e.services), named("error", &e.errors)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ");
        if let Site::Expected(ty, _) = &self.sites[e.site] {
            let ty = ty.clone();
            self.lacking_under(&ty, e.span, &list, (&e.services, &e.labels, &e.errors));
        }
        if let Site::DeferCleanup {
            is_errdefer,
            span: cleanup_span,
        } = self.sites[e.site]
        {
            self.report_defer_cleanup_excess(is_errdefer, cleanup_span, e.span, &list);
            return;
        }
        let (bound, needer, written) = match &self.sites[e.site] {
            Site::Signature(name, written) => (
                format!("the signature of `{name}`"),
                "its body",
                Some(self.editable_place(*written, e.span)),
            ),
            Site::Expected(ty, written) => {
                let editable = self.editable_place(*written, e.span);
                (
                    format!("the expected type `{}`", self.show(ty)),
                    "this",
                    editable.0.is_some().then_some(editable),
                )
            }
            Site::Inferred => ("the type of this function".to_owned(), "its body", None),
            Site::DeferCleanup { .. } => unreachable!("handled above"),
        };
        let mut diagnostic = Diagnostic::error(
            codes::E002,
            format!("{bound} does not allow {list}, which {needer} needs"),
            e.span,
        );
        let bound_written = self.editable_place(e.bound, e.span);
        let written = if bound_written.0.is_some() {
            Some(bound_written)
        } else {
            written
        };
        if let Some(written) = written.filter(|_| !e.taken) {
            let to_add: Vec<String> = if e.labels.len() == e.services.len() {
                e.labels
                    .iter()
                    .zip(&e.services)
                    .map(|(label, short)| match label.name() {
                        Some(name) => match name.split_once('.') {
                            Some((module, _))
                                if module != "Prelude" && module != self.env.module =>
                            {
                                name.to_owned()
                            }
                            _ => short.clone(),
                        },
                        None => short.clone(),
                    })
                    .collect()
            } else {
                e.services.clone()
            };
            // the services go after the `{`; the errors, after the `;`, are left to the user
            let edits: Vec<Edit> = (e.errors.is_empty())
                .then(|| written.adding(&to_add.join(", ")))
                .flatten()
                .map(|(span, replacement)| Edit { span, replacement })
                .into_iter()
                .collect();
            // a function that suspends or needs runner host services (Async or Host.*):
            // adding them to the row changes nothing else callers cannot provide under a runner
            let is_runner_service = e.errors.is_empty()
                && !edits.is_empty()
                // a row variable it lacks has no label
                && e.labels.len() == e.services.len()
                && e.labels.iter().all(|l| {
                    lang::label(Lang::Async) == l.name() || l.name().is_some_and(lang::is_host_service)
                });
            let fix = || (format!("add {list} to {bound}"), edits);
            diagnostic = if is_runner_service {
                diagnostic.with_safe_fix(fix)
            } else {
                diagnostic.with_fix(fix)
            };
        }
        self.diagnostics.push(diagnostic);
    }

    fn report_defer_cleanup_excess(
        &mut self,
        is_errdefer: bool,
        cleanup_span: Span,
        span: Span,
        list: &str,
    ) {
        let kw = if is_errdefer { "errdefer" } else { "defer" };
        let diagnostic = Diagnostic::error(
            codes::E002,
            format!(
                "a `{kw}` cleanup block cannot leave {list} unhandled: handle typed errors locally inside the cleanup block"
            ),
            span,
        )
        .with_related(
            cleanup_span,
            format!("this `{kw}` cleanup block requires an empty error row"),
        )
        .with_fix(|| {
            (
                "handle the error locally with `try … with` inside the cleanup block".to_owned(),
                Vec::new(),
            )
        });
        self.diagnostics.push(diagnostic);
    }
}
