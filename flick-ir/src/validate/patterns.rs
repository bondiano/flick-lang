//! Pattern, binder, and constructor pattern consistency checks.

use std::collections::{BTreeMap, BTreeSet};

use super::agree::unfold_opaque_alias;
use super::{Validator, bind};
use crate::{Constructor, IrType, Lang, Pat, TypeInterner, TypeRef};

impl Validator<'_> {
    pub(super) fn check_pat_binders(
        &mut self,
        pat: &Pat,
        binder_types: &[(String, TypeRef)],
        scrutinee: Option<&TypeRef>,
    ) {
        let module = self.module;
        let mut bound = Vec::new();
        bind(pat, &mut bound);
        let bound_set: BTreeSet<&str> = bound.iter().map(String::as_str).collect();
        let mut seen = BTreeSet::new();
        for (name, ty) in binder_types {
            self.check_type_wf(ty, None);
            if !seen.insert(name.as_str()) {
                self.problems.push(format!(
                    "`{module}` pattern repeats binder type for `{name}`"
                ));
            }
            if !bound_set.contains(name.as_str()) {
                self.problems.push(format!(
                    "`{module}` pattern carries binder type for `{name}`, which the pattern does not bind"
                ));
            }
        }
        let bmap: BTreeMap<&str, &TypeRef> =
            binder_types.iter().map(|(n, t)| (n.as_str(), t)).collect();
        self.check_pat_structure(pat, scrutinee, &bmap);
    }

    fn check_pat_structure(
        &mut self,
        pat: &Pat,
        expected: Option<&TypeRef>,
        bmap: &BTreeMap<&str, &TypeRef>,
    ) {
        match pat {
            Pat::Wild => {}
            Pat::Bind(name) => {
                if let (Some(exp), Some(&actual)) = (expected, bmap.get(name.as_str())) {
                    self.expect_agree(exp, actual, &format!("pattern binder `{name}`"));
                }
            }
            Pat::Lit(lit) => {
                if let Some(exp) = expected {
                    self.check_lit_type(lit, exp, "literal pattern");
                }
            }
            Pat::Tuple(items) => {
                if let Some(exp) = expected {
                    match &**exp {
                        IrType::Tuple(tys) if tys.len() == items.len() => {
                            for (p, t) in items.iter().zip(tys) {
                                self.check_pat_structure(p, Some(t), bmap);
                            }
                            return;
                        }
                        IrType::Tuple(tys) => {
                            let module = self.module;
                            self.problems.push(format!(
                                "`{module}` tuple pattern has {} elements, expected {}",
                                items.len(),
                                tys.len()
                            ));
                        }
                        IrType::Var(_) if self.gadt_depth > 0 => {}
                        _ => {
                            let module = self.module;
                            self.problems.push(format!(
                                "`{module}` tuple pattern matched against non-tuple `{exp}`"
                            ));
                        }
                    }
                }
                for p in items {
                    self.check_pat_structure(p, None, bmap);
                }
            }
            Pat::Con(c, items) => {
                self.check_con_use(c, None);
                if let Some(exp) = expected {
                    self.check_con_pat_type(c, exp);
                }
                if items.len() != c.arity + c.dicts {
                    let module = self.module;
                    self.problems.push(format!(
                        "`{module}` constructor pattern `{}` has {} subpatterns for arity {} + {} dicts",
                        c.name,
                        items.len(),
                        c.arity,
                        c.dicts
                    ));
                }
                for (i, p) in items.iter().take(c.arity).enumerate() {
                    self.check_pat_structure(p, c.fields.get(i), bmap);
                }
            }
            Pat::List(items, rest) => {
                let elem = expected.and_then(|e| {
                    self.types
                        .is_lang_named(e, Lang::List)
                        .and_then(|args| args.first())
                });
                for p in items {
                    self.check_pat_structure(p, elem, bmap);
                }
                if let Some(rest) = rest {
                    self.check_pat_structure(rest, expected, bmap);
                }
            }
            Pat::Record(fields) => {
                let rec_fields = expected.and_then(|e| match &**e {
                    IrType::Record { fields, .. } => Some(fields),
                    _ => None,
                });
                for (label, p) in fields {
                    let field_ty = rec_fields
                        .and_then(|fs| fs.iter().find(|(l, _)| l == label).map(|(_, t)| t));
                    self.check_pat_structure(p, field_ty, bmap);
                }
            }
            Pat::Or(alts) => {
                for alt in alts {
                    self.check_pat_structure(alt, expected, bmap);
                }
            }
        }
    }

    pub(super) fn pat_is_gadt(&self, pat: &Pat) -> bool {
        match pat {
            Pat::Con(c, items) => {
                let here = self
                    .types
                    .ctor_decls
                    .get(&(c.module.as_str(), c.ty.as_str(), c.name.as_str()))
                    .is_some_and(|(_, decl)| {
                        decl.result_args.is_some() || !decl.existentials.is_empty()
                    });
                here || items.iter().any(|p| self.pat_is_gadt(p))
            }
            Pat::Tuple(items) | Pat::Or(items) => items.iter().any(|p| self.pat_is_gadt(p)),
            Pat::List(items, rest) => {
                items.iter().any(|p| self.pat_is_gadt(p))
                    || rest.as_deref().is_some_and(|r| self.pat_is_gadt(r))
            }
            Pat::Record(fields) => fields.iter().any(|(_, p)| self.pat_is_gadt(p)),
            Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => false,
        }
    }

    fn check_con_pat_type(&mut self, c: &Constructor, expected: &IrType) {
        if let Some((tdecl, cdecl)) =
            self.types
                .ctor_decls
                .get(&(c.module.as_str(), c.ty.as_str(), c.name.as_str()))
        {
            if c.type_args.len() == cdecl.type_params.len()
                && (!c.type_args.is_empty() || cdecl.type_params.is_empty())
            {
                let mut interner = TypeInterner::new();
                let subst: Vec<(&str, &TypeRef)> = cdecl
                    .type_params
                    .iter()
                    .map(String::as_str)
                    .zip(c.type_args.iter())
                    .collect();
                let res_args: Vec<TypeRef> = match &cdecl.result_args {
                    Some(args) => args
                        .iter()
                        .map(|a| a.substitute(&subst, &mut interner))
                        .collect(),
                    None => tdecl
                        .params
                        .iter()
                        .map(|p| {
                            subst
                                .iter()
                                .find(|&&(k, _)| k == p)
                                .map_or_else(|| interner.var(p), |&(_, v)| v.clone())
                        })
                        .collect(),
                };
                let con_ty = interner.named(&c.module, &c.ty, res_args);
                self.expect_agree(
                    expected,
                    &con_ty,
                    &format!("constructor pattern `{}.{}`", c.module, c.name),
                );
            } else {
                self.check_con_head_agrees(c, expected, tdecl.error);
            }
            return;
        }
        if !c.type_args.is_empty()
            && matches!(
                expected,
                IrType::Named { module, name, args }
                    if module == &c.module && name == &c.ty && args.len() == c.type_args.len()
            )
        {
            let con_ty = IrType::Named {
                module: c.module.clone(),
                name: c.ty.clone(),
                args: c.type_args.clone(),
            };
            self.expect_agree(
                expected,
                &con_ty,
                &format!("constructor pattern `{}.{}`", c.module, c.name),
            );
            return;
        }
        self.check_con_head_agrees(c, expected, true);
    }

    fn check_con_head_agrees(&mut self, c: &Constructor, expected: &IrType, may_be_error: bool) {
        let mut owned: Option<TypeRef> = None;
        for _ in 0..=4 {
            let cur = owned.as_deref().unwrap_or(expected);
            let ok = match cur {
                IrType::Named { module, name, .. } if module == &c.module && name == &c.ty => true,
                IrType::Var(_) if self.gadt_depth > 0 => true,
                _ if self.types.is_lang_con(cur, Lang::Never) => true,
                _ if may_be_error && self.types.is_lang_con(cur, Lang::Errors) => true,
                _ => false,
            };
            if ok {
                return;
            }
            let Some(unfolded) = unfold_opaque_alias(self.types, cur) else {
                break;
            };
            owned = Some(unfolded);
        }
        let module = self.module;
        self.problems.push(format!(
            "`{module}` constructor pattern `{}.{}` does not match scrutinee type `{expected}`",
            c.module, c.name
        ));
    }
}
