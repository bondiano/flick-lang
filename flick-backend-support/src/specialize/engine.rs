//! Worklist-driven deterministic specialization engine, layout integration,
//! and instantiation/output-size budget enforcement.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use flick_ir::{Constructor, Dict, Foreign, IrType, Program, TypeInterner, TypeRef};

use crate::layout::{LayoutEngine, PrimitiveTable, TypeLayout, TypeRepr};

use super::classify::{Classification, ItemEntry, ProgramIndex, classify_program};
use super::ir::{ExprCounts, SpecExpr, count_expr};
use super::type_util::{is_concrete_layout_type, mentions_any_free_var, subst_dict};
use super::types::{
    CanonicalDict, CtorKey, DefKey, DefMode, DictSpec, FallbackReason, InstanceKey,
    SpecializedCtor, SpecializedDef, SpecializedForeign, SpecializedInstance, SymbolId,
    mangle_foreign, mangle_key, mangle_runtime_instance,
};
use super::work::WorkItem;
use super::{
    BudgetKind, DiagnosticKind, SpecializeConfig, SpecializeDiagnostic, SpecializeError,
    SpecializeStats, SpecializedProgram,
};

/// Stateful specialization engine for one [`Program`].
pub(crate) struct Specializer<'p> {
    pub index: ProgramIndex<'p>,
    pub classification: Classification,
    pub layout_engine: LayoutEngine<'p>,
    pub interner: TypeInterner,
    pub config: SpecializeConfig,

    seen_keys: BTreeMap<InstanceKey, SymbolId>,
    seen_runtime_instances: BTreeMap<DefKey, SymbolId>,
    seen_foreigns: BTreeMap<(DefKey, bool), SymbolId>,
    pending: VecDeque<WorkItem>,

    pub definitions: BTreeMap<SymbolId, SpecializedDef>,
    pub constructors: BTreeMap<CtorKey, SpecializedCtor>,
    pub instances: BTreeMap<SymbolId, SpecializedInstance>,
    pub foreigns: BTreeMap<SymbolId, SpecializedForeign>,
    pub layouts: BTreeMap<TypeRef, TypeLayout>,
    diagnostics: Vec<SpecializeDiagnostic>,
    reported_fallbacks: BTreeSet<DefKey>,

    pub direct_dict_calls: usize,
    total_specialized: usize,
    instances_per_def: BTreeMap<DefKey, usize>,
    estimated_bytes: usize,
}

/// Entry point invoked by [`super::specialize`].
pub(crate) fn run(
    program: &Program,
    roots: &[DefKey],
    primitives: &PrimitiveTable,
    config: &SpecializeConfig,
) -> Result<SpecializedProgram, SpecializeError> {
    let index = ProgramIndex::build(program);
    let classification = classify_program(&index);
    let layout_engine = LayoutEngine::new(program, primitives.clone())?;

    let mut spec = Specializer {
        index,
        classification,
        layout_engine,
        interner: TypeInterner::new(),
        config: config.clone(),
        seen_keys: BTreeMap::new(),
        seen_runtime_instances: BTreeMap::new(),
        seen_foreigns: BTreeMap::new(),
        pending: VecDeque::new(),
        definitions: BTreeMap::new(),
        constructors: BTreeMap::new(),
        instances: BTreeMap::new(),
        foreigns: BTreeMap::new(),
        layouts: BTreeMap::new(),
        diagnostics: Vec::new(),
        reported_fallbacks: BTreeSet::new(),
        direct_dict_calls: 0,
        total_specialized: 0,
        instances_per_def: BTreeMap::new(),
        estimated_bytes: 0,
    };

    let mut sorted_roots: Vec<DefKey> = roots.to_vec();
    sorted_roots.sort();
    sorted_roots.dedup();

    let mut root_map = BTreeMap::new();
    for root in &sorted_roots {
        let Some(entry) = spec.index.items.get(root).copied() else {
            return Err(SpecializeError::UnknownRoot(root.clone()));
        };
        let sym = match entry {
            ItemEntry::Value {
                type_params, dicts, ..
            } => {
                if let Some(reason) = spec.classification.fallback_reason(root) {
                    spec.enqueue_fallback(root, reason)
                } else if !type_params.is_empty() {
                    spec.enqueue_fallback(root, FallbackReason::OpenRowPolymorphic)
                } else {
                    let dict_specs = vec![DictSpec::Runtime; dicts.len()];
                    spec.enqueue_specialized_value(root, Vec::new(), dict_specs)?
                }
            }
            ItemEntry::Foreign { foreign } => {
                let poly = spec.classification.fallback_reason(root).is_some();
                spec.enqueue_foreign(root, foreign, poly)
            }
            ItemEntry::Instance { .. } => spec.enqueue_runtime_instance(&root.0, &root.1),
            ItemEntry::Constructor { ctor } => {
                let boxed = spec.index.is_gadt_dict_or_existential(ctor);
                spec.register_ctor(ctor, None, boxed);
                SymbolId(format!("{}.{}", root.0, root.1))
            }
            ItemEntry::Destructure | ItemEntry::Service => {
                SymbolId(format!("{}.{}", root.0, root.1))
            }
        };
        root_map.insert(root.clone(), sym);
    }

    while let Some(item) = spec.pending.pop_front() {
        spec.process(item)?;
    }

    let stats = spec.compute_stats();
    Ok(SpecializedProgram {
        roots: root_map,
        definitions: spec.definitions,
        constructors: spec.constructors,
        instances: spec.instances,
        foreigns: spec.foreigns,
        layouts: spec.layouts,
        stats,
        diagnostics: spec.diagnostics,
    })
}

