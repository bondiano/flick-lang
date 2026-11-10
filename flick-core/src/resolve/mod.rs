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

pub(crate) use defs::{alias_names, shortest_cycle};
pub use syntax::{
    FunctorParam, Header, Import, Item, ItemKind, ModuleApplication, ModuleSyntax, module_syntax,
};
#[cfg(feature = "ide")]
pub(crate) use unused::item_removal;

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

/// Top-level names used by a synthetic inline-module body. The enclosing
/// file uses these to account for references to its preceding declarations.
pub(crate) fn referenced_names(module: &Module) -> Vec<(Namespace, String)> {
    uses::references(module)
        .0
        .into_iter()
        .filter(|reference| reference.qualifier.is_none())
        .map(|reference| (reference.namespace, reference.name))
        .collect()
}

/// Import-use warnings for a whole source file, including declarations of
/// inline namespaces. Checking the namespaces separately must still count an
/// import once for the file that wrote it.
pub(crate) fn import_warnings(
    module: &Module,
    imported: &dyn Fn(&str) -> Option<Interface>,
) -> Vec<Diagnostic> {
    let defs = defs::defined(defs::collect(module.tree, module.text, module.file));
    let mut ignored = Vec::new();
    let scope = scope::Scope::new(module, &defs, imported, &mut ignored);
    let (references, _) = uses::references(module);
    let mut diagnostics = Vec::new();
    scope.unused_imports(&references, &mut diagnostics);
    diagnostics
}

pub use crate::iface::{Decl, Entity, Export, Hidden, Interface, Namespace};

#[cfg(test)]
use std::collections::BTreeSet;

use crate::diagnostics::Diagnostic;
use crate::doc;
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
    /// Where the code plugins generated starts in `text`, [`NO_GENERATED`]
    /// without any: a local is seen only by names on its side of it.
    pub generated: u32,
}

/// [`Module::generated`] of a module without generated code.
pub const NO_GENERATED: u32 = u32::MAX;

/// The qualifier of each reference in `module`, as written: `M` of `M.name`,
/// `M.N` of `M.N.name`.
#[cfg(test)]
pub(crate) fn qualifiers(module: &Module) -> BTreeSet<String> {
    let (references, _) = uses::references(module);
    (references.into_iter())
        .filter_map(|reference| reference.qualifier)
        .collect()
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
    let mut interface = scope.exports(&mut diagnostics);
    interface.private = module.syntax.header.as_ref().is_some_and(|h| h.private);
    let mut docs = doc::comments(module.text);
    interface.docs = (interface.exports.iter())
        .filter(|e| e.entity.module == interface.module)
        .filter_map(|e| Some((e.entity.name.clone(), docs.remove(&e.entity.name)?)))
        .collect();
    let (references, unused) = uses::references(module);
    for reference in &references {
        scope.check_use(reference, &mut diagnostics);
    }
    scope.unused_imports(&references, &mut diagnostics);
    diagnostics.extend(dead::dead_values(module, &all, &interface, &references));
    diagnostics.extend(unused::unused_locals(unused));
    (interface, diagnostics)
}
