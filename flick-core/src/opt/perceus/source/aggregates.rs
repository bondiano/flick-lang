//! Abstract aggregate-representation policy and `#742`/`#743` layout-to-memory
//! bridge for source-to-memory lowering (#744).

use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(target_arch = "wasm32"))]
use flick_backend_support::layout::{
    TypeRepr, cell_layout_of, value_rep_of, variant_cell_layout_of,
};
#[cfg(not(target_arch = "wasm32"))]
use flick_backend_support::specialize::{DefMode, DiagnosticKind, SpecializedProgram};
use flick_ir::{Constructor, IrType, TypeRef};
#[cfg(not(target_arch = "wasm32"))]
use flick_ir::{ItemKind, Module};

#[cfg(not(target_arch = "wasm32"))]
use crate::ir::Expr;
use crate::ir::Pat;
use crate::ir::memory::{CellLayout, ValueRep};

/// Boundary signature of a boxed fallback function whose polymorphic parameters
/// or return value cross between unboxed aggregates (`ValueRep::Aggregate`) and
/// uniform boxed fallback cells (`Data::BoxedAggregate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackTarget {
    /// For each curried value parameter, whether its declared type is
    /// polymorphic / existential / open-row (expecting a uniform boxed cell).
    pub poly_params: Vec<bool>,
    /// Whether the declared return type is polymorphic / existential / open-row
    /// (returning a uniform boxed cell that specialized callers must unbox).
    pub poly_return: bool,
}

impl FallbackTarget {
    /// Extracts a fallback boundary signature from a higher-rank `IrType::Forall`
    /// parameter type (returning `None` for non-higher-rank types).
    pub(crate) fn from_higher_rank_type(ty: &IrType) -> Option<Self> {
        let IrType::Forall {
            vars,
            constraints,
            body,
        } = ty
        else {
            return None;
        };
        if vars.is_empty() && constraints.is_empty() {
            return None;
        }
        Self::from_signature(body)
    }

    pub(crate) fn from_signature(mut sig: &IrType) -> Option<Self> {
        while let IrType::Forall { body, .. } = sig {
            sig = body;
        }
        let (params, result) = sig.fun_stages();
        let poly_params: Vec<bool> = params
            .iter()
            .map(|(p, _)| type_needs_uniform_cell(p))
            .collect();
        let poly_return = type_needs_uniform_cell(result);
        if poly_params.iter().any(|&b| b) || poly_return {
            Some(Self {
                poly_params,
                poly_return,
            })
        } else {
            None
        }
    }
}

/// Policy selecting which constructors/types lower as unboxed by-value
/// aggregates (`ValueRep::Aggregate`) vs. typed heap cells (`ValueRep::Cell` +
/// [`CellLayout`]), and recording `#743` fallback boundaries for automatic
/// `BoxAggregate` / `UnboxBorrow` / `UnboxConsume` emission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregatePolicy {
    /// Fully qualified type names (`"Module.Type"`) whose constructors lower as
    /// unboxed `ValueRep::Aggregate` values (explicit override API).
    pub types: BTreeSet<String>,
    /// Fully qualified constructor tags (`"Module.Type.Con"` or `"$tuple"`)
    /// that lower as unboxed `ValueRep::Aggregate` values.
    pub constructors: BTreeSet<String>,
    /// Optional abstract `CellLayout` descriptors attached to boxed heap
    /// constructor cells (`ValueRep::Cell`), keyed by qualified constructor tag.
    pub cell_layouts: BTreeMap<String, CellLayout>,
    /// Per-instantiation representation decisions derived automatically from
    /// `#742` [`TypeLayout`](flick_backend_support::layout::TypeLayout)s.
    pub instantiated_reps: BTreeMap<TypeRef, ValueRep>,
    /// Per-instantiation constructor [`CellLayout`] descriptors derived from
    /// `#742` variant layouts, keyed by `(instantiated_type, ctor_name)`.
    pub instantiated_cell_layouts: BTreeMap<(TypeRef, String), CellLayout>,
    /// Per-instantiation whole-type [`CellLayout`] descriptors for fallback
    /// `BoxAggregate` cells.
    pub type_cell_layouts: BTreeMap<TypeRef, CellLayout>,
    /// Top-level fallback functions `(module, name)` whose boundaries require
    /// boxing/unboxing of unboxed aggregate arguments/results.
    pub fallback_targets: BTreeMap<(String, String), FallbackTarget>,
    /// Top-level definitions `(module, name)` whose own bodies execute in
    /// uniform boxed fallback mode.
    pub fallback_functions: BTreeSet<(String, String)>,
}

impl AggregatePolicy {
    /// Automatically derives [`ValueRep`], [`CellLayout`], and fallback
    /// boundary policies from a `#743` [`SpecializedProgram`] and its `#742`
    /// computed layouts (with zero hand-maintained type-name lists).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_specialized(modules: &[Module], specialized: &SpecializedProgram) -> Self {
        let mut policy = Self::default();

