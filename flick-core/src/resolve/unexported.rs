//! Errors for names an import asks for that its module does not export.

use super::syntax::{Import, Item};
use super::{Decl, Interface, Namespace};
use crate::diagnostics::suggest::closest;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;

pub(super) fn not_exported(interface: &Interface, name: &str, span: Span) -> Diagnostic {
    let module = &interface.module;
    if let Some(hidden) = interface.hidden.iter().find(|h| h.name == name) {
        return Diagnostic::error(
            codes::R005,
            format!(
                "constructor `{name}` is not exported: `{module}` exports `{}` without `(..)`",
                hidden.parent
            ),
            span,
        )
        .with_fix(
            format!(
                "export it from `{module}` as `{}(..)`, or build values through its functions",
                hidden.parent
            ),
            Vec::new(),
        );
    }
    let exports = interface.exports.iter().map(|e| e.entity.name.as_str());
    let guess = closest(name, exports).map(str::to_owned);
    Diagnostic::error(
        codes::R004,
        format!("`{module}` does not export `{name}`"),
        span,
    )
    .did_you_mean(span, guess)
}

pub(super) fn no_members(import: &Import, interface: &Interface, item: &Item) -> Diagnostic {
    let name = &item.name;
    let module = &import.module;
    let message = if interface
        .find(Namespace::Type, name)
        .is_some_and(|e| e.decl == Decl::Opaque)
    {
        format!("`{name}` is opaque: `{module}` does not export its constructors")
    } else if interface.hidden.iter().any(|h| &h.parent == name) {
        format!("`{module}` exports `{name}` without its constructors (no `(..)`)")
    } else {
        format!("`{name}` from `{module}` has no constructors to import")
    };
    Diagnostic::error(codes::R005, message, item.span)
        .with_fix(format!("import the type alone: `{name}`"), Vec::new())
}
