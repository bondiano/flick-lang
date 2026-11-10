//! Completion: after `x.` the fields of a record (open rows too), after
//! `M.` the exports of a module or the operations of a service; otherwise
//! the locals in scope, the module's own names, imported names and the
//! Prelude's, constructors included.

use std::collections::BTreeMap;
use std::iter;

use super::hover::companions::companion_note;
use super::{Ide, locals, tree};
use crate::diagnostics::Edit;
use crate::iface::{Export, Interface, Namespace};
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang;
use crate::types::{Form, Type, show_signature, show_type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompletionKind {
    Local,
    Value,
    Constructor,
    Type,
    Module,
    Field,
    Operation,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    /// The source type, when known.
    pub detail: Option<String>,
    /// The `///` documentation, as Markdown.
    pub documentation: Option<String>,
    /// A snippet to insert instead of the label, `$1`-style placeholders
    /// for the arguments of a function.
    pub snippet: Option<String>,
    /// Where the item goes in the list: locals first, auto-imports last.
    pub sort: u8,
    /// The module that defines a top-level name.
    pub module: Option<String>,
    /// More edits, such as the `import` an auto-import needs.
    pub edits: Vec<Edit>,
}

pub(super) fn item(label: &str, kind: CompletionKind, detail: Option<String>) -> Completion {
    Completion {
        label: label.to_owned(),
        kind,
        detail,
        documentation: None,
        snippet: None,
        sort: 0,
        module: None,
        edits: Vec::new(),
    }
}

/// The sort groups of names in scope.
const LOCAL: u8 = 0;
const OWN: u8 = 1;
const IMPORTED: u8 = 2;
const PRELUDE_NAMES: u8 = 3;

impl Ide<'_> {
    fn completion_interface(&self, module: &str, from: &Analyzed) -> Option<&Interface> {
        let interface = self.interface(module)?;
        if interface.private
            && self
                .analysis
                .module(module)
                .is_none_or(|owner| owner.package != from.package)
        {
            return None;
        }
        Some(interface)
    }

    /// Completions at `offset` of `file`. With `dotted`, `offset` is the end
    /// of what comes before a `.`.
    pub fn complete(&self, file: FileId, offset: u32, dotted: bool) -> Vec<Completion> {
        if let Some(items) = self.plugin_complete(file, offset, dotted) {
            return items;
        }
        let mut items = if dotted {
            self.after_dot(file, offset)
        } else {
            let mut items = self.complete_names(file, offset);
            items.extend(self.auto_imports(file, offset, &items));
            items
        };
        self.enrich(&mut items);
        items
    }

    pub(super) fn complete_names(&self, file: FileId, offset: u32) -> Vec<Completion> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = &self.sources.get(file).text;
        let mut out: BTreeMap<String, Completion> = BTreeMap::new();
        let mut add = |sort: u8, c: Completion| {
            out.entry(c.label.clone())
                .or_insert(Completion { sort, ..c });
        };
        for (name, binder) in locals::in_scope(&m.tree, offset, text).into_iter().rev() {
            let ty = m
                .typing
                .types
                .iter()
                .find(|(r, _)| r.0 == binder.0)
                .map(|(_, t)| show_signature(t));
            add(LOCAL, item(&name, CompletionKind::Local, ty));
        }
        for c in exports(&m.interface) {
            add(OWN, c);
        }
        for d in declarations(&m.tree, text, file) {
            let kind = match d.namespace {
                Namespace::Value => CompletionKind::Value,
                Namespace::Constructor => CompletionKind::Constructor,
                Namespace::Type => CompletionKind::Type,
            };
            add(
                OWN,
                Completion {
                    module: Some(m.name.clone()),
                    ..item(&d.name, kind, None)
                },
            );
        }
        for import in m.tree.nodes().filter(|n| n.kind == K::ImportDecl) {
            let Some(path) = import.nodes().find(|n| n.kind == K::ModulePath) else {
                continue;
            };
            let module: String = path.text(text).split_whitespace().collect();
            let Some(interface) = self.completion_interface(&module, m) else {
                continue;
            };
            // `import Conduit.Server.Session as Auth` is `Auth` here
            let name = tree::name_of(import, K::Con, text)
                .unwrap_or_else(|| module.rsplit('.').next().unwrap_or(&module));
            add(IMPORTED, item(name, CompletionKind::Module, None));
            for e in exposed(import, interface, text) {
                add(IMPORTED, completion(e));
            }
        }
        if let Some(prelude) = self.interface(lang::prelude()) {
            for c in exports(prelude) {
                add(PRELUDE_NAMES, c);
            }
        }
        out.into_values().collect()
    }

    fn after_dot(&self, file: FileId, offset: u32) -> Vec<Completion> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = &self.sources.get(file).text;
        // `M.` or `M.N.`: a module, or a service's operations
        let qualifier: String = text[..offset as usize]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if qualifier.starts_with(|c: char| c.is_ascii_uppercase()) {
            if let Some(interface) =
                self.completion_interface(&self.unalias(m, qualifier.clone()), m)
            {
                return exports(interface);
            }
            // `Api.Public.` or `Public.`: the companions of a type
            let (owner, interface) = match qualifier.rsplit_once('.') {
                Some((module, owner)) => (
                    owner,
                    self.completion_interface(&self.unalias(m, module.to_owned()), m),
                ),
                None => (qualifier.as_str(), Some(&m.interface)),
            };
            let companions = interface.map(|i| companions(i, owner)).unwrap_or_default();
            if !companions.is_empty() {
                return companions;
            }
            if let Some(ops) = local_operations(&m.tree, text, &qualifier) {
                return ops;
            }
            let service = iter::once(&m.interface)
                .chain(
                    self.analysis
                        .modules
                        .iter()
                        .filter(|x| !x.inline || x.file == m.file)
                        .filter(|x| !x.interface.private || x.package == m.package)
                        .map(|x| &x.interface),
                )
                .find_map(|i| i.find(Namespace::Type, &qualifier)?.definition.clone());
            if let Some(def) = service
                && let Form::Service { operations, .. } = def.form
            {
                return operations
                    .iter()
                    .map(|o| {
                        item(
                            &o.name,
                            CompletionKind::Operation,
                            Some(show_signature(&o.ty)),
                        )
                    })
                    .collect();
            }
        }
        let Some((_, ty)) = m
            .typing
            .types
            .iter()
            .filter(|(r, _)| r.1 == offset)
            .min_by_key(|(r, _)| r.1 - r.0)
        else {
            return Vec::new();
        };
        let mut fields = Vec::new();
        self.fields(ty, &mut fields, 0);
        fields
            .into_iter()
            .map(|(label, t)| item(&label, CompletionKind::Field, Some(show_type(&t))))
            .collect()
    }

    /// The known fields of a record type, through transparent aliases.
    fn fields(&self, ty: &Type, out: &mut Vec<(String, Type)>, depth: u32) {
        let mut ty = ty;
        while let Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } = ty {
            ty = inner;
        }
        match ty {
            Type::Record(row) => out.extend(row.entries.iter().cloned()),
            Type::App { .. } | Type::Con { .. } if depth < 8 => {
                let (head, args) = ty.spine();
                let Type::Con { module, name } = head else {
                    return;
                };
                let def = self
                    .interface(module)
                    .and_then(|i| i.find(Namespace::Type, name)?.definition.clone());
                if let Some(Form::Alias { params, body, .. }) = def.map(|d| d.form) {
                    let subst: Vec<(String, Type)> =
                        params.into_iter().zip(args.into_iter().cloned()).collect();
                    self.fields(&body.substitute(&subst), out, depth + 1);
                }
            }
            _ => {}
        }
    }
}

