//! What a module has in scope, for the `in-scope` question of its plugins:
//! values with their types and types by the names the module writes them
//! with, qualified through imports or not, and the transparent aliases of
//! every module it sees.

use std::sync::Arc;

use super::Ide;
use super::complete::exposed;
use crate::iface::{Export, Interface, Namespace};
use crate::plugin::source::InScope;
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Type, lang};

impl Ide<'_> {
    pub(super) fn in_scope(&self, m: &Analyzed) -> Arc<InScope> {
        let text = &self.sources.get(m.file).text;
        let mut scope = InScope::default();
        // the module's own names first: they hide those it imports
        for d in declarations(&m.tree, text, m.file) {
            match d.namespace {
                Namespace::Value => {
                    if let Some(ty) = m.typing.types.get(&(d.span.start, d.span.end)) {
                        scope.values.push((d.name, ty.clone()));
                    }
                }
                Namespace::Type => {
                    let ty = Type::con(&m.name, &d.name);
                    scope.types.entry(d.name).or_insert(ty);
                }
                Namespace::Constructor => {}
            }
        }
        let mut interfaces = Vec::new();
        for import in m.tree.nodes().filter(|n| n.kind == K::ImportDecl) {
            let Some(path) = import.nodes().find(|n| n.kind == K::ModulePath) else {
                continue;
            };
            let module: String = path.text(text).split_whitespace().collect();
            let Some(interface) = self.interface(&module) else {
                continue;
            };
            let qualifier = (import.child_tokens())
                .skip_while(|t| t.kind != K::AsKw)
                .find(|t| t.kind == K::Con)
                .map_or(module, |t| t.text(text).to_owned());
            for e in exported(&interface) {
                add(&mut scope, format!("{qualifier}.{}", e.entity.name), e);
            }
            for e in exposed(import, &interface, text) {
                add(&mut scope, e.entity.name.clone(), e);
            }
            interfaces.push(interface);
        }
        if let Some(prelude) = self.interface(lang::prelude()) {
            for e in exported(&prelude) {
                add(&mut scope, e.entity.name.clone(), e);
            }
            interfaces.push(prelude);
        }
        for (name, alias) in &m.typing.aliases {
            scope
                .aliases
                .insert((m.name.clone(), name.clone()), alias.clone());
        }
        for e in interfaces.iter().flat_map(exported) {
            if let Some(Form::Alias { params, body, .. }) = e.definition.as_ref().map(|d| &d.form) {
                let key = (e.entity.module.clone(), e.entity.name.clone());
                scope
                    .aliases
                    .entry(key)
                    .or_insert_with(|| (params.clone(), body.clone()));
            }
        }
        // a name written twice is the first: the stable sort keeps it ahead
        scope.values.sort_by(|a, b| a.0.cmp(&b.0));
        scope.values.dedup_by(|later, first| later.0 == first.0);
        Arc::new(scope)
    }
}

/// `e` under the name `label`, unless an earlier name hides it.
fn add(scope: &mut InScope, label: String, e: &Export) {
    match (e.entity.namespace, &e.scheme) {
        (Namespace::Value, Some(scheme)) => scope.values.push((label, scheme.clone())),
        (Namespace::Type, _) => {
            let ty = Type::con(&e.entity.module, &e.entity.name);
            scope.types.entry(label).or_insert(ty);
        }
        _ => {}
    }
}

/// What an interface exports under a name the user writes.
fn exported(interface: &Interface) -> impl Iterator<Item = &Export> {
    (interface.exports.iter()).filter(|e| !e.entity.name.contains('$'))
}
