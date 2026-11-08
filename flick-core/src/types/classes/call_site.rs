//! `CallSite`, the place of a use, which the checker gives: the caller's
//! own constraint passes on, else the place of the use it is wanted at.
//! Each use has its own, so it is never cached.

use super::solve::Givens;
use super::{Dict, split};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Checker;
use crate::types::env::Instance;
use crate::types::lang::{self, Lang};
use crate::types::ty::Type;

impl Checker<'_> {
    /// The dictionary of `c` at `span` when `c` is `CallSite`.
    pub(super) fn call_site(&self, c: &Type, givens: &Givens, span: Span) -> Option<Dict> {
        let ((module, name), args) = split(c)?;
        if !args.is_empty() || !lang::is(Lang::CallSite, module, name) {
            return None;
        }
        if let Some((_, given)) = givens.iter().rev().find(|(g, _)| g == c) {
            return Some(given.clone());
        }
        let here = self.env.module.clone();
        let line = self.text[..span.start as usize].matches('\n').count() + 1;
        let definition = match self.top.definition_at(span.start, self.generated) {
            Some(name) => format!("{here}.{name}"),
            None => here.clone(),
        };
        Some(Dict::Site {
            place: format!("{here}:{line}"),
            definition,
        })
    }

    /// Whether `instance` is one of `CallSite`, which only the checker
    /// gives; it is reported.
    pub(super) fn written_call_site(&mut self, instance: &Instance, span: Span) -> bool {
        let Some(((module, name), _)) = split(&instance.head) else {
            return false;
        };
        if !lang::is(Lang::CallSite, module, name) {
            return false;
        }
        let message = "`CallSite` is the compiler's: each use gets the place of its caller's, or its own, and a program declares no instance".to_owned();
        self.error(codes::C012, span, message);
        true
    }
}