/// The exports of `interface` that `import` exposes unqualified.
pub(super) fn exposed<'i>(
    import: &Node,
    interface: &'i Interface,
    text: &str,
) -> impl Iterator<Item = &'i Export> {
    let exposing: Vec<String> = import
        .nodes()
        .filter(|n| n.kind == K::Exposing)
        .flat_map(Node::nodes)
        .map(|i| i.text(text).split_whitespace().collect())
        .collect();
    let all = exposing.iter().any(|e| e == "..");
    (interface.exports.iter())
        .filter(|e| !e.entity.name.contains('$'))
        .filter(move |e| {
            let label = &e.entity.name;
            all || exposing.iter().any(|x| {
                x == label
                    || x.strip_suffix("(..)") == Some(label.as_str())
                    || e.parent
                        .as_ref()
                        .is_some_and(|p| x.strip_suffix("(..)") == Some(p.as_str()))
            })
        })
}

/// The values, constructors and types an interface exports, without the
/// companions of types.
pub(super) fn exports(interface: &Interface) -> Vec<Completion> {
    interface
        .exports
        .iter()
        .filter(|e| !e.entity.name.contains('$'))
        .map(completion)
        .collect()
}

/// The companions of the type `owner`, such as `api` of `Public`, under
/// their member names.
fn companions(interface: &Interface, owner: &str) -> Vec<Completion> {
    interface
        .exports
        .iter()
        .filter_map(|e| {
            let member = e.entity.name.strip_prefix(owner)?.strip_prefix('$')?;
            Some(Completion {
                documentation: companion_note(member).map(str::to_owned),
                ..item(
                    member,
                    CompletionKind::Value,
                    e.scheme.as_ref().map(show_signature),
                )
            })
        })
        .collect()
}

fn completion(e: &Export) -> Completion {
    let kind = match e.entity.namespace {
        Namespace::Value => CompletionKind::Value,
        Namespace::Constructor => CompletionKind::Constructor,
        Namespace::Type => CompletionKind::Type,
    };
    Completion {
        module: Some(e.entity.module.clone()),
        ..item(&e.entity.name, kind, e.scheme.as_ref().map(show_signature))
    }
}

/// The operations of a service the module declares, with their signatures.
fn local_operations(root: &Node, text: &str, service: &str) -> Option<Vec<Completion>> {
    let decl = root.nodes().find(|d| {
        d.kind == K::ServiceDecl
            && d.child_tokens()
                .any(|t| t.kind == K::Con && t.text(text) == service)
    })?;
    Some(
        decl.nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::Signature)
            .filter_map(|sig| {
                let name = sig.child_tokens().find(|t| t.kind == K::Ident)?;
                let ty = sig.nodes().next().map(|t| {
                    t.text(text)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                });
                Some(item(name.text(text), CompletionKind::Operation, ty))
            })
            .collect(),
    )
}
