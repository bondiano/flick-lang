//! Phase 2 of finite pointer indirection analysis: builds the finite graph of
//! instantiated by-value nominal ADTs reachable from a monomorphic root (after
//! Phase 1 has removed growing recursive declaration edges) and runs Tarjan's
//! strongly connected components to place cycle-forming fields behind pointers.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_ir::{IrType, TypeInterner, TypeRef};

use super::LayoutError;
use super::catalog::{NamedKind, TypeCatalog, instantiate_constructors, mentions_any_var};
use super::indirection::{StaticIndirection, tarjan_scc};

/// For every reachable instantiated nominal ADT, the set of
/// `(decl_ctor_index, field_index)` pairs that must be stored behind pointers.
pub(crate) type InstIndirectMap = BTreeMap<TypeRef, BTreeSet<(usize, usize)>>;

/// Builds the finite by-value graph of monomorphic nominal ADT instantiations
/// reachable from `root` (skipping Phase 1 growing fields) and uses Tarjan's
/// SCC to identify cycle-forming fields to store behind pointers.
pub(crate) fn compute_instantiated_indirection(
    catalog: &TypeCatalog<'_>,
    static_ind: &StaticIndirection,
    root: &TypeRef,
    interner: &mut TypeInterner,
) -> Result<InstIndirectMap, LayoutError> {
    let mut nodes: Vec<TypeRef> = Vec::new();
    let mut id_of: BTreeMap<TypeRef, usize> = BTreeMap::new();
    let mut worklist: Vec<usize> = Vec::new();

    // Seed with every nominal ADT directly reachable by value from `root`.
    let mut seeds = BTreeSet::new();
    collect_inst_targets(catalog, static_ind, root, interner, &mut seeds)?;
    for seed in seeds {
        let id = intern_node(&seed, &mut nodes, &mut id_of);
        worklist.push(id);
    }

    let mut visited: Vec<bool> = Vec::new();
    let mut adj: Vec<Vec<usize>> = Vec::new();
    let mut field_targets: Vec<Vec<(usize, usize, Vec<usize>)>> = Vec::new();
    let mut pre_indirect: Vec<BTreeSet<(usize, usize)>> = Vec::new();

    while let Some(node_id) = worklist.pop() {
        if visited.len() <= node_id {
            visited.resize(nodes.len(), false);
            adj.resize_with(nodes.len(), Vec::new);
            field_targets.resize_with(nodes.len(), Vec::new);
            pre_indirect.resize_with(nodes.len(), BTreeSet::new);
        }
        if visited[node_id] {
            continue;
        }
        visited[node_id] = true;
        let ty = nodes[node_id].clone();
        let IrType::Named { module, name, args } = &*ty else {
            continue;
        };
        let key = (module.clone(), name.clone());
        let NamedKind::Decl(decl) = catalog.classify(module, name, args.len())? else {
            continue;
        };
        let ctors = instantiate_constructors(decl, args, interner);
        let mut node_edges = BTreeSet::new();
        let mut per_field = Vec::new();
        let mut forced = BTreeSet::new();

        for inst in &ctors {
            for (field_idx, field_ty) in inst.fields.iter().enumerate() {
                let site = (key.clone(), inst.decl_index, field_idx);
                if static_ind.growing_fields.contains(&site) {
                    forced.insert((inst.decl_index, field_idx));
                    continue;
                }
                if mentions_any_var(field_ty, &inst.existentials)
                    && !is_pointer_boundary(catalog, static_ind, field_ty, interner)?
                {
                    forced.insert((inst.decl_index, field_idx));
                    continue;
                }
                let mut targets = BTreeSet::new();
                collect_inst_targets(catalog, static_ind, field_ty, interner, &mut targets)?;
                let mut target_ids = Vec::with_capacity(targets.len());
                for t in targets {
                    let is_new = !id_of.contains_key(&t);
                    let tid = intern_node(&t, &mut nodes, &mut id_of);
                    if is_new {
                        worklist.push(tid);
                    }
                    node_edges.insert(tid);
                    target_ids.push(tid);
                }
                per_field.push((inst.decl_index, field_idx, target_ids));
            }
        }
        visited.resize(nodes.len(), false);
        adj.resize_with(nodes.len(), Vec::new);
        field_targets.resize_with(nodes.len(), Vec::new);
        pre_indirect.resize_with(nodes.len(), BTreeSet::new);
        adj[node_id] = node_edges.into_iter().collect();
        field_targets[node_id] = per_field;
        pre_indirect[node_id] = forced;
    }

    adj.resize_with(nodes.len(), Vec::new);
    field_targets.resize_with(nodes.len(), Vec::new);
    pre_indirect.resize_with(nodes.len(), BTreeSet::new);

    let sccs = tarjan_scc(nodes.len(), &adj);
    let mut result = BTreeMap::new();
    for (node_id, node_ty) in nodes.into_iter().enumerate() {
        let mut indirect = mem::take(&mut pre_indirect[node_id]);
        for &(decl_ctor_idx, field_idx, ref targets) in &field_targets[node_id] {
            if targets.iter().any(|&tid| sccs[node_id] == sccs[tid]) {
                indirect.insert((decl_ctor_idx, field_idx));
            }
        }
        result.insert(node_ty, indirect);
    }
    Ok(result)
}

