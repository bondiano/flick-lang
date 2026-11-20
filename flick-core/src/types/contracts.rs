//! The contracts of functions, typed: each against its function's finished
//! scheme, in a context of its own. A constraint a contract needs is solved
//! there or reported there; it never enters the function's type, its
//! dictionaries or its ABI. Whether the contract holds is `flick prove`'s.

use std::mem;

use super::Checker;
use super::lang::Lang;
use super::ty::Type;
use super::typing::{Range, passes_dictionary};
use crate::iface::{Entity, Namespace};
use crate::logic;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// What the checker settled of one function's contract.
#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub name: String,
    /// Source range of the `LetDecl`.
    pub at: Range,
    /// Source range of the `Contract` node.
    pub contract: Range,
    /// True if the function's scheme has class constraints passing dictionaries.
    pub has_dicts: bool,
    /// True if any arrow of the function carries an effect row (`Sync`/`Runtime`).
    pub has_effects: bool,
    /// True if the function is syntactic `let rec` or inside `rec { ... }`.
    pub recursive: bool,
    /// Parameter types in order, lowered to `logic::Ty`.
    pub params: Vec<logic::Ty>,
    /// Result type after applying `params`, lowered to `logic::Ty`.
    pub result: logic::Ty,
    /// Data types referenced in signature, contract, and body.
    pub data: logic::Data,
}

impl Checker<'_> {
    pub(super) fn check_contracts(&mut self, decls: &[&Node]) {
        for decl in decls {
            match decl.kind {
                K::LetDecl => {
                    if let Some(contract) = decl.nodes().find(|n| n.kind == K::Contract) {
                        self.check_contract(decl, contract, false);
                    }
                }
                K::RecGroup => {
                    for member in decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .filter(|n| n.kind == K::LetDecl)
                    {
                        if let Some(contract) = member.nodes().find(|n| n.kind == K::Contract) {
                            self.check_contract(member, contract, true);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_contract(&mut self, decl: &Node, contract: &Node, in_rec_group: bool) {
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
        // Leave `self.cx.proof = None` so proof rules (`refl`, `symm`, ...) and
        // imported proof lemmas cannot resolve inside contract expressions.
        let (mut ty, _, givens) = self.skolemize(&scheme);
        let has_dicts = givens.iter().any(passes_dictionary);
        self.classes.givens.extend(givens);
        // the parameters at the types of the scheme, in order
        let mut param_types = Vec::new();
        let mut has_effects = false;
        for param in decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
        {
            let Type::Fun {
                param: p,
                effects,
                result,
            } = self.whnf(&ty)
            else {
                break;
            };
            if effects.is_some() {
                has_effects = true;
            }
            self.bind_pattern(param, &p);
            param_types.push(*p);
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
        let mut data = logic::Data::new();
        let params = param_types
            .iter()
            .map(|p| self.contract_logic_type(p, &mut data))
            .collect();
        let result = self.contract_logic_type(&ty, &mut data);
        self.contract_constructors_named(decl, &mut data);
        let recursive = in_rec_group || decl.has_token(K::RecKw);
        self.typing.contracts.push(ContractInfo {
            name: entity.name,
            at: decl.range(),
            contract: contract.range(),
            has_dicts,
            has_effects,
            recursive,
            params,
            result,
            data,
        });
        self.cx = outer;
        (self.store.level, self.classes.wanted, self.classes.givens) = saved;
    }
}
