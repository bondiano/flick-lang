//! The mode of a `foreign`: `pure` or `effect` as written, else a call of the
//! host that its signature tells, awaited as a promise when its row has
//! `Async`.

use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::{Node, foreign_words};
use crate::types::Checker;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Effects, Type};
use flick_ir::ForeignMode;

impl Checker<'_> {
    pub(super) fn foreign_mode(&mut self, decl: &Node, body: &Type) -> ForeignMode {
        let (word, name) = foreign_words(decl);
        let effects = runtime_effects(body);
        let has_async = effects.is_some_and(|e| {
            let label = lang::label(Lang::Async);
            matches!(&e.services, Type::Row(r) if r.entries.iter().any(|(l, _)| l.name() == label))
        });
        match word.map(|w| w.text(self.text)) {
            Some("pure") => return ForeignMode::Pure,
            Some("effect") => return ForeignMode::Effect,
            Some(old @ ("sync" | "async")) => {
                let (word, name) = (word.expect("the word"), name.expect("the name"));
                // the word goes, and a promise's row gets the `Async` it says
                let mut edits = vec![Edit {
                    span: Span::new(self.file, word.start, name.start),
                    replacement: String::new(),
                }];
                if old == "async" && !has_async {
                    let adding = effects.and_then(|e| e.written.adding("Async"));
                    edits.extend(adding.map(|(span, replacement)| Edit { span, replacement }));
                }
                let message = format!(
                    "`foreign {old}` is now `foreign`: its signature tells, `Async` in its row for a promise"
                );
                let d = Diagnostic::error(codes::F001, message, self.span(decl));
                self.diagnostics
                    .push(d.with_safe_fix(|| (format!("remove `{old}`"), edits)));
            }
            _ => {}
        }
        match effects {
            Some(_) if has_async || word.is_some_and(|w| w.text(self.text) == "async") => {
                ForeignMode::Async
            }
            Some(_) => ForeignMode::Sync,
            None => {
                let message = "a `foreign` without a mode calls the host in the runtime, so its signature needs `->{…}`: write `foreign pure` for a function of its arguments alone";
                let d = Diagnostic::error(codes::F001, message, self.span(decl));
                let edits = name
                    .map(|n| Edit {
                        span: Span::new(self.file, n.start, n.start),
                        replacement: "pure ".to_owned(),
                    })
                    .into_iter()
                    .collect();
                self.diagnostics
                    .push(d.with_fix(|| ("write `foreign pure`", edits)));
                ForeignMode::Pure
            }
        }
    }
}

/// The rows of the first runtime arrow of `ty`, the call of the host.
fn runtime_effects(mut ty: &Type) -> Option<&Effects> {
    loop {
        match ty {
            Type::Fun {
                effects: Some(e), ..
            } => return Some(e),
            Type::Fun { result, .. } => ty = result,
            Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } => ty = inner,
            _ => return None,
        }
    }
}