fn intern_node(
    ty: &TypeRef,
    nodes: &mut Vec<TypeRef>,
    id_of: &mut BTreeMap<TypeRef, usize>,
) -> usize {
    if let Some(&id) = id_of.get(ty) {
        return id;
    }
    let id = nodes.len();
    nodes.push(ty.clone());
    id_of.insert(ty.clone(), id);
    id
}

/// Whether `ty` evaluates directly to a primitive/handle/closure boundary
/// without inspecting any type arguments.
fn is_pointer_boundary(
    catalog: &TypeCatalog<'_>,
    static_ind: &StaticIndirection,
    ty: &TypeRef,
    interner: &mut TypeInterner,
) -> Result<bool, LayoutError> {
    let unwrapped = static_ind.unwrap_transparent(catalog, ty, interner)?;
    match &*unwrapped {
        IrType::Fun { .. } | IrType::Exists { .. } => Ok(true),
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            if vars.is_empty() && constraints.is_empty() {
                is_pointer_boundary(catalog, static_ind, body, interner)
            } else {
                Ok(true)
            }
        }
        IrType::Named { module, name, args } => Ok(matches!(
            catalog.classify(module, name, args.len())?,
            NamedKind::Primitive(_)
        )),
        _ => Ok(false),
    }
}

/// Collects every non-transparent nominal ADT reachable directly by value from
/// `ty` (expanding transparent aliases/unboxed types and traversing tuples and
/// closed records, while stopping at nominal ADTs and pointer/closure
/// boundaries).
fn collect_inst_targets(
    catalog: &TypeCatalog<'_>,
    static_ind: &StaticIndirection,
    ty: &TypeRef,
    interner: &mut TypeInterner,
    out: &mut BTreeSet<TypeRef>,
) -> Result<(), LayoutError> {
    let unwrapped = static_ind.unwrap_transparent(catalog, ty, interner)?;
    match &*unwrapped {
        IrType::Named { module, name, args } => {
            match catalog.classify(module, name, args.len())? {
                NamedKind::Primitive(_) => Ok(()),
                NamedKind::Decl(_) => {
                    out.insert(unwrapped);
                    Ok(())
                }
            }
        }
        IrType::Tuple(items) => {
            for item in items {
                collect_inst_targets(catalog, static_ind, item, interner, out)?;
            }
            Ok(())
        }
        IrType::Record { fields, tail } => {
            if let Some(row) = tail {
                return Err(LayoutError::OpenRecord(row.clone()));
            }
            for (_, field_ty) in fields {
                collect_inst_targets(catalog, static_ind, field_ty, interner, out)?;
            }
            Ok(())
        }
        IrType::Fun { .. } | IrType::Exists { .. } => Ok(()),
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            if vars.is_empty() && constraints.is_empty() {
                collect_inst_targets(catalog, static_ind, body, interner, out)
            } else {
                Ok(())
            }
        }
        IrType::Var(_) | IrType::App { .. } => {
            Err(LayoutError::PolymorphicType(unwrapped.to_string()))
        }
    }
}
