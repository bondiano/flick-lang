//! What makes an atom, an action or a view: the makers of the library, and
//! every function that calls one outside the task of a handler, in this
//! module or in one it imports, whose interface lists them. A render must
//! call none (`FLICK-U006`).

use std::collections::HashMap;

use super::typing::Ref;
use super::ui::members;
use super::{Checker, Range, let_parts};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The makers of the library, by module and names.
const MAKERS: [(&str, &[&str]); 3] = [
    (
        "Atom",
        &[
            "make",
            "derived",
            "task",
            "serializable",
            "serializable_family",
            "serializable_task",
            "on_server",
            "client_only",
            "search_param",
            "family",
        ],
    ),
    ("Action", &["exhaust", "latest", "queue", "concurrent"]),
    ("Ui", &["view", "suspended", "named"]),
];

impl Checker<'_> {
    /// The top-level functions of this module that make an atom, an action
    /// or a view, each with the maker of the library it ends at.
    pub fn makers(&mut self, decls: &[&Node]) {
        let mut functions: Vec<(String, &Node)> = Vec::new();
        for decl in members(decls).filter(|d| d.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let syntactic = parts.is_function();
            let (Some(name), Some(body)) = (parts.name, parts.body) else {
                continue;
            };
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
        if MAKERS
            .iter()
            .any(|(m, names)| *m == module && names.contains(&name))
        {
            return Some(format!("{module}.{name}"));
        }
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
