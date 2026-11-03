//! Diagnostics for types that do not unify.

use super::Checker;
use super::ty::Type;
use super::unify::Mismatch;
use crate::diagnostics::{Code, Diagnostic, Edit, codes};
use crate::source::Span;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// An error whose `detail`, when there is one, is a help line under it.
    fn error_help(
        &mut self,
        code: Code,
        span: Span,
        head: &str,
        incomplete: bool,
        detail: impl FnOnce(&Self) -> String,
    ) {
        let d = Diagnostic::error(code, head, span).with_incomplete(incomplete);
        #[cfg(feature = "diagnostic-fixes")]
        let d = {
            let detail = detail(self);
            if detail.is_empty() {
                d
            } else {
                d.with_fix(|| (detail, Vec::new()))
            }
        };
        #[cfg(not(feature = "diagnostic-fixes"))]
        let _ = detail;
        self.diagnostics.push(d);
    }

    /// Unifies the type an expression has with the one its position expects;
    /// reports a mismatch at `span`. `false` after a diagnostic.
    pub fn expect(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        match self.subsume(actual, expected, span) {
            Ok(()) => {
                if let Some(lift) = self.lift(actual, expected) {
                    self.typing.lifts.insert(super::typing::range(span), lift);
                }
                true
            }
            Err(mismatch) => {
                self.mismatch(&mismatch, actual, expected, span);
                false
            }
        }
    }

    /// Qualifiers that must be equal, as in mutable storage, differ.
    fn capture_mismatch(&mut self, head: &str, label: &str, left: bool, span: Span) {
        let meta = label.strip_prefix("^?").and_then(|n| n.parse().ok());
        let incomplete = meta.is_some_and(|m| {
            self.zonk(&Type::Meta(m))
                .contains(&mut |t| matches!(t, Type::Error))
        });
        self.error_help(codes::T003, span, head, incomplete, |this| {
            // a meta label names what the meta has become since
            let element = match meta {
                Some(meta) => this.store.resolve(&Type::Meta(meta)),
                None => match label.strip_prefix("^'").and_then(|n| n.parse().ok()) {
                    Some(skolem) => Type::Skolem(skolem),
                    None => Type::Var(label.trim_start_matches('^').to_owned()),
                },
            };
            let region = match element {
                Type::Skolem(skolem) => this.store.skolem_name(skolem).to_owned(),
                other => this.show(&other),
            };
            let detail = if left {
                "the value holds it, but the expected type does not allow it"
            } else {
                "the expected type holds it, and mutable storage keeps its captures exactly"
            };
            format!("region `{region}` differs: {detail}")
        });
    }

    /// The two types of a mismatch as shown: a type mismatch names the
    /// types, only a capture mismatch is about qualifiers.
    fn mismatch_types(
        &self,
        mismatch: &Mismatch,
        actual: &Type,
        expected: &Type,
    ) -> (String, String) {
        let captures = matches!(mismatch, Mismatch::Label { label, .. } if label.starts_with('^'));
        let shown = |t: &Type| {
            if captures {
                self.show(t)
            } else {
                self.show(&uncaptured(&self.zonk(t)))
            }
        };
        (shown(actual), shown(expected))
    }

    fn mismatch_hint(&self, a: &Type, b: &Type, actual: &str, expected: &str) -> String {
        let errors = |ty: &Type| ty.spine().0.is_lang(Lang::Errors);
        match (a, b) {
            (Type::Fun { effects: e1, .. }, Type::Fun { effects: e2, .. })
                if e1.is_some() != e2.is_some() =>
            {
                "a pure function `->` and a runtime function `->{…}` are different types".to_owned()
            }
            (x, y) if errors(x) || errors(y) => {
                let other = self.show(if errors(x) { y } else { x });
                format!(
                    "`{other}` is not an error; an error row holds types declared with `type error`, so convert the value into one first"
                )
            }
            _ => {
                let (sa, sb) = (self.show(a), self.show(b));
                if sa == actual && sb == expected || sa == expected && sb == actual {
                    String::new()
                } else {
                    format!("`{sa}` is not `{sb}`")
                }
            }
        }
    }

    pub fn mismatch(&mut self, mismatch: &Mismatch, actual: &Type, expected: &Type, span: Span) {
        let (actual, expected) = self.mismatch_types(mismatch, actual, expected);
        let head = format!("expected `{expected}`, found `{actual}`");
        match mismatch {
            Mismatch::Types(a, b) => {
                let (a, b) = (uncaptured(&self.zonk(a)), uncaptured(&self.zonk(b)));
                let rigid = |t: &Type| matches!(t, Type::Skolem(_));
                if rigid(&a) || rigid(&b) {
                    let (var, other) = if rigid(&b) { (&b, &a) } else { (&a, &b) };
                    let (var, other) = (self.show(var), self.show(other));
                    self.error(
                        codes::T008,
                        span,
                        format!(
                            "{head}: `{var}` stands for any type the caller chooses, so it cannot be `{other}`"
                        ),
                    );
                    return;
                }
                if self.served_wrong(&a, &b, span) {
                    return;
                }
                let incomplete = a.contains(&mut |t| matches!(t, Type::Error))
                    || b.contains(&mut |t| matches!(t, Type::Error));
                self.error_help(codes::T003, span, &head, incomplete, |this| {
                    this.mismatch_hint(&a, &b, &actual, &expected)
                });
            }
            Mismatch::Label { label, left } if label.starts_with('^') => {
                self.capture_mismatch(&head, label, *left, span);
            }
            Mismatch::Label { label, left } => {
                self.error_help(codes::T003, span, &head, false, |this| {
                    // in a nested record literal, the record that lacks or has the field
                    let path = this.field_path.join(".");
                    match (label.rsplit_once('.'), *left) {
                        (Some((_, name)), true) => format!(
                            "`{name}` is in the row, which the expected type does not allow"
                        ),
                        (Some((_, name)), false) => format!("`{name}` is missing from the row"),
                        (None, true) if !path.is_empty() => format!(
                            "`{path}` has field `{label}`, which the expected type does not"
                        ),
                        (None, true) => format!(
                            "the value has field `{label}`, which the expected type does not"
                        ),
                        (None, false) if !path.is_empty() => {
                            format!("field `{label}` is missing from `{path}`")
                        }
                        (None, false) => format!("field `{label}` is missing"),
                    }
                });
            }
            Mismatch::Occurs(t) => {
                let t = self.show(t);
                self.error(
                    codes::T016,
                    span,
                    format!("{head}: the type would contain itself (`{t}`)"),
                );
            }
            Mismatch::Escape(skolem) => {
                if !self.region_escape(*skolem, &head, span) {
                    let name = self.store.skolem_name(*skolem).to_owned();
                    self.error(
                        codes::T007,
                        span,
                        format!(
                            "{head}: the type variable `{name}` would escape the scope of its `forall`"
                        ),
                    );
                }
            }
            Mismatch::Polymorphic(t) => {
                let t = self.show(t);
                self.error(
                    codes::T003,
                    span,
                    format!(
                        "{head}: a type variable cannot stand for the polymorphic type `{t}`; write the `forall` in a signature"
                    ),
                );
            }
        }
    }
}

