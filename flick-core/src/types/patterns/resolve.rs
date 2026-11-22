//! Resolving constructor names in patterns.

use crate::diagnostics::codes;
use crate::diagnostics::suggest::closest;
use crate::iface::{self, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::env::Found;

impl<'c> Checker<'c> {
    /// The name of the constructor a pattern names, and its entity: `None`
    /// after a diagnostic or when its module could not be checked.
    pub(in crate::types) fn pattern_constructor(
        &mut self,
        node: &Node,
    ) -> Option<(&'c str, Option<iface::Entity>)> {
        let text = self.text;
        let parts: Vec<&'c str> = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident))
            .map(|t| t.text(text))
            .collect();
        let (&name, qualifier) = parts.split_last()?;
        Some((name, self.constructor(&qualifier.join("."), name, node)))
    }

    /// The entity of a constructor; `None` after a diagnostic or when its
    /// module could not be checked.
    fn constructor(&mut self, qualifier: &str, name: &str, node: &Node) -> Option<iface::Entity> {
        if !qualifier.is_empty() {
            let found = self.env.qualified_module(qualifier).map(|(_, interface)| {
                interface.map(|i| {
                    i.find(Namespace::Constructor, name)
                        .map(|e| e.entity.clone())
                })
            });
            // a module that was checked but exports no such constructor: say so, or nothing records the pattern
            if let Some(Some(None)) = &found {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("`{qualifier}` does not export constructor `{name}`"),
                    |this| {
                        let (_, Some(interface)) = this.env.qualified_module(qualifier)? else {
                            return None;
                        };
                        let constructors = interface
                            .exports
                            .iter()
                            .filter(|e| e.entity.namespace == Namespace::Constructor);
                        closest(name, constructors.map(|e| e.entity.name.as_str()))
                            .map(|g| format!("{qualifier}.{g}"))
                    },
                );
                return None;
            }
            let found = found.map(Option::flatten);
            if found.is_none() {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("unknown module `{qualifier}`"),
                    |this| {
                        closest(qualifier, this.env.qualifier_names())
                            .map(|g| format!("{g}.{name}"))
                    },
                );
            }
            return found.flatten();
        }
        match self.env.find(Namespace::Constructor, name) {
            Found::Entity(entity) => Some(entity),
            Found::Unknown => None,
            Found::Missing => {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("cannot find constructor `{name}`"),
                    |this| closest(name, this.env.names(Namespace::Constructor)).map(str::to_owned),
                );
                None
            }
        }
    }
}
