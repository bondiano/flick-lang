//! Names and modules (§11 "Модули и entry point"): top-level definitions,
//! export lists, imports and the names they bring into scope. Rules:
//! `docs/reference/modules.md`.

mod defs;
mod scope;
mod syntax;
mod uses;

pub(crate) use defs::shortest_cycle;
pub use syntax::{Header, Import, Item, ItemKind, ModuleSyntax, module_syntax};

pub use crate::iface::{Decl, Entity, Export, Hidden, Interface, Namespace};

use crate::diagnostics::Diagnostic;
use crate::source::FileId;
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
    let defs = defs::collect(module.tree, module.text, module.file);
    let mut diagnostics = defs::duplicates(&defs);
    let defs = defs::defined(defs);
    diagnostics.extend(defs::alias_cycles(module.tree, module.text, module.file));
    let scope = scope::Scope::new(module, &defs, imported, &mut diagnostics);
    let interface = scope.exports(&mut diagnostics);
    for reference in uses::references(module.tree, module.text, module.file) {
        scope.check_use(&reference, &mut diagnostics);
    }
    (interface, diagnostics)
}