        // Types appearing in `foreign` declarations (such as host intrinsics
        // returning `Option String` in `tier0`) and nominal type constructors
        // instantiated with free type variables inside reached fallback
        // functions stay `ValueRep::Cell` so representations match everywhere.
        let mut cell_only_types = BTreeSet::new();
        let mut cell_only_named_ctors: BTreeSet<(String, String)> = BTreeSet::new();
        let mut reached_fallbacks: BTreeSet<(String, String)> = BTreeSet::new();

        for def in specialized.definitions.values() {
            if matches!(def.mode, DefMode::Fallback(_)) {
                policy.fallback_functions.insert(def.source.clone());
                reached_fallbacks.insert(def.source.clone());
            }
        }
        for foreign in specialized.foreigns.values() {
            if foreign.polymorphic {
                let key = (foreign.module.clone(), foreign.name.clone());
                policy.fallback_functions.insert(key.clone());
                reached_fallbacks.insert(key);
            }
        }
        for diag in &specialized.diagnostics {
            if matches!(diag.kind, DiagnosticKind::FallbackSelected(_)) {
                policy.fallback_functions.insert(diag.def.clone());
                reached_fallbacks.insert(diag.def.clone());
            }
        }

        for module in modules {
            for item in &module.items {
                match &item.kind {
                    ItemKind::Foreign(f) => {
                        if let Some(ty) = &f.ty {
                            collect_named_types(ty, &mut cell_only_types);
                        }
                    }
                    ItemKind::Value { name, ty, body, .. }
                        if reached_fallbacks.contains(&(module.name.clone(), name.clone())) =>
                    {
                        if let Some(sig) = ty {
                            collect_poly_named_ctors(sig, &mut cell_only_named_ctors);
                        }
                        collect_poly_named_ctors_in_expr(body, &mut cell_only_named_ctors);
                    }
                    _ => {}
                }
            }
        }

        // Populate per-instantiation `ValueRep` and `CellLayout` from `#742`.
        for (ty, layout) in &specialized.layouts {
            if let TypeRepr::Data(data) = &layout.repr {
                let forced_cell = cell_only_types.contains(ty)
                    || matches!(&**ty, IrType::Named { module, name, .. }
                        if cell_only_named_ctors.contains(&(module.clone(), name.clone())));
                let rep = if forced_cell {
                    ValueRep::Cell
                } else {
                    value_rep_of(layout)
                };
                policy.instantiated_reps.insert(ty.clone(), rep);
                policy
                    .type_cell_layouts
                    .insert(ty.clone(), cell_layout_of(layout));
                for variant in &data.variants {
                    let vlay = variant_cell_layout_of(layout, variant);
                    policy
                        .instantiated_cell_layouts
                        .insert((ty.clone(), variant.name.clone()), vlay);
                }
            }
        }

        // Record fallback functions and their polymorphic parameter/return slots.
        for module in modules {
            for item in &module.items {
                let (name, ty, is_fallback) = match &item.kind {
                    ItemKind::Value {
                        name,
                        type_params,
                        ty,
                        ..
                    } => {
                        let key = (module.name.clone(), name.clone());
                        let is_fb =
                            policy.fallback_functions.contains(&key) || !type_params.is_empty();
                        (name.as_str(), ty.as_ref(), is_fb)
                    }
                    ItemKind::Foreign(f) => {
                        let is_fb = !f.type_params.is_empty();
                        (f.name.as_str(), f.ty.as_ref(), is_fb)
                    }
                    _ => continue,
                };
                if !is_fallback {
                    continue;
                }
                let key = (module.name.clone(), name.to_owned());
                policy.fallback_functions.insert(key.clone());
                if let Some(sig) = ty
                    && let Some(target) = FallbackTarget::from_signature(sig)
                {
                    policy.fallback_targets.insert(key, target);
                }
            }
        }

