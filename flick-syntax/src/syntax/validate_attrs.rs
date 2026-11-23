//! Validation for core attributes (`@lang`, `@derive`, `@maker`, `@cell`,
//! `@label`, `@owner`, `@sync`, `@stores`, `@implicit` and `@when`).
//! Backend-owned type-form attributes (such as `@ts`) and unknown attribute
//! checks run in [`super::foreign_meta::validate_tree`] once the active
//! backend descriptor is known.

use super::cst::{Node, Token};
use super::heads::{CHECKED_BY, INVARIANT_OF};
use super::kind::SyntaxKind as K;
use super::when;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};
use crate::syntax::lang::Lang;

/// `@lang item`, with an item the compiler knows, `@derive` before a
/// class, maybe naming the package whose plugin derives it, or `@maker`
/// before a value that makes an atom, an action or a view; `@cell`
/// marks a type whose values hold one value each, `@label kind` one
/// whose top-level values carry their names where the program runs,
/// `@owner` a service whose handler owns what is made under it, `@sync`
/// one whose operations run at once, `@ts` gives a type its TypeScript
/// form, `@stores` marks a foreign type whose values store functions
/// (data with hidden storage), `@implicit` a conversion instance, and
/// `@when` a module condition.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(super) fn check(
    src: &str,
    file: FileId,
    allow_trailing_attribute: bool,
    node: &Node,
    before: Option<&Node>,
    before_module_header: bool,
    duplicate_when: bool,
    errors: &mut Vec<Diagnostic>,
) {
    let next = before.map(|n| n.kind);
    let words: Vec<Token> = node.child_tokens().filter(|t| t.kind == K::Ident).collect();
    let message = match &words[..] {
        [name, ..] if name.text(src) == "when" => {
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
        [name] | [name, _] if name.text(src) == "derive" => return,
        [name] if name.text(src) == "maker" => return,
        [name, ..] if name.text(src) == "maker" => "`@maker` takes no argument".to_owned(),
        [name] if name.text(src) == "cell" => {
            if matches!(next, Some(K::TypeDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@cell` belongs before a type declaration".to_owned()
        }
        [name, ..] if name.text(src) == "cell" => "`@cell` takes no argument".to_owned(),
        [name, _] if name.text(src) == "label" => {
            if matches!(next, Some(K::TypeDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@label` belongs before a type declaration".to_owned()
        }
        [name, ..] if name.text(src) == "label" => {
            "`@label` names one kind, as `@label atom`".to_owned()
        }
        [name] | [name, _] if name.text(src) == "sync" => {
            let reader = words.get(1).is_none_or(|w| w.text(src) == "reader");
            if !reader {
                "`@sync` takes `reader` or nothing".to_owned()
            } else if matches!(next, Some(K::ServiceDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            } else {
                "`@sync` belongs before a service declaration".to_owned()
            }
        }
        [name, ..] if name.text(src) == "sync" => "`@sync` takes `reader` or nothing".to_owned(),
        // generated code alone writes it: a source cannot spell `$`
        [name, _] if CHECKED_BY.ends_with(name.text(src)) && name.text(src).contains('$') => {
            if matches!(next, Some(K::LetDecl)) {
                return;
            }
            "a check is named before the function it reads a constant for".to_owned()
        }
        // generated code alone writes it, before the type it is the
        // invariant of
        [name, _, _, _]
            if INVARIANT_OF.ends_with(name.text(src)) && name.text(src).contains('$') =>
        {
            if matches!(next, Some(K::TypeDecl)) {
                return;
            }
            "an invariant is named before the type whose values it holds for".to_owned()
        }
        [name] if name.text(src) == "owner" => {
            if matches!(next, Some(K::ServiceDecl | K::Attribute))
                || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@owner` belongs before a service declaration".to_owned()
        }
        [name, ..] if name.text(src) == "owner" => "`@owner` takes no argument".to_owned(),
        [name] if name.text(src) == "stores" => {
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
        [name, ..] if name.text(src) == "stores" => "`@stores` takes no argument".to_owned(),
        [name] if name.text(src) == "implicit" => {
            // a lone attribute is a fragment the formatter checks on its own
            if matches!(next, Some(K::InstanceDecl)) || (next.is_none() && allow_trailing_attribute)
            {
                return;
            }
            "`@implicit` belongs before an instance declaration".to_owned()
        }
        [name, ..] if name.text(src) == "implicit" => "`@implicit` takes no argument".to_owned(),
        [name, ..] if name.text(src) == "derive" => {
            "`@derive` names at most the package of its plugin: `@derive std`".to_owned()
        }
        [name, item] if name.text(src) == "lang" && Lang::from_name(item.text(src)).is_some() => {
            return;
        }
        [name, item] if name.text(src) == "lang" => {
            format!("unknown lang item `{}`", item.text(src))
        }
        [name, ..] if name.text(src) == "lang" => "`@lang` names one lang item".to_owned(),
        // Backend-owned type-form attributes (such as `@ts`) and unknown
        // attribute diagnostics are validated by `foreign_meta::validate_tree`.
        _ => return,
    };
    let (start, end) = node.range();
    errors.push(Diagnostic::error(
        codes::S014,
        message,
        Span::new(file, start, end),
    ));
}
