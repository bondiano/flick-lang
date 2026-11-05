//! What makes an atom, an action or a view: the makers of the library, the
//! values its modules mark `@maker`, and every function that calls one
//! outside the task of a handler, in this module or in one it imports,
//! whose interface lists them. A render must call none (`FLICK-U006`).

use std::collections::HashMap;
use std::mem;

use super::ty::Type;
use super::typing::Ref;
use super::ui::members;
use super::{Checker, Range, let_parts};
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The top-level functions of this module that make an atom, an action
    /// or a view, each with the maker of the library it ends at.
    pub fn makers(&mut self, decls: &[&Node]) {
        for name in marked(decls, self.text) {
            let maker = format!("{}.{name}", self.env.module);
            self.typing.makers.insert(name, maker);
        }
        let mut functions: Vec<(String, &Node)> = Vec::new();
        for decl in members(decls).filter(|d| d.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let syntactic = parts.is_function();
            let (Some(name), Some(body)) = (parts.name, parts.body) else {
                continue;
            };
            // what a component's setup makes, it makes once per instance
            let setup = self
                .env
                .value(&self.env.entity(Namespace::Value, &name))
                .is_some_and(needs_setup);
            if setup {
                continue;
            }
            if syntactic {
                functions.push((name, body));
            } else if let Some(alias) = alias(body)
                && let Some(Ref::Value { module, name: of }) = self.typing.names.get(&alias.range())
                && let Some(maker) = self.maker(module, of)
            {
                // `let new_state = Atom.make`; an application, such as a
                // serializable family, gives what its maker made
                self.typing.makers.insert(name, maker);
            }
        }
        // a function calling one found in this pass is found in the next
        loop {
            let mut changed = false;
            for (name, body) in &functions {
                if self.typing.makers.contains_key(name) {
                    continue;
                }
                let mut found = Vec::new();
                self.made(body, &mut found);
                if let Some((.., maker)) = found.into_iter().next() {
                    self.typing.makers.insert(name.clone(), maker);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// The maker of the library that the value `module.name` is or calls.
    fn maker(&self, module: &str, name: &str) -> Option<String> {
        if module == self.env.module {
            return self.typing.makers.get(name).cloned();
        }
        let interface = self.env.interface(module)?;
        interface
            .makers
            .iter()
            .any(|n| n == name)
            .then(|| format!("{module}.{name}"))
    }

    /// The makers named under `node`, outside the tasks of handlers: where,
    /// the name as written, and the maker of the library it ends at.
    pub(super) fn made(&self, node: &Node, out: &mut Vec<(Range, String, String)>) {
        if node.kind == K::TaskExpr {
            return;
        }
        if let Some(Ref::Value { module, name }) = self.typing.names.get(&node.range())
            && let Some(maker) = self.maker(module, name)
        {
            let written = if *module == self.env.module {
                name.clone()
            } else {
                format!("{module}.{name}")
            };
            out.push((node.range(), written, maker));
        }
        for child in node.nodes() {
            self.made(child, out);
        }
    }
}

/// The one name a body is, inside parentheses.
fn alias(body: &Node) -> Option<&Node> {
    let mut statements = body.nodes();
    let (Some(mut node), None) = (statements.next(), statements.next()) else {
        return None;
    };
    while node.kind == K::ParenExpr {
        node = node.nodes().next()?;
    }
    matches!(node.kind, K::NameExpr | K::Path).then_some(node)
}

/// The makers among the exported values, for the interface.
pub fn exported_makers(makers: &HashMap<String, String>, exported: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = exported
        .iter()
        .filter(|n| makers.contains_key(**n))
        .map(|n| (*n).to_owned())
        .collect();
    names.sort();
    names
}

/// Whether a stage of `ty` needs the service `Setup` of a component.
fn needs_setup(ty: &Type) -> bool {
    let mut found = false;
    ty.visit(&mut |t| {
        if let Type::Fun {
            effects: Some(effects),
            ..
        } = t
        {
            effects.services.visit(&mut |s| {
                found |= matches!(s, Type::Row(row) if row.entries.iter().any(|(l, _)| l == "Prelude.Setup"));
            });
        }
    });
    found
}

/// The values `decls` mark `@maker`: the makers of the library.
fn marked(decls: &[&Node], text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = false;
    for decl in decls {
        if decl.kind == K::Attribute {
            let mut words = (decl.child_tokens()).filter(|t| t.kind == K::Ident);
            pending |= words.next().is_some_and(|w| w.text(text) == "maker");
            continue;
        }
        if mem::take(&mut pending)
            && let Some(name) = decl.child_tokens().find(|t| t.kind == K::Ident)
        {
            out.push(name.text(text).to_owned());
        }
    }
    out
}
