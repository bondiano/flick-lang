//! Types marked `@label kind`: a top-level value of one, not a function, is
//! given its name, `Module.name`, where the program runs, for the tools
//! that show it. The kind is the mark's own word: a build may keep the
//! labels of some kinds only.

use flick_ir::Label;

use super::ty::Type;
use super::ui::members;
use super::{Checker, let_parts};
use crate::iface::{self, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The types of this module marked `@label`, each with its kind, and
    /// those of the modules whose interfaces it knows.
    // shortcut: a labelled type of a module this one does not know gives no label; pass the marks on, as those of cells are, when a package's types need it
    pub fn declare_labels(&mut self, decls: &[&Node]) {
        // this module's first: `own_labels` are those at the front
        let mut labels = kinds(decls, self.text, &self.env.module);
        for interface in self.env.known_interfaces() {
            for (name, of) in interface.tags.get(iface::LABEL).into_iter().flatten() {
                if let Some(kind) = of.first() {
                    labels.push((interface.module.clone(), name.clone(), kind.clone()));
                }
            }
        }
        self.labels = labels;
    }

    /// The labelled types of this module, as its interface tells them: each
    /// with its kind.
    pub(super) fn own_labels(&self) -> impl Iterator<Item = (&String, &String)> {
        let own = |(module, ..): &&(String, String, String)| *module == self.env.module;
        (self.labels.iter().take_while(own)).map(|(_, name, kind)| (name, kind))
    }

    /// The top-level values of a labelled type, which the generated code
    /// labels `Module.name`.
    pub fn labelled(&mut self, decls: &[&Node]) {
        for decl in members(decls) {
            if decl.kind != K::LetDecl {
                continue;
            }
            let parts = let_parts(decl, self.text);
            let Some(name) = parts.name.clone().filter(|_| !parts.is_function()) else {
                continue;
            };
            let entity = self.env.entity(Namespace::Value, &name);
            let Some(ty) = self.env.value(&entity).cloned() else {
                continue;
            };
            // the type as it is written: an alias has a mark of its own
            let Type::Con { module, name: ty } = self.bare(&ty).spine().0.clone() else {
                continue;
            };
            let marked = |(of, known, _): &&(String, String, String)| *of == module && *known == ty;
            if let Some((.., kind)) = self.labels.iter().find(marked) {
                let kind = kind.clone();
                self.typing.labelled.insert(name, Label { kind });
            }
        }
    }
}

/// The types of `module` an attribute `@label kind` stands before, each
/// with the kind.
fn kinds(decls: &[&Node], text: &str, module: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut pending = None;
    for decl in decls {
        if decl.kind == K::Attribute {
            let mut words = (decl.child_tokens()).filter(|t| t.kind == K::Ident);
            if words.next().is_some_and(|w| w.text(text) == iface::LABEL) {
                pending = words.next().map(|kind| kind.text(text).to_owned());
            }
            continue;
        }
        if let Some(kind) = pending.take()
            && let Some(name) = decl.child_tokens().find(|t| t.kind == K::Con)
        {
            out.push((module.to_owned(), name.text(text).to_owned(), kind));
        }
    }
    out
}
