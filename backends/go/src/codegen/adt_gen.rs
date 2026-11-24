//! Eligibility, layout selection, and expression emission for by-value closed ADTs.

use std::collections::{BTreeMap, HashSet};

use flick_backend_support::Known;
use flick_backend_support::names as js_names;
use flick_ir::{Constructor, IrType, Lang, Rep, TypeDecl, TypeInterner, TypeRef};
use flick_pretty::{Document, docvec};

use super::Gen;
use super::adt::{AdtField, AdtOps, AdtShape, AdtSlot, AdtVariant, struct_size_align};
use super::go_type::{GoType, TypeDecls, Typed, Want};
use super::layout::{Code, list};

/// Maximum number of constructors supported by a `uint8` discriminant.
const MAX_ADT_VARIANTS: usize = 255;

/// Maximum estimated 64-bit Go struct size (in bytes) for passing an ADT by value.
const MAX_ADT_BYTES: usize = 64;

fn check_template<'a>(
    lang: &Known,
    decls: &TypeDecls<'a>,
    module: &str,
    name: &str,
) -> Option<(&'a TypeDecl, Option<&'a str>, bool)> {
    if lang.is(Lang::Option, module, name)
        || lang.is(Lang::Errors, module, name)
        || lang.is(Lang::ServiceImpl, module, name)
        || lang.is(Lang::Field, module, name)
    {
        return None;
    }
    let is_result = lang.is(Lang::Result, module, name);
    let info = decls.get(&(module.to_owned(), name.to_owned()))?;
    if !is_result && !info.own {
        return None;
    }
    let decl = info.decl;
    if decl.alias.is_some()
        || decl.error
        || decl.constructors.is_empty()
        || decl.constructors.len() > MAX_ADT_VARIANTS
    {
        return None;
    }
    for ctor in &decl.constructors {
        if ctor.rep != Rep::Tagged
            || !ctor.constraints.is_empty()
            || !ctor.existentials.is_empty()
            || ctor.result_args.is_some()
        {
            return None;
        }
    }
    if is_recursive_decl(decls, module, name) {
        return None;
    }
    Some((decl, info.package, is_result))
}

/// Checks whether `(start_mod, start_name)` reaches itself through constructor
/// field types in `decls` (direct or mutual recursion across modules).
fn is_recursive_decl(decls: &TypeDecls, start_mod: &str, start_name: &str) -> bool {
    fn walk_type(
        decls: &TypeDecls<'_>,
        ty: &IrType,
        target: (&str, &str),
        seen: &mut HashSet<(String, String)>,
    ) -> bool {
        match ty {
            IrType::Named { module, name, args } => {
                if module == target.0 && name == target.1 {
                    return true;
                }
                if args.iter().any(|a| walk_type(decls, a, target, seen)) {
                    return true;
                }
                let key = (module.clone(), name.clone());
                if seen.insert(key.clone())
                    && let Some(info) = decls.get(&key)
                {
                    if let Some(alias) = &info.decl.alias
                        && walk_type(decls, alias, target, seen)
                    {
                        return true;
                    }
                    for ctor in &info.decl.constructors {
                        if ctor
                            .fields
                            .iter()
                            .any(|f| walk_type(decls, f, target, seen))
                        {
                            return true;
                        }
                    }
                }
                false
            }
            IrType::Var(_) => false,
            IrType::App { head, args } => {
                walk_type(decls, head, target, seen)
                    || args.iter().any(|a| walk_type(decls, a, target, seen))
            }
            IrType::Fun { param, result, .. } => {
                walk_type(decls, param, target, seen) || walk_type(decls, result, target, seen)
            }
            IrType::Tuple(items) => items.iter().any(|t| walk_type(decls, t, target, seen)),
            IrType::Record { fields, .. } => fields
                .iter()
                .any(|(_, t)| walk_type(decls, t, target, seen)),
            IrType::Forall { body, .. } => walk_type(decls, body, target, seen),
            IrType::Exists { contract, body, .. } => {
                walk_type(decls, contract, target, seen) || walk_type(decls, body, target, seen)
            }
        }
    }

    let Some(start) = decls.get(&(start_mod.to_owned(), start_name.to_owned())) else {
        return false;
    };
    let mut seen = HashSet::new();
    for ctor in &start.decl.constructors {
        if ctor
            .fields
            .iter()
            .any(|f| walk_type(decls, f, (start_mod, start_name), &mut seen))
        {
            return true;
        }
    }
    false
}

