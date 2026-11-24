//! Whole-program shape collection and transitive shape closure for Go codegen.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use flick_backend_support::reach::{self, Key};
use flick_ir::{ItemKind, Module};

use super::adt::{AdtOps, AdtShape, AdtSlot};
use super::record::{RecordShape, RecordSlot};
use super::shape::{Shape, shape};
use super::{Input, direct, go_type, names, own};

pub(super) fn collect_shapes(
    input: &Input,
    modules: &[&Module],
    decls: &go_type::TypeDecls<'_>,
    reached: &HashSet<(usize, usize)>,
) -> HashMap<Key, Shape> {
    let mut shapes = HashMap::new();
    for &(m, i) in reached {
        let item = &modules[m].items[i];
        for name in reach::defined(item) {
            let held = shape(input, decls, &modules[m].name, &item.kind);
            shapes.insert((modules[m].name.clone(), name), held);
        }
        shapes.extend(direct::shapes(input, decls, &modules[m].name, &item.kind));
    }
    shapes
}

pub(super) fn has_initializer(
    input: &Input,
    decls: &go_type::TypeDecls<'_>,
    item: &ItemKind,
) -> bool {
    match item {
        // the shape of a value does not depend on its module
        ItemKind::Value { .. } => matches!(shape(input, decls, "", item), Shape::Var),
        ItemKind::Instance { info, .. } => info.context.is_empty(),
        ItemKind::Destructure { .. } => true,
        ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => false,
    }
}

/// Transitively closes reached closed ADTs, closed records, their derived op
/// requirements, and the erased `own_types.go` constructor declarations needed
/// by `Box_A_*` / `As_A_*`.
pub(super) fn close_shapes(
    adt_shapes: &mut BTreeMap<AdtShape, AdtOps>,
    record_shapes: &mut BTreeSet<RecordShape>,
    owned: &mut own::Owned,
    records: &own::Records,
    fields: &own::CtorFields,
) {
    loop {
        let mut changed = false;
        for (_, held) in owned.values() {
            if let own::Held::Record(shape) = held {
                changed |= record_shapes.insert(shape.clone());
            }
        }
        for rec in record_shapes.iter() {
            for (_, slot) in &rec.fields {
                if let RecordSlot::Adt(adt) = slot {
                    let entry = adt_shapes.entry(adt.clone()).or_insert_with(|| {
                        changed = true;
                        AdtOps::default()
                    });
                    if !adt.is_go_comparable() {
                        changed |= entry.merge(AdtOps::EQ_WITH);
                    }
                }
            }
        }
        let snapshot: Vec<(AdtShape, AdtOps)> =
            adt_shapes.iter().map(|(s, o)| (s.clone(), *o)).collect();
        for (shape, ops) in snapshot {
            for slot in &shape.slots {
                let mut propagate = |inner: &AdtShape| -> bool {
                    let mut prop = AdtOps::default();
                    if ops.has(AdtOps::EQ_WITH) && !inner.is_go_comparable() {
                        prop = prop.with(AdtOps::EQ_WITH);
                    }
                    if ops.has(AdtOps::ORD) {
                        prop = prop.with(AdtOps::ORD);
                    }
                    if ops.has(AdtOps::HASH) {
                        prop = prop.with(AdtOps::HASH);
                    }
                    if ops.has(AdtOps::SHOW) {
                        prop = prop.with(AdtOps::SHOW);
                    }
                    let mut inserted = false;
                    let entry = adt_shapes.entry(inner.clone()).or_insert_with(|| {
                        inserted = true;
                        AdtOps::default()
                    });
                    inserted | entry.merge(prop)
                };
                match slot {
                    AdtSlot::Record(rec) => {
                        changed |= record_shapes.insert(rec.clone());
                        for (_, rslot) in &rec.fields {
                            if let RecordSlot::Adt(inner) = rslot {
                                changed |= propagate(inner);
                            }
                        }
                    }
                    AdtSlot::Adt(inner) => changed |= propagate(inner),
                    AdtSlot::Scalar(_) | AdtSlot::Opt(_) | AdtSlot::Unit => {}
                }
            }
            if !shape.is_result {
                for variant in &shape.variants {
                    let own_ty = names::own_type(&shape.module, &variant.name);
                    if let Entry::Vacant(e) = owned.entry(own_ty) {
                        let held = if variant.fields.is_empty() {
                            own::Held::Zero
                        } else if variant.fields.len() == 1
                            && let Some(rec) =
                                records.get(&(shape.module.clone(), variant.name.clone()))
                        {
                            own::Held::Record(rec.clone())
                        } else {
                            let slots = fields
                                .get(&(
                                    shape.module.clone(),
                                    shape.name.clone(),
                                    variant.name.clone(),
                                ))
                                .filter(|s| s.len() == variant.fields.len())
                                .cloned()
                                .unwrap_or_else(|| vec![None; variant.fields.len()]);
                            own::Held::Fields(slots)
                        };
                        e.insert((names::string(&variant.tag), held));
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}
