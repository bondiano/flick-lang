//! The contracts of functions, typed: each against its function's finished
//! scheme, in a context of its own. A constraint a contract needs is solved
//! there or reported there; it never enters the function's type, its
//! dictionaries or its ABI. Whether the contract holds is `flick prove`'s.

use std::mem;

use super::Checker;
use super::lang::Lang;
use super::ty::Type;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    pub(super) fn check_contracts(&mut self, decls: &[&Node]) {
        for decl in decls.iter().filter(|d| d.kind == K::LetDecl) {
            if let Some(contract) = decl.nodes().find(|n| n.kind == K::Contract) {
                self.check_contract(decl, contract);
            }
        }
    }

    fn check_contract(&mut self, decl: &Node, contract: &Node) {
        let Some(name) = decl.nodes().find(|n| n.kind == K::VarPat) else {
            return;
        };
        let entity = Entity {
            module: self.env.module.clone(),
            namespace: Namespace::Value,
            name: name.text(self.text).trim().to_owned(),
        };
        let Some(scheme) = self.env.value(&entity).cloned() else {
            return;
        };
        let outer = mem::take(&mut self.cx);
        let saved = (
            mem::replace(&mut self.store.level, 1),
            mem::take(&mut self.classes.wanted),
            mem::take(&mut self.classes.givens),
        );
        self.cx.proof = Some(decl.range());
        let (mut ty, _, givens) = self.skolemize(&scheme);
        self.classes.givens.extend(givens);
        // the parameters at the types of the scheme, in order
        for param in decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
        {
            let Type::Fun {
                param: p, result, ..
            } = bare(self.store.resolve(&ty))
            else {
                break;
            };
            self.bind_pattern(param, &p);
            ty = *result;
        }
        let bool_ = Type::lang(Lang::Bool);
        for entry in contract.nodes() {
            let Some(e) = entry.nodes().next() else {
                continue;
            };
            match entry.kind {
                K::Requires => self.check(e, &bool_),
                K::Ensures => self.check(e, &Type::pure_fun(ty.clone(), bool_.clone())),
                _ => {}
            }
        }
        self.settle_operators();
        self.solve_classes(true);
        self.solve_rows();
        self.cx = outer;
        (self.store.level, self.classes.wanted, self.classes.givens) = saved;
    }
}

/// `t` without the captures and bounds around it.
fn bare(mut t: Type) -> Type {
    while let Type::Bounded { ty, .. } | Type::Captured { ty, .. } = t {
        t = *ty;
    }
    t
}
