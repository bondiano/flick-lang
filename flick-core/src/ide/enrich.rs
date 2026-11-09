//! What makes a completion more than a name: its documentation, a snippet
//! of its arguments, and the import that a name from elsewhere needs.

use std::collections::{HashMap, HashSet};

use super::Ide;
use super::complete::{Completion, CompletionKind, exports};
use crate::diagnostics::Edit;
use crate::iface::{Entity, Namespace};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::builtins;

/// The sort group of names that need a qualifier or an import.
const ELSEWHERE: u8 = 4;

/// The `///` comments and parameter names of a module's declarations.
struct Known {
    docs: HashMap<String, String>,
    params: HashMap<String, Vec<String>>,
}

impl Ide<'_> {
    /// Fills the documentation and argument snippets of top-level names,
    /// reading each module's source once.
    pub(super) fn enrich(&self, items: &mut [Completion]) {
        let mut known: HashMap<String, Option<Known>> = HashMap::new();
        for c in items.iter_mut() {
            let Some(module) = c.module.clone() else {
                continue;
            };
            let Some(k) = known
                .entry(module)
                .or_insert_with_key(|m| {
                    let source = self.source(m)?;
                    Some(Known {
                        docs: source.docs(),
                        params: params(&source.tree, &source.text),
                    })
                })
                .as_ref()
            else {
                continue;
            };
            c.documentation = k.docs.get(&c.label).cloned();
            if c.kind == CompletionKind::Value
                && let Some(names) = k.params.get(&c.label)
            {
                let head = c.snippet.clone().unwrap_or_else(|| c.label.clone());
                let args: Vec<String> = names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| format!("${{{}:{n}}}", i + 1))
                    .collect();
                c.snippet = Some(format!("{head} {}", args.join(" ")));
            }
        }
    }

    /// Values and constructors of modules the file does not import that
    /// start with the word at `offset`, as `Module.name`; a project
    /// module also gets its `import`, a built-in one needs none.
    pub(super) fn auto_imports(
        &self,
        file: FileId,
        offset: u32,
        in_scope: &[Completion],
    ) -> Vec<Completion> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = self.text(file);
        // the word around `offset`, which may be its start or its end
        let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let at = offset as usize;
        let start = text[..at].trim_end_matches(ident).len();
        let end = text[at..]
            .find(|c: char| !ident(c))
            .map_or(text.len(), |i| at + i);
        let word = &text[start..end];
        // a short prefix matches too much to be useful
        if word.len() < 2 || word.starts_with(|c: char| c.is_ascii_digit()) {
            return Vec::new();
        }
        let imported: HashSet<String> = m
            .tree
            .nodes()
            .filter(|n| n.kind == K::ImportDecl)
            .filter_map(|n| n.nodes().find(|p| p.kind == K::ModulePath))
            .map(|p| p.text(text).split_whitespace().collect())
            .collect();
        let local: HashSet<&str> = in_scope.iter().map(|c| c.label.as_str()).collect();
        let project = self
            .analysis
            .modules
            .iter()
            .filter(|x| !x.inline)
            .map(|x| (x.name.clone(), true));
        // shortcut: built-in modules split into parts (`List.Core`) re-export through the whole
        let builtin = builtins()
            .iter()
            .filter(|i| {
                !i.module.contains('.') && !matches!(i.module.as_str(), "Prelude" | "Instances")
            })
            .map(|i| (i.module.clone(), false));
        let mut seen: HashSet<Entity> = HashSet::new();
        let mut out = Vec::new();
        for (module, needs_import) in project.chain(builtin) {
            if module == m.name || imported.contains(&module) {
                continue;
            }
            let Some(interface) = self.interface(&module) else {
                continue;
            };
            if interface.private
                && self
                    .analysis
                    .module(&module)
                    .is_none_or(|owner| owner.package != m.package)
            {
                continue;
            }
            let items = exports(&interface);
            for e in &interface.exports {
                let name = &e.entity.name;
                if !name.starts_with(word)
                    || name.contains('$')
                    || local.contains(name.as_str())
                    || e.entity.namespace == Namespace::Type
                    || !seen.insert(e.entity.clone())
                {
                    continue;
                }
                let Some(mut c) = items.iter().find(|c| c.label == *name).cloned() else {
                    continue;
                };
                let qualified = format!("{module}.{name}");
                c.detail = Some(
                    c.detail
                        .map_or(qualified.clone(), |t| format!("{qualified} : {t}")),
                );
                c.snippet = Some(qualified);
                c.sort = ELSEWHERE;
                if needs_import {
                    c.edits.push(import_edit(&m.tree, file, &module));
                }
                out.push(c);
            }
        }
        out
    }
}

/// `import module` after the last import, or after the module header.
pub(super) fn import_edit(root: &Node, file: FileId, module: &str) -> Edit {
    let last = root
        .nodes()
        .filter(|n| n.kind == K::ImportDecl)
        .last()
        .map(|n| (n.range().1, format!("\nimport {module}")));
    let (at, replacement) = last.unwrap_or_else(|| {
        let header = root
            .nodes()
            .find(|n| n.kind == K::ModuleHeader)
            .map_or(0, |h| h.range().1);
        (header, format!("\n\nimport {module}"))
    });
    Edit {
        span: Span::new(file, at, at),
        replacement,
    }
}

/// The parameter names of each top-level function `let`.
fn params(root: &Node, text: &str) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    for decl in root.nodes().filter(|n| n.kind == K::LetDecl) {
        let Some(name) = decl
            .nodes()
            .find(|n| n.kind == K::VarPat)
            .and_then(|p| p.child_tokens().find(|t| t.kind == K::Ident))
        else {
            continue;
        };
        let names: Vec<String> = decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
            .map(
                |p| match (p.kind, p.child_tokens().find(|t| t.kind == K::Ident)) {
                    (K::VarPat, Some(t)) => t.text(text).to_owned(),
                    (K::UnitPat, _) => "()".to_owned(),
                    _ => "_".to_owned(),
                },
            )
            .collect();
        if !names.is_empty() {
            out.insert(name.text(text).to_owned(), names);
        }
    }
    out
}
