//! Pattern lowering and binder representation tracking.

use std::collections::BTreeMap;

use flick_ir::{IrType, Pat, TypeRef};

use super::super::SpecializeError;
use super::super::ir::SpecPat;
use super::super::type_util::{is_concrete_layout_type, subst_constructor, subst_type};
use super::super::types::ValueRep;
use super::{LowerCtx, LowerMode};

type BoundPattern = (
    SpecPat,
    Vec<(String, ValueRep)>,
    Vec<(String, Option<ValueRep>)>,
);

impl LowerCtx<'_, '_> {
    pub(super) fn lower_pat_and_bind(
        &mut self,
        pat: &Pat,
        binder_types: &[(String, TypeRef)],
        scrut_ty: Option<&TypeRef>,
    ) -> Result<BoundPattern, SpecializeError> {
        let mut type_map: BTreeMap<String, TypeRef> = BTreeMap::new();
        for (name, ty) in binder_types {
            let sub = subst_type(ty, &self.type_subst, &mut self.spec.interner);
            type_map.insert(name.clone(), sub);
        }
        let mut boxed_binders = BTreeMap::new();
        let spec_pat = self.lower_pat(pat, scrut_ty, &mut boxed_binders)?;
        let mut names = Vec::new();
        pat.binders(&mut names);
        let mut binder_reps = Vec::with_capacity(names.len());
        let mut saved = Vec::with_capacity(names.len());
        for name in names {
            let forced_box = boxed_binders.get(name).copied().unwrap_or(false);
            let rep = if forced_box || self.mode == LowerMode::Fallback {
                ValueRep::Boxed
            } else if let Some(t) = type_map.get(name) {
                self.rep_for_ty(Some(t))
            } else if matches!(pat, Pat::Bind(_)) {
                self.rep_for_ty(scrut_ty)
            } else {
                ValueRep::Boxed
            };
            saved.push((
                name.to_owned(),
                self.local_reps.insert(name.to_owned(), rep.clone()),
            ));
            binder_reps.push((name.to_owned(), rep));
        }
        Ok((spec_pat, binder_reps, saved))
    }

    #[allow(clippy::too_many_lines)]
    fn lower_pat(
        &mut self,
        pat: &Pat,
        scrut_ty: Option<&TypeRef>,
        boxed_binders: &mut BTreeMap<String, bool>,
    ) -> Result<SpecPat, SpecializeError> {
        match pat {
            Pat::Wild => Ok(SpecPat::Wild),
            Pat::Bind(n) => Ok(SpecPat::Bind(n.clone())),
            Pat::Lit(l) => Ok(SpecPat::Lit(l.clone())),
            Pat::Tuple(items) => {
                let item_tys = match scrut_ty.map(|t| &**t) {
                    Some(IrType::Tuple(ts)) => Some(ts.as_slice()),
                    _ => None,
                };
                let mut out = Vec::with_capacity(items.len());
                for (i, p) in items.iter().enumerate() {
                    out.push(self.lower_pat(
                        p,
                        item_tys.and_then(|ts| ts.get(i)),
                        boxed_binders,
                    )?);
                }
                Ok(SpecPat::Tuple(out))
            }
            Pat::List(items, rest) => {
                let elem_ty = match scrut_ty.map(|t| &**t) {
                    Some(IrType::Named { args, .. }) if args.len() == 1 => Some(&args[0]),
                    _ => None,
                };
                let mut out = Vec::with_capacity(items.len());
                for p in items {
                    out.push(self.lower_pat(p, elem_ty, boxed_binders)?);
                }
                let rest = match rest {
                    Some(r) => Some(Box::new(self.lower_pat(r, scrut_ty, boxed_binders)?)),
                    None => None,
                };
                Ok(SpecPat::List(out, rest))
            }
            Pat::Record(fields) => {
                let is_closed = self.mode == LowerMode::Specialized
                    && scrut_ty.is_some_and(|t| {
                        matches!(&**t, IrType::Record { tail: None, .. })
                            && is_concrete_layout_type(t, &self.spec.index)
                    });
                let mut out = Vec::with_capacity(fields.len());
                for (label, p) in fields {
                    let loc = if is_closed {
                        scrut_ty
                            .and_then(|t| self.spec.record_field_offset(t, label).ok().flatten())
                            .map(|(idx, off, _)| (idx, off))
                    } else {
                        None
                    };
                    let field_ty = scrut_ty.and_then(|t| match &**t {
                        IrType::Record { fields: rfs, .. } => {
                            rfs.iter().find(|(l, _)| l == label).map(|(_, ft)| ft)
                        }
                        _ => None,
                    });
                    out.push((
                        label.clone(),
                        loc,
                        self.lower_pat(p, field_ty, boxed_binders)?,
                    ));
                }
                Ok(SpecPat::Record {
                    fields: out,
                    boxed: !is_closed,
                })
            }
            Pat::Con(raw_ctor, args) => {
                let ctor = subst_constructor(raw_ctor, &self.type_subst, &mut self.spec.interner);
                let is_gadt = self.spec.index.is_gadt_dict_or_existential(&ctor);
                let boxed = self.mode == LowerMode::Fallback
                    || is_gadt
                    || !ctor
                        .fields
                        .iter()
                        .all(|f| is_concrete_layout_type(f, &self.spec.index));
                self.spec.register_ctor(&ctor, scrut_ty, boxed);
                let bf = self.spec.ctor_boxed_fields(&ctor, boxed);
                let mut out = Vec::with_capacity(args.len());
                for (i, p) in args.iter().enumerate() {
                    if bf.contains(&i) {
                        let mut bs = Vec::new();
                        p.binders(&mut bs);
                        for b in bs {
                            boxed_binders.insert(b.to_owned(), true);
                        }
                    }
                    let fty = ctor.fields.get(i);
                    out.push(self.lower_pat(p, fty, boxed_binders)?);
                }
                Ok(SpecPat::Con {
                    ctor,
                    boxed,
                    args: out,
                })
            }
            Pat::Or(alts) => {
                let mut out = Vec::with_capacity(alts.len());
                for p in alts {
                    out.push(self.lower_pat(p, scrut_ty, boxed_binders)?);
                }
                Ok(SpecPat::Or(out))
            }
        }
    }
}
