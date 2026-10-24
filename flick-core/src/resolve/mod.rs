//! Names and modules: top-level definitions,
//! export lists, imports and the names they bring into scope. Rules:
//! `docs/src/content/docs/reference/modules.md`.

mod dead;
mod defs;
mod scope;
mod syntax;
mod unexported;
mod unused;
mod uses;

pub(crate) use defs::shortest_cycle;
pub use syntax::{Header, Import, Item, ItemKind, ModuleSyntax, module_syntax};

/// A top-level name a module declares, signatures included.
#[derive(Debug, Clone)]
pub struct Declared {
    pub namespace: Namespace,
    pub name: String,
    pub span: Span,
    /// Type of a constructor, class of a method.
    pub parent: Option<String>,
    pub decl: Decl,
    pub signature: bool,
}

/// Every top-level name `tree` declares, in source order.
pub fn declarations(tree: &Node, text: &str, file: FileId) -> Vec<Declared> {
    defs::collect(tree, text, file)
        .into_iter()
        .map(|d| Declared {
            namespace: d.namespace,
            name: d.name,
            span: d.span,
            parent: d.parent,
            decl: d.decl,
            signature: d.signature,
        })
        .collect()
}

pub use crate::iface::{Decl, Entity, Export, Hidden, Interface, Namespace};

use crate::diagnostics::Diagnostic;
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;

/// One parsed module to resolve.
pub struct Module<'a> {
    /// Full module name; empty for a file that names no module.
    pub name: &'a str,
    pub file: FileId,
    pub text: &'a str,
    pub tree: &'a Node,
    pub syntax: &'a ModuleSyntax,
}

/// Resolves one module against the interfaces of its imports; `imported`
/// returns `None` for a module that could not be loaded, whose names are then
/// taken on trust (the failed import is reported once, by the loader).
pub fn resolve(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
) -> (Interface, Vec<Diagnostic>) {
    let all = defs::collect(module.tree, module.text, module.file);
    let mut diagnostics = defs::duplicates(&all);
    let defs = defs::defined(all.clone());
    diagnostics.extend(defs::alias_cycles(module.tree, module.text, module.file));
    let scope = scope::Scope::new(module, &defs, imported, &mut diagnostics);
    let interface = scope.exports(&mut diagnostics);
    let (references, unused) = uses::references(module.tree, module.text, module.file);
    for reference in &references {
        scope.check_use(reference, &mut diagnostics);
    }
    scope.unused_imports(&references, &mut diagnostics);
    diagnostics.extend(dead::dead_values(module, &all, &interface, &references));
    diagnostics.extend(unused::unused_locals(unused));
    (interface, diagnostics)
}
