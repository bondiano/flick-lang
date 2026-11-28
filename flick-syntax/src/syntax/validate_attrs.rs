//! Validation for core compiler attributes (`@lang`, `@derive`, `@maker`,
//! `@cell`, `@label`, `@owner`, `@sync`, `@stores`, `@implicit`, `@when` and
//! `@export`).
//! Backend-declared type-form attributes (such as `@ts` or `@go`) and unknown
//! attribute errors are validated by the checker against the active backend's
//! descriptor.

use super::attr::attr_parts;
use super::cst::Node;
use super::export_attr;
use super::heads::{CHECKED_BY, INVARIANT_OF};
use super::kind::SyntaxKind as K;
use super::when;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};
use crate::syntax::lang::Lang;

/// Placement context of an attribute being validated.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Placement {
    TopLevel {
        before_module_header: bool,
        duplicate_when: bool,
    },
    Nested,
    Variant,
    RecordField,
}

/// `@lang item`, with an item the compiler knows, `@derive` before a
/// class, maybe naming the package whose plugin derives it, or `@maker`
/// before a value that makes an atom, an action or a view; `@cell`
/// marks a type whose values hold one value each, `@label kind` one
/// whose top-level values carry their names where the program runs,
/// `@owner` a service whose handler owns what is made under it, `@sync`
/// one whose operations run at once, `@ts` gives a type its TypeScript
/// form, `@stores` marks a foreign type whose values store functions
/// (data with hidden storage), `@implicit` a conversion instance,
/// `@when` a module condition, and `@export "name"` a host-named
/// top-level function export.
#[allow(clippy::too_many_lines)]
pub(super) fn check(
    src: &str,
    file: FileId,
    allow_trailing_attribute: bool,
    placement: Placement,
    node: &Node,
    before: Option<&Node>,
    errors: &mut Vec<Diagnostic>,
) {
    let (top_level, before_module_header, duplicate_when) = match placement {
        Placement::TopLevel {
            before_module_header,
            duplicate_when,
        } => (true, before_module_header, duplicate_when),
        Placement::Nested | Placement::Variant | Placement::RecordField => (false, false, false),
    };
    let Some(parts) = attr_parts(node, src) else {
        return;
    };
    if parts.namespaced {
        return;
    }
    let next = before.map(|n| n.kind);
    let args: Vec<(K, &str)> = node
        .child_tokens()
        .filter(|t| t.start >= parts.name_end)
        .map(|t| (t.kind, t.text(src)))
        .collect();
    let message = match (parts.name, args.as_slice()) {
        ("export", _) => match export_attr::read_raw(node, src) {
            Some(Err(why)) => why.to_owned(),
            _ if !top_level => "`@export` belongs before a top-level function".to_owned(),
            _ if matches!(next, Some(K::Signature | K::LetDecl))
                || (next.is_none() && allow_trailing_attribute) =>
            {
                return;
            }
            _ => "`@export` belongs before a top-level function".to_owned(),
        },
        ("when", _) => {
            if duplicate_when && before_module_header {
                "a module may have at most one `@when` attribute: combine conditions with `&&`"
                    .to_owned()
            } else {
                match when::read(node, src) {
                    Some(Err(why)) => why,
                    _ if before_module_header || (next.is_none() && allow_trailing_attribute) => {
                        return;
                    }
                    _ => "`@when` belongs before the module header".to_owned(),
                }
            }
        }
        ("native", _) => {
            "`@native` belongs indented after the body of a top-level `let` definition".to_owned()
        }
        ("derive", [] | [(K::Ident, _)]) => {
            if matches!(placement, Placement::RecordField | Placement::Variant) {
                "`@derive` belongs before a class declaration".to_owned()
            } else {
                return;
            }
        }
        ("derive", _) => {
            "`@derive` names at most the package of its plugin: `@derive std`".to_owned()
        }
        ("maker", []) => {
            if matches!(placement, Placement::RecordField | Placement::Variant) {
                "`@maker` belongs before a declaration".to_owned()
            } else {
                return;
            }
        }
        ("maker", _) => "`@maker` takes no argument".to_owned(),
        ("cell", []) => {
            if matches!(next, Some(K::TypeDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@cell` belongs before a type declaration".to_owned()
        }
        ("cell", _) => "`@cell` takes no argument".to_owned(),
        ("label", [(K::Ident, _)]) => {
            if matches!(next, Some(K::TypeDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@label` belongs before a type declaration".to_owned()
        }
        ("label", _) => "`@label` names one kind, as `@label atom`".to_owned(),
        ("sync", [] | [(K::Ident, "reader")]) => {
            if matches!(next, Some(K::ServiceDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@sync` belongs before a service declaration".to_owned()
        }
        ("sync", _) => "`@sync` takes `reader` or nothing".to_owned(),
        // generated code alone writes it: a source cannot spell `$`
        (name, [(K::Ident, _)]) if CHECKED_BY.ends_with(name) && name.contains('$') => {
            if matches!(next, Some(K::LetDecl)) {
                return;
            }
            "a check is named before the function it reads a constant for".to_owned()
        }
        // generated code alone writes it, before the type it is the
        // invariant of
        (name, [(K::Ident, _), (K::Ident, _), (K::Ident, _)])
            if INVARIANT_OF.ends_with(name) && name.contains('$') =>
        {
            if matches!(next, Some(K::TypeDecl)) {
                return;
            }
            "an invariant is named before the type whose values it holds for".to_owned()
        }
        ("owner", []) => {
            if matches!(next, Some(K::ServiceDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@owner` belongs before a service declaration".to_owned()
        }
        ("owner", _) => "`@owner` takes no argument".to_owned(),
        ("stores", []) => {
            // constructors or a body show what a type stores itself
            if before.is_some_and(|d| d.kind == K::TypeDecl && d.has_token(K::Eq)) {
                "`@stores` belongs before a type without constructors or a body".to_owned()
            } else if matches!(next, Some(K::TypeDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            } else {
                "`@stores` belongs before a type declaration".to_owned()
            }
        }
        ("stores", _) => "`@stores` takes no argument".to_owned(),
        ("implicit", []) => {
            // a lone attribute is a fragment the formatter checks on its own
            if matches!(next, Some(K::InstanceDecl)) || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@implicit` belongs before an instance declaration".to_owned()
        }
        ("implicit", _) => "`@implicit` takes no argument".to_owned(),
        ("lang", [(K::Ident, _)]) if placement == Placement::RecordField => {
            "`@lang` belongs before a declaration or variant".to_owned()
        }
        ("lang", [(K::Ident, item)]) if Lang::from_name(item).is_some() => return,
        ("lang", [(K::Ident, item)]) => format!("unknown lang item `{item}`"),
        ("lang", _) => "`@lang` names one lang item".to_owned(),
        _ => return,
    };
    let (start, end) = node.range();
    errors.push(Diagnostic::error(
        codes::S014,
        message,
        Span::new(file, start, end),
    ));
}
