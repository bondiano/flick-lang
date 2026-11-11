//! Diagnostics for types that do not unify.

use super::Checker;
use super::ty::Type;
use super::unify::Mismatch;
mod async_place;
mod coercions;

use crate::diagnostics::{Code, Diagnostic, Edit, codes};
use crate::source::Span;
use crate::types::lang::{self, Lang};

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
        self.diagnostic(d);
    }

    /// Unifies the type an expression has with the one its position expects;
    /// reports a mismatch at `span`. `false` after a diagnostic.
    pub fn expect(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        let ground = self.coercion_types(actual, expected);
        let marks = (self.regions.holds.len(), self.subrows.len());
        match self.subsume(actual, expected, span) {
            Ok(()) => {
                if let Some(lift) = self.lift(actual, expected) {
                    self.typing.lifts.insert(super::typing::range(span), lift);
                }
                true
            }
            Err(mismatch) => {
                if let Some((actual, expected)) = ground
                    && self.insert_coercion(&actual, &expected, span)
                {
                    // ground nominal types bind no metas; what the failed
                    // comparison queued is about a value no longer placed here
                    self.regions.holds.truncate(marks.0);
                    self.subrows.truncate(marks.1);
                    return true;
                }
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
        let plain = |t: &Type| uncaptured(&self.zonk(t));
        let polymorphic = |t: &Type| matches!(plain(t), Type::Forall { .. });
        let short = |t: &Type| self.show(&plain(t));
        // the difference may be what the short form leaves out
        if captures || polymorphic(actual) || polymorphic(expected) {
            let exact = |t: &Type| {
                if captures {
                    self.show_exact(t)
                } else {
                    self.show_exact(&plain(t))
                }
            };
            return (exact(actual), exact(expected));
        }
        let (a, e) = (short(actual), short(expected));
        if a == e {
            let (a, e) = (
                self.show_exact(&plain(actual)),
                self.show_exact(&plain(expected)),
            );
            if a == e {
                // Distinct nominal types from different modules, including
                // virtual parameterized module results, can share the same short name.
                return (
                    self.show_qualified(&plain(actual)),
                    self.show_qualified(&plain(expected)),
                );
            }
            return (a, e);
        }
        (a, e)
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
                // here the difference may be the `forall` itself
                let show = |t: &Type| match t {
                    Type::Forall { .. } => self.show_exact(t),
                    _ => self.show(t),
                };
                let (sa, sb) = (show(a), show(b));
                if sa == sb && self.show_qualified(a) != self.show_qualified(b) {
                    format!(
                        "`{}` and `{}` are distinct nominal types; use one module result or convert the values explicitly",
                        self.show_qualified(a),
                        self.show_qualified(b)
                    )
                } else if sa == actual && sb == expected || sa == expected && sb == actual {
                    String::new()
                } else {
                    format!("`{sa}` is not `{sb}`")
                }
            }
        }
    }

    pub fn mismatch(&mut self, mismatch: &Mismatch, actual: &Type, expected: &Type, span: Span) {
        let reported = self.diagnostics.len();
        self.report_mismatch(mismatch, actual, expected, span);
        // a type that lacks only `Async` says so where it is written
        let about_async = match mismatch {
            Mismatch::Label { label, .. } => lang::label(Lang::Async) == Some(label.as_str()),
            Mismatch::Types(..) => true,
            _ => false,
        };
        if about_async
            && self.diagnostics.len() == reported + 1
            && let Some((at, text)) = (self.async_place(actual, expected))
                .or_else(|| self.async_place(expected, actual))
                .and_then(|w| w.adding("Async"))
            && let Some(d) = self.diagnostics.pop()
        {
            let edits = vec![Edit {
                span: at,
                replacement: text,
            }];
            self.diagnostics
                .push(d.with_safe_fix(|| ("add `Async` to the type", edits)));
        }
    }

    fn report_mismatch(&mut self, mismatch: &Mismatch, actual: &Type, expected: &Type, span: Span) {
        let through_live = matches!(mismatch, Mismatch::Escape(s) if self.needs_live(actual, *s));
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
            Mismatch::Escape(skolem) => self.report_escape(*skolem, head, through_live, span),
            Mismatch::Untouchable(t) => {
                let t = self.show(t);
                self.error(
                    codes::T057,
                    span,
                    format!(
                        "{head}: a type from outside this branch would become `{t}` by what the branch learned about the matched value; annotate it"
                    ),
                );
            }
            Mismatch::Polymorphic(t) => {
                let t = self.show_exact(t);
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
    /// A rigid variable that would leave its `forall`, or a region its
    /// `scope`, as one a row needs live (`through_live`).
    fn report_escape(&mut self, skolem: u32, head: String, through_live: bool, span: Span) {
        let head = if through_live {
            let name = self.store.skolem_name(skolem);
            format!("this value needs region `{name}` live")
        } else {
            head
        };
        if !self.region_escape(skolem, &head, span) {
            let name = self.store.skolem_name(skolem).to_owned();
            let scope = if self.gadt.existentials.contains(&skolem) {
                "the branch that matched the constructor hiding it"
            } else {
                "the scope of its `forall`"
            };
            self.error(
                codes::T007,
                span,
                format!("{head}: the type variable `{name}` would escape {scope}"),
            );
        }
    }

    /// E011: a `Server.Guarded` where a `Server.Function` goes, or the other
    /// way round; `false` when the mismatch is something else.
    fn served_wrong(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        // whether it is the guarded kind
        let server = |t: &Type| match t.spine().0 {
            t if t.is_lang(Lang::ServerGuarded) => Some(true),
            t if t.is_lang(Lang::ServerFunction) => Some(false),
            _ => None,
        };
        let (Some(actual), Some(expected)) = (server(actual), server(expected)) else {
            return false;
        };
        // `server name`, as `name$server`, names the server fn; another
        // expression does not
        let text = &self.text[span.start as usize..span.end as usize];
        let name = (text.trim_matches(|c| c == '(' || c == ')'))
            .strip_suffix("$server")
            .filter(|n| !n.contains(char::is_whitespace));
        let named = name.map_or_else(
            || "this server fn".to_owned(),
            |n| format!("server fn `{n}`"),
        );
        let d = match (actual, expected) {
            (true, false) => Diagnostic::error(
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
            (false, true) => {
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
    /// `server value`, whose implementation `value$server` no `server fn`
    /// declares.
    pub(super) fn not_a_server_fn(&mut self, value: &str, span: Span) {
        self.error(
            codes::E010,
            span,
            format!(
                "`{value}` is not a server fn: only `server fn` has an implementation to serve"
            ),
        );
    }

    fn unauthorized_edit(&self, name: &str) -> Option<Edit> {
        // the signature of a server fn types its implementation
        let sig = (self.signature_type_span(&format!("{name}$impl")))
            .or_else(|| self.signature_span(name))?;
        let text = &self.text[sig.start as usize..sig.end as usize];
        let open = text.rfind("->{")?;
        let close = open + text[open..].find('}')?;
        let Some(semi) = text[open..close].find(';').map(|at| open + at) else {
            let at = sig.start + u32::try_from(close).ok()?;
            return Some(Edit {
                span: Span::new(sig.file, at, at),
                replacement: "; Server.Unauthorized".to_owned(),
            });
        };
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
