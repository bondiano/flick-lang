//! First-class packed modules (`Module Sig`) and existential module packages
//! (`exists K : Sig. T`), following the module boundary contract of
//! `docs/formal/modules.md` and `formal/Formal/Modules.lean`.
//!
//! `pack M as Sig` lowers to the record of `Sig`'s value fields, and `unpack`
//! binds it locally and projects its fields (and, when a witness escapes,
//! packs `{ $module, $value }` into `exists K : Sig. T`). Keeping `Module Sig`
//! and `exists K : Sig. T` nominal preserves signature identity and per-opening
//! abstract witness separation while checking the lowered records against `Sig`.

mod rewrite;

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use self::rewrite::{
    ExistsRewriter, abstract_member_map, close_opened_containers, find_witness, has_naked_witness,
    is_projected, substitute_contract_scheme,
};
use super::dicts::arg;
use super::generalize::{Inst, substitute};
use super::{Check, Checker, TypeError, error};
use crate::decls::ModuleContract;
use crate::syntax::{Name, Path, Term};
use crate::types::{Fields, MANIFEST_PREFIX, Scheme, Type, exists_module, witness_type};

impl Checker<'_> {
    pub(super) fn fresh_witness(&mut self) -> u32 {
        self.rigid += 1;
        self.rigid
    }

    pub(super) fn contract(&self, sig: &Path) -> Check<&ModuleContract> {
        self.decls
            .module_contracts
            .get(sig)
            .ok_or_else(|| TypeError {
                rule: "Module",
                message: format!("unknown module signature `{sig}`"),
            })
    }

    pub(super) fn packable_contract(&self, sig: &Path) -> Check<&ModuleContract> {
        let contract = self.contract(sig)?;
        if !contract.packable {
            return error(
                "Module",
                format!("module signature `{}` cannot be packed", sig.name),
            );
        }
        Ok(contract)
    }

    /// `sig`'s value fields as a closed record type with `sig`'s abstract types
    /// instantiated to `args`.
    pub(super) fn contract_fields_at(&self, sig: &Path, args: &[Type]) -> Check<Fields> {
        let contract = self.packable_contract(sig)?;
        let abstract_map = abstract_member_map(contract, args);
        let fields: BTreeMap<Name, Type> = contract
            .fields
            .iter()
            .map(|(name, scheme)| {
                let s = substitute_contract_scheme(scheme, &abstract_map);
                let s = self.live_scheme(&s);
                let ty = if s.vars.is_empty() && s.outlives.is_empty() {
                    s.body
                } else {
                    Type::Forall(Rc::new(s))
                };
                (name.clone(), ty)
            })
            .collect();
        Ok(Fields { fields, tail: None })
    }

    /// Field `label` of `Module sig`, at `witness` (or a fresh witness if
    /// projected directly without an enclosing `unpack` binding).
    pub(super) fn contract_field(
        &mut self,
        sig: &Path,
        witness: Option<u32>,
        label: &Name,
    ) -> Check<Type> {
        let contract = self.packable_contract(sig)?.clone();
        let Some((_, scheme)) = contract.fields.iter().find(|(n, _)| n == label) else {
            return error(
                "Field",
                format!("`Module {}` has no field `{label}`", sig.name),
            );
        };
        let args: Vec<Type> = if contract.abstract_types.is_empty() {
            Vec::new()
        } else {
            let w = witness.unwrap_or_else(|| self.fresh_witness());
            contract
                .abstract_types
                .iter()
                .map(|(name, _)| witness_type(w, name))
                .collect()
        };
        let abstract_map = abstract_member_map(&contract, &args);
        let scheme = substitute_contract_scheme(scheme, &abstract_map);
        let scheme = self.live_scheme(&scheme);
        Ok(self.instantiate(&scheme))
    }

    /// Opens `body` of `exists binder : sig. body` with `sig`'s abstract types
    /// replaced by `args`, expanding manifest aliases and manifest nominals.
    pub(super) fn open_exists_body(
        &self,
        binder: &Name,
        sig: &Path,
        body: &Type,
        args: &[Type],
    ) -> Check<Type> {
        let contract = self.contract(sig)?;
        let abstract_map = abstract_member_map(contract, args);
        let prefix = exists_module(binder);
        let rewriter = ExistsRewriter {
            binder,
            prefix: &prefix,
            sig,
            contract,
            abstract_map: &abstract_map,
        };
        Ok(rewriter.rewrite(body, 32))
    }

    /// Checking a packed record `{ f₁ = e₁, ... }` against `Module sig`.
    pub(super) fn check_pack_module(&mut self, fields: &[(Name, Term)], sig: &Path) -> Check<()> {
        let contract = self.packable_contract(sig)?.clone();
        let metas: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|_| self.fresh())
            .collect();
        let expected = self.contract_fields_at(sig, &metas)?;
        self.record_against(fields, &expected)
    }

    /// Checking `{ $module = m, $value = v }` against `exists binder : sig. body`.
    pub(super) fn check_pack_exists(
        &mut self,
        fields: &[(Name, Term)],
        binder: &Name,
        sig: &Path,
        body: &Type,
    ) -> Check<()> {
        let contract = self.packable_contract(sig)?.clone();
        let mod_term = fields
            .iter()
            .find(|(l, _)| &**l == "$module")
            .map(|(_, t)| t);
        let val_term = fields
            .iter()
            .find(|(l, _)| &**l == "$value")
            .map(|(_, t)| t);
        let (Some(mod_term), Some(val_term)) = (mod_term, val_term) else {
            return error(
                "Rec",
                format!(
                    "an existential package of `{}` needs `$module` and `$value`",
                    sig.name
                ),
            );
        };
        if fields.len() != 2 {
            return error(
                "Rec",
                format!(
                    "an existential package of `{}` has only `$module` and `$value`",
                    sig.name
                ),
            );
        }
        let metas: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|_| self.fresh())
            .collect();
        self.check_existential_module_term(mod_term, sig, &contract, &metas)?;
        let expected_val = self.open_exists_body(binder, sig, body, &metas)?;
        self.check(val_term, &expected_val)
    }

    fn check_existential_module_term(
        &mut self,
        mod_term: &Term,
        sig: &Path,
        contract: &ModuleContract,
        metas: &[Type],
    ) -> Check<()> {
        if let Term::Record(mod_fields) = mod_term {
            let expected = self.contract_fields_at(sig, metas)?;
            return self.record_against(mod_fields, &expected);
        }
        let mod_ty = self.infer(mod_term)?;
        self.match_existential_module_type(&mod_ty, sig, contract, metas)
    }

    fn match_existential_module_type(
        &mut self,
        mod_ty: &Type,
        sig: &Path,
        contract: &ModuleContract,
        metas: &[Type],
    ) -> Check<()> {
        match self.resolve(mod_ty) {
            Type::OpenedModule(sig2, w) => {
                if sig2 != *sig {
                    return self.mismatch(mod_ty, &Type::Module(sig.clone()));
                }
                for (meta, (name, _)) in metas.iter().zip(&contract.abstract_types) {
                    self.unify(meta, &witness_type(w, name))?;
                }
                Ok(())
            }
            Type::Module(sig2) => {
                if sig2 != *sig {
                    return self.mismatch(mod_ty, &Type::Module(sig.clone()));
                }
                Ok(())
            }
            Type::Record(_) => {
                let expected = Type::Record(self.contract_fields_at(sig, metas)?);
                self.subsume(mod_ty, &expected)
            }
            _ => self.mismatch(mod_ty, &Type::Module(sig.clone())),
        }
    }

    /// Subsumption of a plain record (an unannotated `pack M as Sig` value)
    /// into `Module sig`.
    pub(super) fn subsume_record_to_module(&mut self, record: &Type, sig: &Path) -> Check<()> {
        let contract = self.packable_contract(sig)?.clone();
        let metas: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|_| self.fresh())
            .collect();
        let expected = Type::Record(self.contract_fields_at(sig, &metas)?);
        self.subsume(record, &expected)
    }

    /// Subsumption of a lowered `{ $module, $value }` record into `exists binder : sig. body`.
    pub(super) fn subsume_record_to_exists(
        &mut self,
        record: &Fields,
        binder: &Name,
        sig: &Path,
        body: &Type,
    ) -> Check<()> {
        let contract = self.packable_contract(sig)?.clone();
        if record.fields.len() != 2
            || !record.fields.contains_key("$module")
            || !record.fields.contains_key("$value")
        {
            return self.mismatch(
                &Type::Record(record.clone()),
                &Type::Exists(binder.clone(), sig.clone(), Rc::new(body.clone())),
            );
        }
        let metas: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|_| self.fresh())
            .collect();
        self.match_existential_module_type(&record.fields["$module"], sig, &contract, &metas)?;
        let expected_val = self.open_exists_body(binder, sig, body, &metas)?;
        self.subsume(&record.fields["$value"], &expected_val)
    }

    /// Unification for `Module`, `OpenedModule`, `Exists`, and `OpenedExists`.
    pub(super) fn unify_module(&mut self, a: &Type, b: &Type) -> Check<()> {
        match (a, b) {
            (Type::Module(s1), Type::Module(s2)) if s1 == s2 => Ok(()),
            (Type::OpenedModule(s1, w1), Type::OpenedModule(s2, w2)) if s1 == s2 && w1 == w2 => {
                Ok(())
            }
            (Type::Exists(k1, s1, b1), Type::Exists(k2, s2, b2)) => {
                self.compare_exists((k1, s1, b1), (k2, s2, b2), false)
            }
            (Type::OpenedExists(s1, w1, b1), Type::OpenedExists(s2, w2, b2))
                if s1 == s2 && w1 == w2 =>
            {
                self.unify(b1, b2)
            }
            _ => self.mismatch(a, b),
        }
    }

    /// `exists k1 : s1. b1 ≤ exists k2 : s2. b2`.
    pub(super) fn subsume_exists(
        &mut self,
        left: (&Name, &Path, &Type),
        right: (&Name, &Path, &Type),
    ) -> Check<()> {
        self.compare_exists(left, right, true)
    }

    fn compare_exists(
        &mut self,
        (k1, s1, b1): (&Name, &Path, &Type),
        (k2, s2, b2): (&Name, &Path, &Type),
        sub: bool,
    ) -> Check<()> {
        if s1 != s2 {
            return self.mismatch(
                &Type::Exists(k1.clone(), s1.clone(), Rc::new(b1.clone())),
                &Type::Exists(k2.clone(), s2.clone(), Rc::new(b2.clone())),
            );
        }
        let contract = self.contract(s1)?.clone();
        let w = self.fresh_witness();
        self.level += 1;
        self.witness_levels.insert(w, self.level);
        let args: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|(n, _)| witness_type(w, n))
            .collect();
        let o1 = self.open_exists_body(k1, s1, b1, &args);
        let o2 = self.open_exists_body(k2, s2, b2, &args);
        let res = o1.and_then(|o1| {
            o2.and_then(|o2| {
                if sub {
                    self.subsume(&o1, &o2)
                } else {
                    self.unify(&o1, &o2)
                }
            })
        });
        self.level -= 1;
        res
    }

    /// Re-packing an opened existential back into `exists k2 : s2. b2`.
    pub(super) fn subsume_opened_exists(
        &mut self,
        s1: &Path,
        w: u32,
        opened_body: &Type,
        k2: &Name,
        s2: &Path,
        b2: &Type,
    ) -> Check<()> {
        if s1 != s2 {
            return self.mismatch(
                &Type::OpenedExists(s1.clone(), w, Rc::new(opened_body.clone())),
                &Type::Exists(k2.clone(), s2.clone(), Rc::new(b2.clone())),
            );
        }
        let contract = self.contract(s1)?.clone();
        let args: Vec<Type> = contract
            .abstract_types
            .iter()
            .map(|(n, _)| witness_type(w, n))
            .collect();
        let o2 = self.open_exists_body(k2, s2, b2, &args)?;
        self.subsume(opened_body, &o2)
    }

    /// If `let x = e1 in e2` opens a packed module or existential (because `e2`
    /// projects fields from `x`), mint a fresh witness for `x`'s scope.
    pub(super) fn open_let_binding(
        &mut self,
        x: &Name,
        s: Scheme,
        e2: &Term,
    ) -> Check<(Scheme, Option<(u32, Path)>)> {
        if !s.vars.is_empty() || !is_projected(x, e2) {
            return Ok((s, None));
        }
        match self.resolve(&s.body) {
            Type::Module(sig) => {
                let contract = self.contract(&sig)?.clone();
                if contract.abstract_types.is_empty() {
                    return Ok((s, None));
                }
                let w = self.fresh_witness();
                let opened = self.rewrap_qualifiers(&s.body, Type::OpenedModule(sig.clone(), w));
                Ok((Scheme { body: opened, ..s }, Some((w, sig))))
            }
            Type::Exists(binder, sig, body) => {
                let contract = self.contract(&sig)?.clone();
                let w = self.fresh_witness();
                let args: Vec<Type> = contract
                    .abstract_types
                    .iter()
                    .map(|(n, _)| witness_type(w, n))
                    .collect();
                let opened_body = self.open_exists_body(&binder, &sig, &body, &args)?;
                let opened = self.rewrap_qualifiers(
                    &s.body,
                    Type::OpenedExists(sig.clone(), w, Rc::new(opened_body)),
                );
                Ok((Scheme { body: opened, ..s }, Some((w, sig))))
            }
            _ => Ok((s, None)),
        }
    }

    /// Re-closes `OpenedModule(sig, w)` / `OpenedExists(sig, w, body)` inside `t`
    /// back into their closed existential forms when `w` does not leak elsewhere in `t`.
    pub(super) fn close_witness(t: &Type, w: u32) -> Type {
        if has_naked_witness(t, w) {
            return t.clone();
        }
        close_opened_containers(t, w)
    }

    /// Whether `t` mentions witness `w` anywhere (after zonking).
    pub(super) fn mentions_witness(&self, t: &Type, w: u32) -> bool {
        let mut found = None;
        find_witness(&self.zonk(t), &|id| id == w, &mut found);
        found.is_some()
    }

    /// Any witness inside `t` whose scope is deeper than `level`.
    pub(super) fn escaping_witness(&self, t: &Type, level: u32) -> Option<u32> {
        if self.witness_levels.is_empty() {
            return None;
        }
        let mut found = None;
        find_witness(
            &self.zonk(t),
            &|id| self.witness_levels.get(&id).is_some_and(|&l| l > level),
            &mut found,
        );
        found
    }

    /// Unfolds a partially applied manifest alias `$manifest:Mod.Sig.Name` once saturated.
    pub(super) fn unfold_manifest(&self, t: &Type) -> Option<Type> {
        let mut args = Vec::new();
        let mut head = t;
        while let Type::App(h, a) = head {
            args.push((**a).clone());
            head = h;
        }
        let Type::Con(p) = head else {
            return None;
        };
        let sig_str = p.module.strip_prefix(MANIFEST_PREFIX)?;
        let sig = Path::parse(sig_str);
        let contract = self.decls.module_contracts.get(&sig)?;
        let (params, body) = contract.manifest_types.get(&p.name)?;
        if args.len() < params.len() {
            return None;
        }
        args.reverse();
        let map: HashMap<Name, Inst> = params
            .iter()
            .cloned()
            .zip(args[..params.len()].iter().map(arg))
            .collect();
        let mut out = substitute(body, &map);
        for extra in &args[params.len()..] {
            out = Type::app(out, extra.clone());
        }
        Some(out)
    }
}
