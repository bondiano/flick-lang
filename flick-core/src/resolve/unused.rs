//! Imports nothing uses, and local names nothing refers to.

use std::collections::BTreeSet;

use super::scope::Scope;
use super::syntax::{Import, Item, ItemKind};
use super::uses::{Reference, Unused};
use super::{Interface, Module};
use crate::diagnostics::{Code, Diagnostic, Edit, Severity, codes};
use crate::plugin::first_party;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::implicit_package_of;

/// Warnings for local names bound and never used.
pub(super) fn unused_locals(unused: Vec<Unused>) -> impl Iterator<Item = Diagnostic> {
    unused.into_iter().map(|u| {
        warning(codes::R013, format!("`{}` is never used", u.name), u.span).with_fix(|| {
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
            (message, vec![edit])
        })
    })
}

fn packed_paths(node: &Node, text: &str, out: &mut Vec<String>) {
    if node.kind == K::PackExpr
        && let Some(path) = node.nodes().find(|n| n.kind == K::ModulePath)
    {
        out.push(path.text(text).split_whitespace().collect());
    }
    for child in node.nodes() {
        packed_paths(child, text, out);
    }
}

fn module_expression_paths(node: &Node, text: &str, out: &mut Vec<String>) {
    if node.kind == K::ModulePath {
        out.push(node.text(text).split_whitespace().collect());
    }
    for child in node.nodes() {
        module_expression_paths(child, text, out);
    }
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
        // `M.S.op` uses the import `M` too: count each prefix of a qualifier
        let mut packed = Vec::new();
        packed_paths(module.tree, module.text, &mut packed);
        if let Some(header) = &module.syntax.header {
            if let Some(body) = &header.body {
                module_expression_paths(body, module.text, &mut packed);
            }
            if let Some(target) = &header.materialized_body {
                packed.push(target.clone());
            }
        }
        for application in &module.syntax.applications {
            module_expression_paths(&application.expression, module.text, &mut packed);
            packed.push(application.template.clone());
            packed.extend(application.arguments.iter().cloned());
            if names.contains(application.name.as_str()) {
                packed.push(application.name.clone());
            }
        }
        let qualifiers: BTreeSet<&str> = references
            .iter()
            .filter_map(|r| r.qualifier.as_deref())
            .flat_map(|q| q.match_indices('.').map(|(i, _)| &q[..i]).chain([q]))
            .chain(packed.iter().map(String::as_str))
            .collect();
        // `T.member`, a companion, uses the type `T`
        names.extend(qualifiers.iter().filter(|q| !q.contains('.')));
        for (import, interface) in self.imports() {
            let Some(interface) = interface else {
                continue;
            };
            let unused: Vec<&Item> = import
                .exposing
                .iter()
                .filter(|item| !used(item, interface, &names))
                .collect();
            // every module has the heads of std's plugin: an import of one is
            // needed only for a block it lets the module write without `{`
            let of_std = implicit_package_of(&import.module) == Some(first_party::PACKAGE);
            let needless: Vec<&Item> = (import.plugins.iter())
                .filter(|p| of_std && first_party::HEADS.iter().any(|(h, _)| *h == p.name))
                .filter(|p| !module.syntax.bodiless.contains(&p.name))
                .collect();
            // blocks of the plugins it exposes use it
            let whole =
                unused.len() == import.exposing.len() && needless.len() == import.plugins.len();
            if !qualifiers.contains(import.qualifier())
                && !qualifiers.contains(import.module.as_str())
                && whole
            {
                // importing a module brings its instances, which may be the point
                if interface
                    .instances
                    .iter()
                    .any(|i| i.module == import.module)
                {
                    // the import stays for them; a head of std still goes
                    for item in needless {
                        diagnostics.push(needless_head(module.text, import, item));
                    }
                } else {
                    diagnostics.push(unused_import(module, import));
                }
                continue;
            }
            for item in unused {
                // Synthesized imports have no separate source `exposing`
                // clause, so they have no user-editable item to diagnose.
                if item.span == import.span {
                    continue;
                }
                diagnostics.push(unused_item(module.text, import, item));
            }
            for item in needless {
                diagnostics.push(needless_head(module.text, import, item));
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
            d.with_fix(|| {
                (
                    "remove the import",
                    vec![Edit {
                        span: Span::new(import.span.file, start, end),
                        replacement: String::new(),
                    }],
                )
            })
        }
        None => d,
    }
}

/// `R012` on `plugin head` for a head of std's plugin, which every module
/// has; removing it is safe.
fn needless_head(text: &str, import: &Import, item: &Item) -> Diagnostic {
    let message = format!("`plugin {}` is not needed: every module has it", item.name);
    warning(codes::R012, message, item.span).with_safe_fix(|| {
        let (start, end) = item_removal(text, import, item.span);
        (
            format!("remove `plugin {}` from the import", item.name),
            vec![Edit {
                span: Span::new(item.span.file, start, end),
                replacement: String::new(),
            }],
        )
    })
}

fn unused_item(text: &str, import: &Import, item: &Item) -> Diagnostic {
    let message = if item.kind == ItemKind::All {
        format!("nothing `(..)` brings from `{}` is used", import.module)
    } else {
        format!("`{}` is imported but never used", item.name)
    };
    warning(codes::R012, message, item.span).with_fix(|| {
        let what = if item.kind == ItemKind::All {
            "`(..)`".to_owned()
        } else {
            format!("`{}`", item.name)
        };
        let (start, end) = item_removal(text, import, item.span);
        (
            format!("remove {what} from the import"),
            vec![Edit {
                span: Span::new(item.span.file, start, end),
                replacement: String::new(),
            }],
        )
    })
}

/// The text to delete with an `exposing` item: its comma, or the whole
/// `exposing (…)` when it is the only item.
pub(crate) fn item_removal(text: &str, import: &Import, item: Span) -> (u32, u32) {
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
