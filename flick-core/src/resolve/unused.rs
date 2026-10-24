//! Imports nothing uses, and local names nothing refers to.

use std::collections::BTreeSet;

use super::scope::Scope;
use super::syntax::{Import, Item, ItemKind};
use super::uses::{Reference, Unused};
use super::{Interface, Module};
use crate::diagnostics::{Code, Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::kind::SyntaxKind as K;

/// Warnings for local names bound and never used.
pub(super) fn unused_locals(unused: Vec<Unused>) -> impl Iterator<Item = Diagnostic> {
    unused.into_iter().map(|u| {
        let start = Span::new(u.span.file, u.span.start, u.span.start);
        let (message, edit) = if u.punned {
            (
                format!("bind it as `{} = _{}`", u.name, u.name),
                Edit {
                    span: u.span,
                    replacement: format!("{} = _{}", u.name, u.name),
                },
            )
        } else {
            (
                format!("rename it `_{}` if it is unused on purpose", u.name),
                Edit {
                    span: start,
                    replacement: "_".to_owned(),
                },
            )
        };
        warning(codes::R013, format!("`{}` is never used", u.name), u.span)
            .with_fix(message, vec![edit])
    })
}

impl Scope<'_> {
    /// Warnings for imports, and names they expose, that no reference and no
    /// export of the module uses.
    pub fn unused_imports(&self, references: &[Reference], diagnostics: &mut Vec<Diagnostic>) {
        let module = self.module();
        let mut names: BTreeSet<&str> = references
            .iter()
            .filter(|r| r.qualifier.is_none())
            .map(|r| r.name.as_str())
            .collect();
        // a re-export uses the name too
        let header = module.syntax.header.as_ref();
        names.extend(
            header
                .and_then(|h| h.exposing.as_ref())
                .into_iter()
                .flatten()
                .map(|i| i.name.as_str()),
        );
        let qualifiers: BTreeSet<&str> = references
            .iter()
            .filter_map(|r| r.qualifier.as_deref())
            .collect();
        for (import, interface) in self.imports() {
            let Some(interface) = interface else {
                continue;
            };
            let unused: Vec<&Item> = import
                .exposing
                .iter()
                .filter(|item| !used(item, interface, &names))
                .collect();
            if !qualifiers.contains(import.qualifier()) && unused.len() == import.exposing.len() {
                // importing a module brings its instances, which may be the point
                if !interface
                    .instances
                    .iter()
                    .any(|i| i.module == import.module)
                {
                    diagnostics.push(unused_import(module, import));
                }
                continue;
            }
            for item in unused {
                diagnostics.push(unused_item(module.text, import, item));
            }
        }
    }
}

fn used(item: &Item, interface: &Interface, names: &BTreeSet<&str>) -> bool {
    match item.kind {
        ItemKind::All => interface
            .exports
            .iter()
            .any(|e| names.contains(e.entity.name.as_str())),
        ItemKind::ConAll => {
            names.contains(item.name.as_str())
                || interface
                    .members(&item.name)
                    .any(|m| names.contains(m.entity.name.as_str()))
        }
        ItemKind::Value | ItemKind::Con => names.contains(item.name.as_str()),
    }
}

fn warning(code: Code, message: String, span: Span) -> Diagnostic {
    Diagnostic::new(code, Severity::Warning, message).with_span(span)
}

fn unused_import(module: &Module, import: &Import) -> Diagnostic {
    let d = warning(
        codes::R012,
        format!("`{}` is imported but never used", import.module),
        import.span,
    );
    let decl = module.tree.nodes().find(|n| {
        n.kind == K::ImportDecl
            && n.range().0 <= import.span.start
            && import.span.end <= n.range().1
    });
    match decl {
        Some(decl) => {
            let (start, end) = decl.range();
            let end = end + u32::from(module.text[end as usize..].starts_with('\n'));
            d.with_fix(
                "remove the import",
                vec![Edit {
                    span: Span::new(import.span.file, start, end),
                    replacement: String::new(),
                }],
            )
        }
        None => d,
    }
}

fn unused_item(text: &str, import: &Import, item: &Item) -> Diagnostic {
    let (message, what) = if item.kind == ItemKind::All {
        (
            format!("nothing `(..)` brings from `{}` is used", import.module),
            "`(..)`".to_owned(),
        )
    } else {
        (
            format!("`{}` is imported but never used", item.name),
            format!("`{}`", item.name),
        )
    };
    let (start, end) = item_removal(text, import, item.span);
    warning(codes::R012, message, item.span).with_fix(
        format!("remove {what} from the import"),
        vec![Edit {
            span: Span::new(item.span.file, start, end),
            replacement: String::new(),
        }],
    )
}

/// The text to delete with an `exposing` item: its comma, or the whole
/// `exposing (…)` when it is the only item.
fn item_removal(text: &str, import: &Import, item: Span) -> (u32, u32) {
    let after = &text[item.end as usize..];
    let gap = after.len() - after.trim_start().len();
    if after.trim_start().starts_with(',') {
        let rest = &after[gap + 1..];
        let space = rest.len() - rest.trim_start().len();
        return (item.start, item.end + to_u32(gap + 1 + space));
    }
    let before = &text[..item.start as usize];
    let trimmed = before.trim_end();
    if trimmed.ends_with(',') {
        return (to_u32(trimmed.len() - 1), item.end);
    }
    let head = &text[import.span.end as usize..item.start as usize];
    let close = after
        .find(')')
        .map_or(item.end, |i| item.end + to_u32(i + 1));
    match head.rfind("exposing") {
        Some(i) => {
            let start = text[..import.span.end as usize + i].trim_end().len();
            (to_u32(start), close)
        }
        None => (item.start, item.end),
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}

#[cfg(test)]
#[path = "unused_tests.rs"]
mod tests;