/// Whether `ty` is ground and agreement-invariant as a concrete ADT type argument.
fn is_concrete_arg(lang: &Known, decls: &TypeDecls, ty: &IrType) -> bool {
    match ty {
        IrType::Var(_) | IrType::App { .. } | IrType::Exists { .. } | IrType::Forall { .. } => {
            false
        }
        IrType::Fun { param, result, .. } => {
            is_concrete_arg(lang, decls, param) && is_concrete_arg(lang, decls, result)
        }
        IrType::Tuple(items) => items.iter().all(|t| is_concrete_arg(lang, decls, t)),
        IrType::Record { fields, tail } => {
            tail.is_none() && fields.iter().all(|(_, t)| is_concrete_arg(lang, decls, t))
        }
        IrType::Named { module, name, args } => {
            if lang.is(Lang::Errors, module, name)
                || lang.is(Lang::ServiceImpl, module, name)
                || lang.is(Lang::Field, module, name)
            {
                return false;
            }
            if decls
                .get(&(module.clone(), name.clone()))
                .is_some_and(|info| info.decl.alias.is_some())
            {
                return false;
            }
            args.iter().all(|a| is_concrete_arg(lang, decls, a))
        }
    }
}

fn variant_tag(
    lang: &Known,
    module: &str,
    ty: &str,
    ctor_name: &str,
    package: Option<&str>,
    is_result: bool,
) -> String {
    if is_result {
        if lang.is(Lang::Ok, module, ctor_name) {
            return "Success".to_owned();
        }
        if lang.is(Lang::Err, module, ctor_name) {
            return "Failure".to_owned();
        }
    }
    let dummy = Constructor {
        module: module.to_owned(),
        ty: ty.to_owned(),
        name: ctor_name.to_owned(),
        arity: 0,
        rep: Rep::Tagged,
        dicts: 0,
        fields: Vec::new(),
        type_args: Vec::new(),
    };
    js_names::tag(&dummy, package)
}

impl AdtShape {
    /// Whether `ty` names a parameterized eligible ADT template that could
    /// instantiate to a by-value `AdtShape` under some type substitution.
    pub(super) fn could_be_adt_under_subst(lang: &Known, decls: &TypeDecls, ty: &IrType) -> bool {
        let IrType::Named { module, name, .. } = ty else {
            return false;
        };
        let Some((decl, _, _)) = check_template(lang, decls, module, name) else {
            return false;
        };
        !decl.params.is_empty()
    }

    /// Computes the by-value closed ADT representation for `ty` when eligible.
    ///
    /// Substitution-invariant: every type argument and every constructor field
    /// is concrete and agreement-invariant, so `of(σ(T)) == of(T)` whenever
    /// `of(T)` is `Some`.
    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: Option<&IrType>) -> Option<AdtShape> {
        let IrType::Named { module, name, args } = ty? else {
            return None;
        };
        let (decl, package, is_result) = check_template(lang, decls, module, name)?;
        if args.len() != decl.params.len() {
            return None;
        }
        if !args.iter().all(|a| is_concrete_arg(lang, decls, a)) {
            return None;
        }

        let subst: Vec<(&str, &TypeRef)> = decl
            .params
            .iter()
            .map(String::as_str)
            .zip(args.iter())
            .collect();
        let mut interner = TypeInterner::new();
        let mut raw_variants: Vec<(String, String, Vec<AdtSlot>)> =
            Vec::with_capacity(decl.constructors.len());

        for ctor in &decl.constructors {
            let mut field_slots = Vec::with_capacity(ctor.fields.len());
            for fty in &ctor.fields {
                let resolved = if subst.is_empty() {
                    fty.clone()
                } else {
                    fty.substitute(&subst, &mut interner)
                };
                field_slots.push(AdtSlot::of(lang, decls, &resolved)?);
            }
            let tag = variant_tag(lang, module, name, &ctor.name, package, is_result);
            raw_variants.push((ctor.name.clone(), tag, field_slots));
        }

        // Pool non-Unit slots by exact Go slot type across variants (max multiplicity).
        let mut max_counts: BTreeMap<AdtSlot, usize> = BTreeMap::new();
        for (_, _, fslots) in &raw_variants {
            let mut per_variant: BTreeMap<AdtSlot, usize> = BTreeMap::new();
            for s in fslots {
                if !matches!(s, AdtSlot::Unit) {
                    *per_variant.entry(s.clone()).or_insert(0) += 1;
                }
            }
            for (s, count) in per_variant {
                let entry = max_counts.entry(s).or_insert(0);
                *entry = (*entry).max(count);
            }
        }
        let mut slots: Vec<AdtSlot> = Vec::new();
        for (slot, count) in max_counts {
            for _ in 0..count {
                slots.push(slot.clone());
            }
        }
        slots.sort_by(|a, b| a.layout_rank().cmp(&b.layout_rank()).then_with(|| a.cmp(b)));

        let has_discriminant = raw_variants.len() > 1;
        let (size, _) = struct_size_align(&slots, has_discriminant);
        if size > MAX_ADT_BYTES {
            return None;
        }

        let mut variants = Vec::with_capacity(raw_variants.len());
        for (vname, tag, fslots) in raw_variants {
            let mut used = vec![false; slots.len()];
            let mut fields = Vec::with_capacity(fslots.len());
            for slot in fslots {
                let pool = if matches!(slot, AdtSlot::Unit) {
                    None
                } else {
                    let idx = slots
                        .iter()
                        .enumerate()
                        .position(|(i, s)| !used[i] && *s == slot)
                        .expect("slot pooled by max multiplicity");
                    used[idx] = true;
                    Some(idx)
                };
                fields.push(AdtField { slot, pool });
            }
            variants.push(AdtVariant {
                name: vname,
                tag,
                fields,
            });
        }

        Some(AdtShape {
            module: module.clone(),
            name: name.clone(),
            parameterized: !decl.params.is_empty(),
            is_result,
            variants,
            slots,
        })
    }
}

