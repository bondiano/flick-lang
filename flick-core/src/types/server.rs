//! What reaches `Server`: every top-level value whose type needs it, and
//! every one that names such a value, in this module or in one it imports,
//! whose interface lists it. A client entry must reach none (`FLICK-E009`).

use super::ty::Type;
use super::typing::Ref;
use super::ui::members;
use super::{Checker, let_parts};
use crate::iface::{self, Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// The top-level values of this module that reach `Server`, each with
    /// the names it reaches it through: local ones, then the imported one
    /// whose interface goes on, or none when its own type needs `Server`.
    pub fn servers(&mut self, decls: &[&Node]) {
        let mut values: Vec<(String, &Node)> = Vec::new();
        for decl in members(decls).filter(|d| d.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let (Some(name), Some(body)) = (parts.name, parts.body) else {
                continue;
            };
            let own = self.env.entity(Namespace::Value, &name);
            if self.env.value(&own).is_some_and(needs_server) {
                self.typing.servers.insert(name, Vec::new());
            } else {
                values.push((name, body));
            }
        }
        // a value naming one found in this pass is found in the next
        loop {
            let mut changed = false;
            for (name, body) in &values {
                if self.typing.servers.contains_key(name) {
                    continue;
                }
                if let Some(path) = self.reached(body) {
                    self.typing.servers.insert(name.clone(), path);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// The path to `Server` of the first value under `node` that reaches it.
    // shortcut: only names are followed, so a value that reaches `Server` only
    // as an argument or through a class dictionary gives no path; follow
    // dictionaries and higher-order arguments when a program needs that.
    fn reached(&self, node: &Node) -> Option<Vec<String>> {
        // down a chain of operators in a loop: its nodes, then its operands
        let (spine, left) = node.left_spine();
        if let Some(found) = spine.iter().find_map(|n| self.reached_at(n)) {
            return Some(found);
        }
        self.reached_at(left)
            .or_else(|| left.nodes().find_map(|child| self.reached(child)))
            .or_else(|| {
                (spine.iter().rev().flat_map(|n| n.nodes().skip(1)))
                    .find_map(|child| self.reached(child))
            })
    }

    /// The server `node` itself names, if any.
    fn reached_at(&self, node: &Node) -> Option<Vec<String>> {
        if let Some(Ref::Value { module, name }) = self.typing.names.get(&node.range()) {
            if *module == self.env.module {
                if let Some(rest) = self.typing.servers.get(name) {
                    return Some([vec![name.clone()], rest.clone()].concat());
                }
            } else if self.imported_server(module, name) {
                return Some(vec![format!("{module}.{name}")]);
            }
        }
        None
    }

    fn imported_server(&self, module: &str, name: &str) -> bool {
        let listed = self
            .env
            .interface(module)
            .is_some_and(|i| i.tagged(iface::SERVER, name).is_some());
        let entity = Entity {
            module: module.to_owned(),
            namespace: Namespace::Value,
            name: name.to_owned(),
        };
        listed || self.env.value(&entity).is_some_and(needs_server)
    }
}

/// Whether `ty` mentions `Server` in a row: a function or an effect that
/// needs it, or a layer that provides it.
fn needs_server(ty: &Type) -> bool {
    let Some(server) = lang::label(Lang::Server) else {
        return false;
    };
    let mut found = false;
    ty.visit(&mut |t| {
        found |= matches!(t, Type::Row(row) if row.entries.iter().any(|(l, _)| l == server));
    });
    found
}
