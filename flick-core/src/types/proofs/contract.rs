//! The invariants a module's blocks declare, and whether each holds
//! together: an `unboxed` type of one constructor with `$` and one field,
//! its check `Base -> Bool`, and its value exactly the field. The expansion
//! has seen to it that the attribute, the type and the companions are the
//! code of one block (`plugin/expansion/private.rs`). A contract that does
//! not hold gives nothing: no `T.prove`, and the constructor stays as it is.

use super::formula::word;
use super::{Checker, Contract};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::heads::INVARIANT_OF;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::bindings::{let_parts, signature_name};
use crate::types::env::{Abi, Form};
use crate::types::lang::Lang;
use crate::types::sugar_lints::unparen;
use crate::types::ty::Type;

/// The bases whose values the logic reads.
const BASES: [Lang; 4] = [Lang::Int, Lang::Float, Lang::String, Lang::Bool];

impl<'a> Checker<'a> {
    /// The invariants of the module's types, before any value is checked:
    /// `T.prove` is known wherever it is written.
    pub(in crate::types) fn declare_invariants(&mut self, decls: &[&'a Node]) {
        for (i, decl) in decls.iter().enumerate() {
            let words: Vec<&str> = (decl.child_tokens())
                .filter(|t| t.kind == K::Ident)
                .map(|t| t.text(self.text))
                .collect();
            let [word, check, value, prove] = words[..] else {
                continue;
            };
            if decl.kind != K::Attribute || !(INVARIANT_OF.ends_with(word) && word.contains('$')) {
                continue;
            }
            let at = self.span(decl);
            let ty = (decls.get(i + 1))
                .filter(|d| d.kind == K::TypeDecl)
                .and_then(|d| d.child_tokens().find(|t| t.kind == K::Con))
                .map(|t| t.text(self.text).to_owned());
            match ty.and_then(|ty| self.invariant_contract(decls, &ty, [check, value, prove])) {
                Some(mut contract) => {
                    contract.at = at;
                    self.proofs.contracts.push(contract);
                }
                None => self.error(
                    codes::P049,
                    at,
                    "the invariant a plugin gave does not hold together: it stands before an `unboxed` type of one constructor with one field, with a `let` of its check and of its value",
                ),
            }
        }
        self.imported_invariants();
    }

    /// The invariant of `ty` with the companions `members` names, when the
    /// type and the companions are as an invariant needs them.
    fn invariant_contract(
        &self,
        decls: &[&'a Node],
        ty: &str,
        [check, value, prove]: [&str; 3],
    ) -> Option<Contract<'a>> {
        let module = self.env.module.clone();
        let Form::Adt { constructors, abi } = &self.env.type_def(&module, ty)?.form else {
            return None;
        };
        let [constructor] = &constructors[..] else {
            return None;
        };
        if !constructor.contains('$') || !matches!(abi, Abi::Unboxed) {
            return None;
        }
        let companion = |member: &str| format!("{ty}${member}");
        let named = |name: &str| {
            (decls.iter()).find(|d| {
                d.kind == K::LetDecl && let_parts(d, self.text).name.as_deref() == Some(name)
            })
        };
        let check_decl = named(&companion(check))?;
        let value_decl = named(&companion(value))?;
        let proved = companion(prove);
        let declares_prove = named(&proved).is_some()
            || (decls.iter()).any(|d| {
                d.kind == K::Signature && signature_name(d, self.text) == Some(proved.as_str())
            });
        if declares_prove || !self.projects_field(value_decl, constructor) {
            return None;
        }
        let entity = Entity {
            module: module.clone(),
            namespace: Namespace::Constructor,
            name: constructor.clone(),
        };
        let active = match self.env.value(&entity).map(|s| self.whnf(s)) {
            Some(Type::Fun { param, .. }) => {
                let base = self.whnf(&param);
                BASES.iter().any(|b| base.is_lang(*b))
            }
            _ => false,
        };
        Some(Contract {
            module,
            ty: ty.to_owned(),
            constructor: constructor.clone(),
            check: companion(check),
            value: companion(value),
            prove: prove.to_owned(),
            active,
            at: self.span(check_decl),
            body: let_parts(check_decl, self.text).body,
            summary: None,
            base: None,
        })
    }

    /// Whether `decl` is `let T$value (C v) = v` of the constructor `C`.
    fn projects_field(&self, decl: &Node, constructor: &str) -> bool {
        let parts = let_parts(decl, self.text);
        let ([param], Some(body)) = (&parts.params[..], parts.body) else {
            return false;
        };
        let mut param = *param;
        while param.kind == K::ParenPat {
            let Some(inner) = param.nodes().next() else {
                return false;
            };
            param = inner;
        }
        let named = param.child_tokens().find(|t| t.kind == K::Con);
        let fields: Vec<&Node> = param.nodes().collect();
        let (true, [field]) = (param.kind == K::ConPat, &fields[..]) else {
            return false;
        };
        let bound = word(field, self.text);
        let mut statements = body.nodes();
        let result = match (statements.next(), statements.next()) {
            (Some(only), None) if unparen(only).kind == K::NameExpr => {
                word(unparen(only), self.text)
            }
            _ => return false,
        };
        named.is_some_and(|c| c.text(self.text) == constructor)
            && field.kind == K::VarPat
            && bound.is_some()
            && result == bound
    }

    /// `FLICK-P049` on each invariant whose check is not `Base -> Bool`, a
    /// pure function of the field's type; the contract then gives nothing.
    /// One the compiler does not prove gives nothing anyway.
    pub(super) fn validate_contracts(&mut self) {
        for i in 0..self.proofs.contracts.len() {
            let contract = &self.proofs.contracts[i];
            // an imported one was validated against its interface
            if !contract.active || contract.body.is_none() {
                continue;
            }
            let constructor = Entity {
                module: contract.module.clone(),
                namespace: Namespace::Constructor,
                name: contract.constructor.clone(),
            };
            let check = Entity {
                namespace: Namespace::Value,
                name: contract.check.clone(),
                ..constructor.clone()
            };
            let base = match self.env.value(&constructor).map(|s| self.whnf(s)) {
                Some(Type::Fun { param, .. }) => Some(self.zonk(&param)),
                _ => None,
            };
            let holds = match (base, self.env.value(&check).map(|s| self.whnf(s))) {
                (
                    Some(base),
                    Some(Type::Fun {
                        param,
                        effects: None,
                        result,
                    }),
                ) => self.zonk(&param) == base && self.whnf(&result).is_lang(Lang::Bool),
                _ => false,
            };
            if !holds {
                let message = format!(
                    "the invariant a plugin gave does not hold together: the check `{}` is not a pure function from the field of `{}` to `Bool`",
                    contract.check.replace('$', "."),
                    contract.ty
                );
                let at = contract.at;
                self.proofs.contracts[i].active = false;
                self.error(codes::P049, at, message);
            }
        }
    }
}