        policy
    }

    pub fn is_fallback_function(&self, module: &str, name: &str) -> bool {
        self.fallback_functions
            .contains(&(module.to_owned(), name.to_owned()))
    }

    pub fn fallback_target(&self, module: &str, name: &str) -> Option<&FallbackTarget> {
        self.fallback_targets
            .get(&(module.to_owned(), name.to_owned()))
    }

    pub fn is_aggregate_constructor(&self, con: &Constructor) -> bool {
        if self.types.is_empty() && self.constructors.is_empty() {
            return false;
        }
        let ty_key = format!("{}.{}", con.module, con.ty);
        if self.types.contains(&ty_key) {
            return true;
        }
        let tag = format!("{ty_key}.{}", con.name);
        self.constructors.contains(&tag)
    }

    pub fn constructor_rep(
        &self,
        con: &Constructor,
        expr_ty: Option<&TypeRef>,
        _in_fallback: bool,
    ) -> ValueRep {
        if self.is_aggregate_constructor(con) {
            return ValueRep::Aggregate;
        }
        if self.instantiated_reps.is_empty() {
            return ValueRep::Cell;
        }
        if let Some(result_ty) = constructor_result_type(con, expr_ty)
            && let Some(&rep) = self.instantiated_reps.get(&result_ty)
        {
            return rep;
        }
        ValueRep::Cell
    }

    pub fn constructor_cell_layout(
        &self,
        con: &Constructor,
        expr_ty: Option<&TypeRef>,
        tag: &str,
    ) -> Option<CellLayout> {
        if let Some(lay) = self.cell_layouts.get(tag) {
            return Some(lay.clone());
        }
        if self.instantiated_cell_layouts.is_empty() {
            return None;
        }
        let result_ty = constructor_result_type(con, expr_ty)?;
        self.instantiated_cell_layouts
            .get(&(result_ty, con.name.clone()))
            .cloned()
    }

    pub fn type_is_aggregate(&self, ty: &TypeRef) -> bool {
        if self.instantiated_reps.get(ty) == Some(&ValueRep::Aggregate) {
            return true;
        }
        if let IrType::Named { module, name, .. } = &**ty {
            let key = format!("{module}.{name}");
            if self.types.contains(&key) {
                return true;
            }
        }
        false
    }

    pub fn cell_layout_for_type(&self, ty: &TypeRef) -> Option<CellLayout> {
        self.type_cell_layouts.get(ty).cloned()
    }

    pub fn is_aggregate_tag(&self, tag: &str) -> bool {
        self.constructors.contains(tag)
    }

    pub fn cell_layout(&self, tag: &str) -> Option<CellLayout> {
        self.cell_layouts.get(tag).cloned()
    }

    pub fn pattern_is_aggregate(&self, pat: &Pat) -> bool {
        if self.types.is_empty() && self.constructors.is_empty() {
            return false;
        }
        match pat {
            Pat::Con(con, _) => self.is_aggregate_constructor(con),
            Pat::Tuple(items) if !items.is_empty() => self.is_aggregate_tag("$tuple"),
            Pat::Or(alts) => alts.iter().any(|alt| self.pattern_is_aggregate(alt)),
            _ => false,
        }
    }

    pub fn pattern_is_aggregate_in(
        &self,
        pat: &Pat,
        scrut_ty: Option<&TypeRef>,
        _in_fallback: bool,
    ) -> bool {
        if self.pattern_is_aggregate(pat) {
            return true;
        }
        if self.instantiated_reps.is_empty() {
            return false;
        }
        if let Some(ty) = scrut_ty {
            return self.instantiated_reps.get(ty) == Some(&ValueRep::Aggregate);
        }
        match pat {
            Pat::Con(con, _) => {
                if let Some(res_ty) = constructor_result_type(con, None) {
                    return self.instantiated_reps.get(&res_ty) == Some(&ValueRep::Aggregate);
                }
                false
            }
            Pat::Or(alts) => alts
                .iter()
                .any(|alt| self.pattern_is_aggregate_in(alt, scrut_ty, false)),
            _ => false,
        }
    }
}

fn constructor_result_type(con: &Constructor, expr_ty: Option<&TypeRef>) -> Option<TypeRef> {
    if let Some(ty) = expr_ty {
        let (_, res) = ty.fun_stages();
        if let IrType::Named { module, name, .. } = res
            && module == &con.module
            && name == &con.ty
        {
            return Some(TypeRef::new(res.clone()));
        }
    }
    if con.dicts == 0 {
        return Some(TypeRef::new(IrType::Named {
            module: con.module.clone(),
            name: con.ty.clone(),
            args: con.type_args.clone(),
        }));
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_named_types(ty: &TypeRef, out: &mut BTreeSet<TypeRef>) {
    if matches!(&**ty, IrType::Named { .. }) {
        out.insert(ty.clone());
    }
    ty.each_child(&mut |child| collect_named_types(child, out));
}

#[cfg(not(target_arch = "wasm32"))]
fn type_has_var(ty: &IrType) -> bool {
    if matches!(ty, IrType::Var(_)) {
        return true;
    }
    let mut found = false;
    ty.each_child(&mut |child| {
        if !found && type_has_var(child) {
            found = true;
        }
    });
    found
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_poly_named_ctors(ty: &IrType, out: &mut BTreeSet<(String, String)>) {
    if let IrType::Named { module, name, args } = ty
        && args.iter().any(|arg| type_has_var(arg))
    {
        out.insert((module.clone(), name.clone()));
    }
    ty.each_child(&mut |child| collect_poly_named_ctors(child, out));
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_poly_named_ctors_in_expr(expr: &Expr, out: &mut BTreeSet<(String, String)>) {
    if let Some(ty) = &expr.ty {
        collect_poly_named_ctors(ty, out);
    }
    expr.each_child(&mut |child| collect_poly_named_ctors_in_expr(child, out));
}

fn type_needs_uniform_cell(ty: &IrType) -> bool {
    match ty {
        IrType::Var(_) | IrType::Exists { .. } | IrType::Record { tail: Some(_), .. } => true,
        IrType::Forall {
            vars, constraints, ..
        } if !vars.is_empty() || !constraints.is_empty() => true,
        _ => false,
    }
}
