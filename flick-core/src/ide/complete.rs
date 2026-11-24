//! Completion: after `x.` the fields of a record (open rows too), after
//! `M.` the exports of a module or the operations of a service; otherwise
//! the locals in scope, the module's own names, imported names and the
//! Prelude's, constructors included, those that fit the place first.

use std::collections::BTreeMap;
use std::iter;

use super::expect::{Expected, MISFIT, hole};
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
use crate::types::{Form, Type, has_hole, show_signature, show_type};

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
    /// Where the item goes in the list: locals first, auto-imports last,
    /// and where the place expects a type, after every item that fits it.
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

/// Whether `offset` is at a block statement position lexically inside an
/// explicit `scope { … }` without an intervening deferred or cleanup boundary.
fn in_defer_scope(tree: &Node, offset: u32) -> bool {
    let path = tree::path_to(tree, offset);
    let statement_block = match path.as_slice() {
        [.., parent, last]
            if parent.kind == K::Block && matches!(last.kind, K::NameExpr | K::Error) =>
        {
            path.len() - 2
        }
        [.., last] if last.kind == K::Block => path.len() - 1,
        _ => return false,
    };
    if let Some(&owner) = statement_block.checked_sub(1).and_then(|i| path.get(i)) {
        match owner.kind {
            K::ForExpr
                if owner
                    .nodes()
                    .next()
                    .is_some_and(|c| c.range() == path[statement_block].range()) =>
            {
                return false;
            }
            K::DoExpr | K::AssignStmt | K::RecordField | K::ProofDecl => return false,
            K::LetDecl
                if !owner.nodes().any(|c| c.kind == K::Params)
                    && owner
                        .child_tokens()
                        .find(|t| t.kind == K::ElseKw)
                        .is_none_or(|kw| path[statement_block].range().0 < kw.end) =>
            {
                return false;
            }
            _ => {}
        }
    }
    for (i, &node) in path[..statement_block].iter().enumerate().rev() {
        match node.kind {
            K::ScopeExpr => return true,
            K::AppExpr if has_hole(node) => return false,
            K::FunExpr | K::TaskExpr | K::LazyExpr | K::DeferStmt => return false,
            K::LetDecl
                if node.nodes().any(|c| c.kind == K::Params) || node.has_token(K::LazyKw) =>
            {
                return false;
            }
            K::ForExpr if !node.has_token(K::DoKw) => return false,
            K::MatchArm
                if i > 0
                    && path[i - 1].kind == K::TryExpr
                    && node
                        .nodes()
                        .next()
                        .is_some_and(|p| p.kind == K::ConPat && p.has_token(K::Dot)) =>
            {
                return false;
            }
            _ => {}
        }
    }
    false
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
        if !dotted && let Some(items) = self.foreign_meta_complete(file, offset) {
            return items;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(items) = self.proof_completion(file, offset, dotted) {
            return items;
        }
        let mut items = if dotted {
            self.after_dot(file, offset)
        } else {
            let expected = self.expected(file, offset);
            let mut items = self.complete_names(file, offset, expected.as_ref());
            items.extend(self.auto_imports(file, offset, &items, expected.as_ref()));
            items
        };
        self.enrich(&mut items);
        items
    }

    pub(super) fn complete_names(
        &self,
        file: FileId,
        offset: u32,
        expected: Option<&Expected>,
    ) -> Vec<Completion> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = &self.sources.get(file).text;
        let mut out: BTreeMap<String, Completion> = BTreeMap::new();
        let mut add = |sort: u8, c: Completion, ty: Option<&Type>| {
            let fits = expected.is_none_or(|x| x.misfit(self, c.kind, ty) == 0);
            // the first of a name is shown; the name of a type is also its
            // constructor's, as of a `type error`, and one that fits moves it up
            let new = !out.contains_key(&c.label);
            let shown = out.entry(c.label.clone()).or_insert(Completion {
                sort: sort + MISFIT,
                ..c
            });
            if fits && shown.sort >= MISFIT && (new || shown.kind == CompletionKind::Type) {
                shown.sort -= MISFIT;
            }
        };
        // a hole after a space, as at the end of `f x `, is in the scope of `x`
        let scope = hole(text, offset).unwrap_or(offset);
        for (name, binder) in locals::in_scope(&m.tree, scope, text).into_iter().rev() {
            let ty = m
                .typing
                .types
                .iter()
                .find(|(r, _)| r.0 == binder.0)
                .map(|(_, t)| t);
            add(
                LOCAL,
                item(&name, CompletionKind::Local, ty.map(show_signature)),
                ty,
            );
        }
        for e in visible(&m.interface) {
            add(OWN, completion(e), e.scheme.as_ref());
        }
        for d in declarations(&m.tree, text, file) {
            let kind = match d.namespace {
                Namespace::Value => CompletionKind::Value,
                Namespace::Constructor => CompletionKind::Constructor,
                Namespace::Type => CompletionKind::Type,
            };
            // what the module does not export is typed among its declarations
            let ty = expected
                .and_then(|_| m.typing.declarations.find(d.namespace, &d.name))
                .and_then(|e| e.scheme.as_ref());
            add(
                OWN,
                Completion {
                    module: Some(m.name.clone()),
                    ..item(&d.name, kind, None)
                },
                ty,
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
            add(IMPORTED, item(name, CompletionKind::Module, None), None);
            for e in exposed(import, interface, text) {
                add(IMPORTED, completion(e), e.scheme.as_ref());
            }
        }
        if let Some(prelude) = self.interface(lang::prelude()) {
            for e in visible(prelude) {
                add(PRELUDE_NAMES, completion(e), e.scheme.as_ref());
            }
        }
        if in_defer_scope(&m.tree, scope) {
            for (label, detail, snippet) in [
                (
                    "defer",
                    "scope-owned cleanup (runs on every scope close)",
                    "defer {\n  $0\n}",
                ),
                (
                    "errdefer",
                    "scope-owned cleanup (runs when scope body fails)",
                    "errdefer {\n  $0\n}",
                ),
            ] {
                out.entry(label.to_owned()).or_insert_with(|| Completion {
                    snippet: Some(snippet.to_owned()),
                    sort: PRELUDE_NAMES,
                    ..item(label, CompletionKind::Keyword, Some(detail.to_owned()))
                });
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
        let before = &text[..offset as usize];
        let qualifier = &before[before
            .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            .len()..];
        if qualifier.starts_with(|c: char| c.is_ascii_uppercase()) {
            if let Some(interface) =
                self.completion_interface(&self.unalias(m, qualifier.to_owned()), m)
            {
                return exports(interface);
            }
            // `Api.Public.` or `Public.`: the companions of a type
            let (owner, interface) = match qualifier.rsplit_once('.') {
                Some((module, owner)) => (
                    owner,
                    self.completion_interface(&self.unalias(m, module.to_owned()), m),
                ),
                None => (qualifier, Some(&m.interface)),
            };
            let companions = interface.map(|i| companions(i, owner)).unwrap_or_default();
            if !companions.is_empty() {
                return companions;
            }
            if let Some(ops) = local_operations(&m.tree, text, qualifier) {
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
                .find_map(|i| i.find(Namespace::Type, qualifier)?.definition.as_ref());
            if let Some(def) = service
                && let Form::Service { operations, .. } = &def.form
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
        self.record_fields(file, ty, &mut fields, 0);
        fields
            .into_iter()
            .map(|(label, t)| item(&label, CompletionKind::Field, Some(show_type(&t))))
            .collect()
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
    visible(interface).map(completion).collect()
}

fn visible(interface: &Interface) -> impl Iterator<Item = &Export> {
    (interface.exports.iter()).filter(|e| !e.entity.name.contains('$'))
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

pub(super) fn completion(e: &Export) -> Completion {
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