impl Specializer<'_> {
    /// Computes and caches the deterministic [`TypeLayout`] for concrete `ty`.
    pub(crate) fn ensure_layout(&mut self, ty: &TypeRef) -> Result<&TypeLayout, SpecializeError> {
        if !self.layouts.contains_key(ty) {
            let layout = self.layout_engine.layout(ty)?;
            self.layouts.insert(ty.clone(), layout);
        }
        Ok(&self.layouts[ty])
    }

    /// Resolves a closed record field `(index, offset, size)` using
    /// [`LayoutEngine`].
    pub(crate) fn record_field_offset(
        &mut self,
        rec_ty: &TypeRef,
        label: &str,
    ) -> Result<Option<(usize, u64, u64)>, SpecializeError> {
        if !matches!(&**rec_ty, IrType::Record { tail: None, .. })
            || !is_concrete_layout_type(rec_ty, &self.index)
        {
            return Ok(None);
        }
        let layout = self.ensure_layout(rec_ty)?;
        let TypeRepr::Record(rec) = &layout.repr else {
            return Ok(None);
        };
        Ok(rec
            .fields
            .iter()
            .find(|f| f.label == label)
            .map(|f| (f.field.index, f.field.offset, f.field.size)))
    }

    /// Returns which field indices of `ctor` must be stored as boxed cells.
    pub(crate) fn ctor_boxed_fields(&self, ctor: &Constructor, boxed: bool) -> Vec<usize> {
        if let Some((_, decl)) = self.index.ctor_decl(ctor)
            && !decl.existentials.is_empty()
        {
            let ex: BTreeSet<&str> = decl.existentials.iter().map(String::as_str).collect();
            return decl
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, f)| (boxed || mentions_any_free_var(f, &ex)).then_some(i))
                .collect();
        }
        if boxed {
            (0..ctor.arity).collect()
        } else {
            Vec::new()
        }
    }

    /// Registers a reachable constructor and computes its concrete ADT layout
    /// when monomorphized.
    pub(crate) fn register_ctor(
        &mut self,
        ctor: &Constructor,
        result_ty: Option<&TypeRef>,
        boxed: bool,
    ) {
        let key = CtorKey {
            module: ctor.module.clone(),
            ty: ctor.ty.clone(),
            name: ctor.name.clone(),
            type_args: if boxed {
                Vec::new()
            } else {
                ctor.type_args.clone()
            },
            boxed,
        };
        if self.constructors.contains_key(&key) {
            return;
        }
        let concrete_result = result_ty
            .filter(|t| is_concrete_layout_type(t, &self.index))
            .cloned()
            .or_else(|| {
                if !boxed
                    && ctor
                        .type_args
                        .iter()
                        .all(|a| is_concrete_layout_type(a, &self.index))
                {
                    let decl_arity = self
                        .index
                        .type_decls
                        .get(&(ctor.module.clone(), ctor.ty.clone()))
                        .map_or(ctor.type_args.len(), |d| d.params.len());
                    if decl_arity == ctor.type_args.len() {
                        Some(self.interner.named(
                            ctor.module.clone(),
                            ctor.ty.clone(),
                            ctor.type_args.clone(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        let layout = concrete_result
            .as_ref()
            .and_then(|res| self.ensure_layout(res).ok().cloned());
        let boxed_fields = self.ctor_boxed_fields(ctor, boxed);
        self.constructors.insert(
            key.clone(),
            SpecializedCtor {
                key,
                ctor: ctor.clone(),
                result_ty: concrete_result,
                layout,
                boxed_fields,
            },
        );
    }

    /// Enqueues a monomorphized top-level value definition, enforcing
    /// [`SpecializeConfig::max_instances`].
    pub(crate) fn enqueue_specialized_value(
        &mut self,
        def: &DefKey,
        type_args: Vec<TypeRef>,
        dict_specs: Vec<DictSpec>,
    ) -> Result<SymbolId, SpecializeError> {
        let key = InstanceKey::Value {
            def: def.clone(),
            type_args: type_args.clone(),
            dict_specs: dict_specs.clone(),
        };
        if let Some(existing) = self.seen_keys.get(&key) {
            return Ok(existing.clone());
        }
        self.check_instance_budget(def)?;
        let symbol = mangle_key(&key);
        self.seen_keys.insert(key, symbol.clone());
        self.total_specialized += 1;
        *self.instances_per_def.entry(def.clone()).or_insert(0) += 1;
        self.pending.push_back(WorkItem::SpecializedValue {
            symbol: symbol.clone(),
            def: def.clone(),
            type_args,
            dict_specs,
        });
        Ok(symbol)
    }

    /// Enqueues a uniform boxed-cell fallback definition (compiled at most once
    /// per `def`).
    pub(crate) fn enqueue_fallback(&mut self, def: &DefKey, reason: FallbackReason) -> SymbolId {
        let key = InstanceKey::Fallback { def: def.clone() };
        if let Some(existing) = self.seen_keys.get(&key) {
            return existing.clone();
        }
        let symbol = mangle_key(&key);
        self.seen_keys.insert(key, symbol.clone());
        if self.reported_fallbacks.insert(def.clone()) {
            self.diagnostics.push(SpecializeDiagnostic {
                def: def.clone(),
                kind: DiagnosticKind::FallbackSelected(reason),
                message: format!(
                    "`{}.{}` compiled once over uniform boxed cells ({reason})",
                    def.0, def.1
                ),
            });
        }
        self.pending.push_back(WorkItem::FallbackValue {
            symbol: symbol.clone(),
            def: def.clone(),
            reason,
        });
        symbol
    }

    /// Resolves a statically known [`CanonicalDict`] (including superclass
    /// chains) to its target `(module, instance_name)` and static context args.
    pub(crate) fn resolve_static_instance(
        &self,
        canon: &CanonicalDict,
    ) -> Option<(DefKey, Vec<CanonicalDict>)> {
        match canon {
            CanonicalDict::Instance { module, name, args } => {
                let key = (module.clone(), name.clone());
                matches!(self.index.items.get(&key), Some(ItemEntry::Instance { .. }))
                    .then_some((key, args.clone()))
            }
            CanonicalDict::Super { dict, class } => {
                let (inst_key, ctx_args) = self.resolve_static_instance(dict)?;
                let Some(ItemEntry::Instance { info, .. }) = self.index.items.get(&inst_key) else {
                    return None;
                };
                let (_, super_dict) = info.supers.iter().find(|(c, _)| c == class)?;
                let ctx_env: BTreeMap<String, Dict> = info
                    .context
                    .iter()
                    .cloned()
                    .zip(ctx_args.iter().map(CanonicalDict::to_dict))
                    .collect();
                let resolved_super = subst_dict(super_dict, &ctx_env);
                let canon_super = CanonicalDict::from_static(&resolved_super)?;
                self.resolve_static_instance(&canon_super)
            }
            _ => None,
        }
    }

    /// Enqueues a devirtualized direct call to a known class instance method.
    pub(crate) fn enqueue_direct_method(
        &mut self,
        instance: &DefKey,
        method: &str,
        context_dicts: Vec<CanonicalDict>,
        type_args: Vec<TypeRef>,
        method_dict_specs: Vec<DictSpec>,
    ) -> Result<Option<SymbolId>, SpecializeError> {
        let Some(ItemEntry::Instance { info, methods, .. }) = self.index.items.get(instance) else {
            return Ok(None);
        };
        let has_method = methods.iter().any(|(m, _)| m == method)
            || info.defaults.iter().any(|(m, _, _)| m == method);
        if !has_method {
            return Ok(None);
        }
        let key = InstanceKey::InstanceMethod {
            instance: instance.clone(),
            method: method.to_owned(),
            context_dicts: context_dicts.clone(),
            type_args: type_args.clone(),
            method_dict_specs: method_dict_specs.clone(),
        };
        if let Some(existing) = self.seen_keys.get(&key) {
            return Ok(Some(existing.clone()));
        }
        self.check_instance_budget(instance)?;
        let symbol = mangle_key(&key);
        self.seen_keys.insert(key, symbol.clone());
        self.total_specialized += 1;
        *self.instances_per_def.entry(instance.clone()).or_insert(0) += 1;
        self.pending.push_back(WorkItem::DirectMethod {
            symbol: symbol.clone(),
            instance: instance.clone(),
            method: method.to_owned(),
            context_dicts,
            type_args,
            method_dict_specs,
        });
        Ok(Some(symbol))
    }

    /// Enqueues a runtime class instance table (for dynamic dictionaries).
    pub(crate) fn enqueue_runtime_instance(&mut self, module: &str, name: &str) -> SymbolId {
        let def = (module.to_owned(), name.to_owned());
        if let Some(existing) = self.seen_runtime_instances.get(&def) {
            return existing.clone();
        }
        let symbol = mangle_runtime_instance(module, name);
        self.seen_runtime_instances
            .insert(def.clone(), symbol.clone());
        self.pending.push_back(WorkItem::RuntimeInstance {
            symbol: symbol.clone(),
            def,
        });
        symbol
    }

    /// Registers a reachable `foreign` binding.
    pub(crate) fn enqueue_foreign(
        &mut self,
        def: &DefKey,
        foreign: &Foreign,
        polymorphic: bool,
    ) -> SymbolId {
        let key = (def.clone(), polymorphic);
        if let Some(existing) = self.seen_foreigns.get(&key) {
            return existing.clone();
        }
        let symbol = mangle_foreign(&def.0, &def.1, polymorphic);
        self.seen_foreigns.insert(key, symbol.clone());
        if polymorphic && self.reported_fallbacks.insert(def.clone()) {
            self.diagnostics.push(SpecializeDiagnostic {
                def: def.clone(),
                kind: DiagnosticKind::FallbackSelected(FallbackReason::PolymorphicForeign),
                message: format!(
                    "`{}.{}` is a polymorphic foreign binding and uses uniform boxed cells",
                    def.0, def.1
                ),
            });
        }
        self.foreigns.insert(
            symbol.clone(),
            SpecializedForeign {
                symbol: symbol.clone(),
                module: def.0.clone(),
                name: def.1.clone(),
                foreign: foreign.clone(),
                polymorphic,
                reason: polymorphic.then_some(FallbackReason::PolymorphicForeign),
            },
        );
        symbol
    }

    fn check_instance_budget(&self, def: &DefKey) -> Result<(), SpecializeError> {
        let next = self.total_specialized + 1;
        if next > self.config.max_instances {
            return Err(SpecializeError::BudgetExceeded {
                def: def.clone(),
                kind: BudgetKind::MaxInstances,
                limit: self.config.max_instances,
                count: next,
            });
        }
        Ok(())
    }

    pub(crate) fn check_output_budget(
        &mut self,
        def: &DefKey,
        expr: &SpecExpr,
    ) -> Result<(), SpecializeError> {
        let mut counts = ExprCounts::default();
        count_expr(expr, &mut counts);
        self.estimated_bytes = self
            .estimated_bytes
            .saturating_add(counts.estimated_bytes());
        if let Some(limit) = self.config.max_output_bytes
            && self.estimated_bytes > limit
        {
            return Err(SpecializeError::BudgetExceeded {
                def: def.clone(),
                kind: BudgetKind::MaxOutputBytes,
                limit,
                count: self.estimated_bytes,
            });
        }
        Ok(())
    }

    fn compute_stats(&self) -> SpecializeStats {
        let mut counts = ExprCounts::default();
        let mut specialized_instances = 0;
        let mut fallback_definitions = 0;

        for def in self.definitions.values() {
            match def.mode {
                DefMode::Specialized => specialized_instances += 1,
                DefMode::Fallback(_) => fallback_definitions += 1,
            }
            count_expr(&def.body, &mut counts);
        }
        for inst in self.instances.values() {
            for body in inst.methods.values() {
                count_expr(body, &mut counts);
            }
        }
        let polymorphic_foreigns = self.foreigns.values().filter(|f| f.polymorphic).count();

        SpecializeStats {
            specialized_instances,
            fallback_definitions,
            polymorphic_foreigns,
            box_count: counts.box_count,
            unbox_count: counts.unbox_count,
            boxed_cells: counts.box_count + counts.unbox_count,
            direct_dict_calls: self.direct_dict_calls,
            runtime_dict_calls: counts.runtime_dict_calls,
            offset_field_reads: counts.offset_field_reads,
            label_field_reads: counts.label_field_reads,
            estimated_output_bytes: counts.estimated_bytes(),
            instances_per_def: self.instances_per_def.clone(),
        }
    }
}