impl<'a> Gen<'a> {
    pub(super) fn adt_shape_of(&self, ty: Option<&IrType>) -> Option<AdtShape> {
        let decls = self.decls?;
        AdtShape::of(self.input.lang, decls, ty)
    }

    pub(super) fn register_adt(&mut self, shape: &AdtShape) {
        self.adt_shapes.entry(shape.clone()).or_default();
    }

    pub(super) fn register_adt_ops(&mut self, shape: &AdtShape, ops: AdtOps) {
        self.adt_shapes.entry(shape.clone()).or_default().merge(ops);
    }

    /// Resolves the target `AdtShape` and variant index for constructing `c`
    /// under `want` and expression type `ty`. Returns `None` for `Want::Value`
    /// so erased sites build `*T_...` / `*flickrt.Data` directly.
    pub(super) fn adt_con_target(
        &self,
        c: &Constructor,
        ty: Option<&IrType>,
        want: Want<'_>,
    ) -> Option<(AdtShape, usize)> {
        if c.rep != Rep::Tagged || c.dicts > 0 {
            return None;
        }
        let shape = match want {
            Want::Exact(GoType::Adt(s)) => s.clone(),
            Want::Any => self.adt_shape_of(ty)?,
            Want::Value | Want::Exact(_) => return None,
        };
        let vi = shape.variant_index(c)?;
        Some((shape, vi))
    }

    /// Emits a saturated closed ADT constructor as a Go value composite literal.
    pub(super) fn adt_con_call(
        &mut self,
        shape: &AdtShape,
        vi: usize,
        all: &[Code<'a>],
    ) -> Typed<'a> {
        let a = self.a;
        self.register_adt(shape);
        let ty = a.alloc_str(&shape.go_type());
        let variant = &shape.variants[vi];
        let mut items: Vec<Document<'a>> = Vec::with_capacity(variant.fields.len() + 1);
        if shape.has_discriminant() && (vi != 0 || !variant.fields.is_empty()) {
            items.push(docvec![a, "tag: ", vi]);
        }
        let has_side_unit = variant
            .fields
            .iter()
            .zip(all)
            .any(|(f, code)| f.pool.is_none() && !code.atomic);
        let mut resolved: Vec<Code<'a>> = all.to_vec();
        if has_side_unit {
            let last_unit = variant
                .fields
                .iter()
                .zip(all)
                .rposition(|(f, code)| f.pool.is_none() && !code.atomic)
                .unwrap_or(0);
            for (field, code) in variant
                .fields
                .iter()
                .zip(resolved.iter_mut())
                .take(last_unit + 1)
            {
                if field.pool.is_none() {
                    if !code.atomic {
                        self.emit(docvec![a, "_ = ", *code]);
                    }
                } else if !code.atomic {
                    let t = self.temp();
                    let fty = field.slot.to_go_type();
                    if let Some(gt) = &fty {
                        self.register_go_type(gt);
                    }
                    let go_ty = self.go_type_name(fty.as_ref());
                    self.emit(docvec![a, "var ", t, " ", go_ty, " = ", *code]);
                    *code = self.code(t);
                }
            }
        }
        for (field, code) in variant.fields.iter().zip(resolved) {
            if let Some(pool) = field.pool {
                items.push(docvec![a, "f", pool, ": ", code.doc]);
            }
        }
        let doc = if items.is_empty() {
            docvec![a, ty, "{}"]
        } else {
            list(a, docvec![a, ty, "{"], items, "}")
        };
        Typed {
            code: Code { doc, atomic: false },
            ty: Some(GoType::Adt(shape.clone())),
        }
    }
}