impl Checker<'_> {
    /// E011: a `Server.Guarded` where a `Server.Function` goes, or the other
    /// way round; `false` when the mismatch is something else.
    fn served_wrong(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        let server = |t: &Type| match t.spine().0 {
            Type::Con { module, name } if module == "Server" => Some(name.clone()),
            _ => None,
        };
        let (Some(actual), Some(expected)) = (server(actual), server(expected)) else {
            return false;
        };
        // `server name` names the server fn; another expression does not
        let text = &self.text[span.start as usize..span.end as usize];
        let name = text
            .strip_prefix("server")
            .map(str::trim)
            .filter(|n| !n.contains(char::is_whitespace));
        let named = name.map_or_else(
            || "this server fn".to_owned(),
            |n| format!("server fn `{n}`"),
        );
        let d = match (actual.as_str(), expected.as_str()) {
            ("Guarded", "Function") => Diagnostic::error(
                codes::E011,
                format!(
                    "{named} lists `Server.Unauthorized`, so only `Server.authenticated` serves it"
                ),
                span,
            )
            .with_fix(|| {
                (
                    "serve it through `Server.authenticated Service check [...]`",
                    Vec::new(),
                )
            }),
            ("Function", "Guarded") => {
                let d = Diagnostic::error(
                    codes::E011,
                    format!(
                        "{named} does not list `Server.Unauthorized`, so `Server.authenticated` cannot guard it"
                    ),
                    span,
                );
                d.with_fix(|| {
                    let edit = name.and_then(|n| self.unauthorized_edit(n));
                    (
                        "list `Server.Unauthorized` among its errors",
                        edit.into_iter().collect(),
                    )
                })
            }
            _ => return false,
        };
        self.diagnostics.push(d);
        true
    }

    /// The edit that adds `Server.Unauthorized` to the errors of the last
    /// arrow of the signature of this module's `name`.
    fn unauthorized_edit(&self, name: &str) -> Option<Edit> {
        let sig = self.signature_span(name)?;
        let text = &self.text[sig.start as usize..sig.end as usize];
        let open = text.rfind("->{")?;
        let close = open + text[open..].find('}')?;
        let semi = open + text[open..close].find(';')?;
        let listed = !text[semi + 1..close].trim().is_empty();
        let at = sig.start + u32::try_from(close).ok()?;
        let at = if listed {
            at
        } else {
            sig.start + u32::try_from(semi).ok()? + 1
        };
        Some(Edit {
            span: Span {
                file: sig.file,
                start: at,
                end: at,
            },
            replacement: if listed {
                ", Server.Unauthorized"
            } else {
                " Server.Unauthorized"
            }
            .to_owned(),
        })
    }
}

/// `ty` without capture qualifiers at any depth.
fn uncaptured(ty: &Type) -> Type {
    match ty {
        Type::Captured { ty, .. } | Type::Bounded { ty, .. } => uncaptured(ty),
        other => other.map_children(&mut uncaptured),
    }
}
