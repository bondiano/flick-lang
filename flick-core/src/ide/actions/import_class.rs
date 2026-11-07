//! Import a class named by an unresolved type in a signature or constraint.

use super::super::{Ide, enrich::import_edit, tree};
use super::{Action, edit};
use crate::diagnostics::codes;
use crate::iface::{Decl, Namespace};
use crate::resolve::module_syntax;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::builtins;

impl Ide<'_> {
    pub(super) fn import_class(&self, file: FileId, offset: u32) -> Vec<Action> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        if !path
            .iter()
            .any(|n| matches!(n.kind, K::Context | K::ConstrainedType))
        {
            return Vec::new();
        }
        let Some(name_type) = path.iter().rev().find(|n| n.kind == K::NameType) else {
            return Vec::new();
        };
        let Some(token) = name_type.child_tokens().find(|t| t.kind == K::Con) else {
            return Vec::new();
        };
        if !(token.start <= offset && offset <= token.end)
            || !self.analysis.diagnostics.iter().any(|d| {
                d.code == codes::T001
                    && d.span
                        .is_some_and(|s| s.file == file && s.start <= offset && offset <= s.end)
            })
        {
            return Vec::new();
        }
        let name = token.text(text);
        let syntax = module_syntax(&m.tree, text, file);
        let project = self.analysis.modules.iter().map(|m| m.name.clone());
        let builtin = builtins()
            .iter()
            .filter(|i| {
                !i.module.contains('.') && !matches!(i.module.as_str(), "Prelude" | "Instances")
            })
            .map(|i| i.module.clone());
        let mut seen = std::collections::HashSet::new();
        let mut actions = Vec::new();
        for module in project.chain(builtin) {
            if !seen.insert(module.clone()) || module == m.name {
                continue;
            }
            let Some(interface) = self.interface(&module) else {
                continue;
            };
            if !interface.exports.iter().any(|e| {
                e.decl == Decl::Class
                    && e.entity.namespace == Namespace::Type
                    && e.entity.name == name
            }) {
                continue;
            }
            let change = if let Some(import) = syntax.imports.iter().find(|i| i.module == module) {
                if import
                    .exposing
                    .iter()
                    .any(|i| i.name == name || i.kind == crate::resolve::ItemKind::All)
                {
                    continue;
                }
                let Some(decl) = m.tree.nodes().find(|n| {
                    n.kind == K::ImportDecl
                        && n.range().0 <= import.span.start
                        && import.span.end <= n.range().1
                }) else {
                    continue;
                };
                match import.exposing.last() {
                    Some(last) => edit(file, last.span.end, last.span.end, format!(", {name}")),
                    None => edit(
                        file,
                        decl.range().1,
                        decl.range().1,
                        format!(" exposing ({name})"),
                    ),
                }
            } else {
                let whole = import_edit(&m.tree, file, &module);
                edit(
                    file,
                    whole.span.start,
                    whole.span.end,
                    format!("{} exposing ({name})", whole.replacement),
                )
            };
            actions.push(Action {
                title: format!("import `{name}` from `{module}`"),
                kind: "quickfix",
                edits: vec![change],
            });
        }
        actions
    }
}
