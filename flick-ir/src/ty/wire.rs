//! Wire interning of [`TypeRef`] inside a serialized [`Program`](crate::Program).
//!
//! Each unique [`IrType`] reachable from a program's modules is stored once in
//! the program's top-level `types` table in bottom-up DAG order (every child
//! index is strictly smaller than its parent's index). Inside `modules`, each
//! `TypeRef` serializes as its `u32` index in that table. Outside a `Program`,
//! `TypeRef` serializes inline as [`IrType`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};

use super::{Constraint, IrType, TypeRef};
use crate::{Module, Stage};

/// Wire-encoded form of [`IrType`] where every child type is a `u32` index
/// into the enclosing program's `types` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WireIrType {
    Named {
        module: String,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<u32>,
    },
    Var(String),
    App {
        head: u32,
        args: Vec<u32>,
    },
    Fun {
        param: u32,
        stage: Stage,
        result: u32,
    },
    Tuple(Vec<u32>),
    Record {
        fields: Vec<(String, u32)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<String>,
    },
    Forall {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        vars: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<WireConstraint>,
        body: u32,
    },
    Exists {
        binder: String,
        contract: u32,
        body: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireConstraint {
    pub module: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<u32>,
}

enum DecodeState {
    Ready(Vec<TypeRef>),
    Deferred {
        by_id: BTreeMap<u32, TypeRef>,
        by_ptr: BTreeMap<usize, u32>,
    },
}

thread_local! {
    static ENCODE_POOL: RefCell<Option<BTreeMap<TypeRef, u32>>> = const { RefCell::new(None) };
    static DECODE_POOL: RefCell<Option<DecodeState>> = const { RefCell::new(None) };
}

/// Builds the deterministic bottom-up wire table and lookup map for all
/// [`TypeRef`]s reachable from `modules`.
pub(crate) fn collect_table(modules: &[Module]) -> (Vec<WireIrType>, BTreeMap<TypeRef, u32>) {
    let mut table = Vec::new();
    let mut ids = BTreeMap::new();
    for module in modules {
        module.each_type_ref(&mut |ty| {
            intern_wire(ty, &mut table, &mut ids);
        });
    }
    (table, ids)
}

fn intern_wire(ty: &TypeRef, table: &mut Vec<WireIrType>, ids: &mut BTreeMap<TypeRef, u32>) -> u32 {
    if let Some(&id) = ids.get(ty) {
        return id;
    }
    let wire = match &**ty {
        IrType::Named { module, name, args } => WireIrType::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|a| intern_wire(a, table, ids)).collect(),
        },
        IrType::Var(name) => WireIrType::Var(name.clone()),
        IrType::App { head, args } => WireIrType::App {
            head: intern_wire(head, table, ids),
            args: args.iter().map(|a| intern_wire(a, table, ids)).collect(),
        },
        IrType::Fun {
            param,
            stage,
            result,
        } => WireIrType::Fun {
            param: intern_wire(param, table, ids),
            stage: *stage,
            result: intern_wire(result, table, ids),
        },
        IrType::Tuple(items) => {
            WireIrType::Tuple(items.iter().map(|a| intern_wire(a, table, ids)).collect())
        }
        IrType::Record { fields, tail } => WireIrType::Record {
            fields: fields
                .iter()
                .map(|(l, t)| (l.clone(), intern_wire(t, table, ids)))
                .collect(),
            tail: tail.clone(),
        },
        IrType::Forall {
            vars,
            constraints,
            body,
        } => WireIrType::Forall {
            vars: vars.clone(),
            constraints: constraints
                .iter()
                .map(|c| WireConstraint {
                    module: c.module.clone(),
                    class: c.class.clone(),
                    args: c.args.iter().map(|a| intern_wire(a, table, ids)).collect(),
                })
                .collect(),
            body: intern_wire(body, table, ids),
        },
        IrType::Exists {
            binder,
            contract,
            body,
        } => WireIrType::Exists {
            binder: binder.clone(),
            contract: intern_wire(contract, table, ids),
            body: intern_wire(body, table, ids),
        },
    };
    let id = u32::try_from(table.len()).expect("fewer than 2^32 distinct IR types");
    table.push(wire);
    ids.insert(ty.clone(), id);
    id
}

/// Decodes a bottom-up `WireIrType` table into interned [`TypeRef`]s, rejecting
/// any out-of-bounds or forward (cyclic) child reference.
pub(crate) fn decode_table<E: de::Error>(wire_types: Vec<WireIrType>) -> Result<Vec<TypeRef>, E> {
    let mut out: Vec<TypeRef> = Vec::with_capacity(wire_types.len());
    let get = |out: &[TypeRef], id: u32, current: usize| -> Result<TypeRef, E> {
        let idx = id as usize;
        if idx >= current {
            return Err(E::custom(format!(
                "IR type index {id} out of range (must be < {current})"
            )));
        }
        Ok(out[idx].clone())
    };
    for (i, entry) in wire_types.into_iter().enumerate() {
        let ir = match entry {
            WireIrType::Named { module, name, args } => IrType::Named {
                module,
                name,
                args: args
                    .into_iter()
                    .map(|a| get(&out, a, i))
                    .collect::<Result<_, _>>()?,
            },
            WireIrType::Var(name) => IrType::Var(name),
            WireIrType::App { head, args } => IrType::App {
                head: get(&out, head, i)?,
                args: args
                    .into_iter()
                    .map(|a| get(&out, a, i))
                    .collect::<Result<_, _>>()?,
            },
            WireIrType::Fun {
                param,
                stage,
                result,
            } => IrType::Fun {
                param: get(&out, param, i)?,
                stage,
                result: get(&out, result, i)?,
            },
            WireIrType::Tuple(items) => IrType::Tuple(
                items
                    .into_iter()
                    .map(|a| get(&out, a, i))
                    .collect::<Result<_, _>>()?,
            ),
            WireIrType::Record { fields, tail } => IrType::Record {
                fields: fields
                    .into_iter()
                    .map(|(l, t)| Ok((l, get(&out, t, i)?)))
                    .collect::<Result<_, _>>()?,
                tail,
            },
            WireIrType::Forall {
                vars,
                constraints,
                body,
            } => IrType::Forall {
                vars,
                constraints: constraints
                    .into_iter()
                    .map(|c| {
                        Ok(Constraint {
                            module: c.module,
                            class: c.class,
                            args: c
                                .args
                                .into_iter()
                                .map(|a| get(&out, a, i))
                                .collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
                body: get(&out, body, i)?,
            },
            WireIrType::Exists {
                binder,
                contract,
                body,
            } => IrType::Exists {
                binder,
                contract: get(&out, contract, i)?,
                body: get(&out, body, i)?,
            },
        };
        out.push(TypeRef::new(ir));
    }
    Ok(out)
}

/// RAII guard installing an encode table on the current thread.
pub(crate) struct EncodeGuard {
    prev: Option<BTreeMap<TypeRef, u32>>,
}

impl EncodeGuard {
    pub(crate) fn enter(ids: BTreeMap<TypeRef, u32>) -> EncodeGuard {
        let prev = ENCODE_POOL.with(|cell| cell.borrow_mut().replace(ids));
        EncodeGuard { prev }
    }
}

impl Drop for EncodeGuard {
    fn drop(&mut self) {
        ENCODE_POOL.with(|cell| {
            *cell.borrow_mut() = self.prev.take();
        });
    }
}

/// RAII guard installing a decode table (or deferred placeholder table for
/// arbitrary map key ordering) on the current thread.
pub(crate) struct DecodeGuard {
    prev: Option<DecodeState>,
}

impl DecodeGuard {
    pub(crate) fn enter_deferred() -> DecodeGuard {
        let state = DecodeState::Deferred {
            by_id: BTreeMap::new(),
            by_ptr: BTreeMap::new(),
        };
        let prev = DECODE_POOL.with(|cell| cell.borrow_mut().replace(state));
        DecodeGuard { prev }
    }

    /// Installs the decoded `types` table. When `modules` has not yet been
    /// decoded (standard `"types"`-before-`"modules"` order), switches directly
    /// to immediate resolution and returns `None`; otherwise returns `Some(table)`
    /// for post-pass placeholder resolution.
    pub(crate) fn set_ready(table: Vec<TypeRef>) -> Option<Vec<TypeRef>> {
        DECODE_POOL.with(|cell| {
            let mut guard = cell.borrow_mut();
            let empty_deferred = matches!(
                guard.as_ref(),
                Some(DecodeState::Deferred { by_ptr, .. }) if by_ptr.is_empty()
            );
            if empty_deferred {
                *guard = Some(DecodeState::Ready(table));
                None
            } else {
                Some(table)
            }
        })
    }

    pub(crate) fn take_deferred() -> BTreeMap<usize, u32> {
        DECODE_POOL.with(|cell| match cell.borrow_mut().as_mut() {
            Some(DecodeState::Deferred { by_ptr, .. }) => mem::take(by_ptr),
            _ => BTreeMap::new(),
        })
    }
}

impl Drop for DecodeGuard {
    fn drop(&mut self) {
        DECODE_POOL.with(|cell| {
            *cell.borrow_mut() = self.prev.take();
        });
    }
}

/// Replaces deferred placeholder [`TypeRef`]s in `modules` using `table` when
/// `"modules"` appeared before `"types"` on the wire.
pub(crate) fn resolve_deferred_modules<E: de::Error>(
    modules: &mut [Module],
    deferred: &BTreeMap<usize, u32>,
    table: &[TypeRef],
) -> Result<(), E> {
    if deferred.is_empty() {
        return Ok(());
    }
    let mut bad_id: Option<u32> = None;
    for module in modules {
        module.each_type_ref_mut(&mut |ty| {
            if bad_id.is_some() {
                return;
            }
            let ptr = Arc::as_ptr(&ty.0) as usize;
            if let Some(&id) = deferred.get(&ptr) {
                match table.get(id as usize) {
                    Some(resolved) => *ty = resolved.clone(),
                    None => bad_id = Some(id),
                }
            }
        });
    }
    if let Some(id) = bad_id {
        return Err(E::custom(format!(
            "IR type index {id} out of range (table has {} entries)",
            table.len()
        )));
    }
    Ok(())
}

pub(super) fn serialize_ref<S: Serializer>(ty: &TypeRef, serializer: S) -> Result<S::Ok, S::Error> {
    let pooled = ENCODE_POOL.with(|cell| cell.borrow().as_ref().map(|ids| ids.get(ty).copied()));
    match pooled {
        Some(Some(id)) => serializer.serialize_u32(id),
        Some(None) => Err(ser::Error::custom(
            "IR TypeRef was not collected into the Program type table",
        )),
        None => ty.0.serialize(serializer),
    }
}

pub(super) fn deserialize_ref<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<TypeRef, D::Error> {
    let has_pool = DECODE_POOL.with(|cell| cell.borrow().is_some());
    if has_pool {
        let id = u32::deserialize(deserializer)?;
        DECODE_POOL.with(|cell| {
            let mut guard = cell.borrow_mut();
            match guard.as_mut().expect("checked pool present") {
                DecodeState::Ready(table) => table.get(id as usize).cloned().ok_or_else(|| {
                    de::Error::custom(format!(
                        "IR type index {id} out of range (table has {} entries)",
                        table.len()
                    ))
                }),
                DecodeState::Deferred { by_id, by_ptr } => {
                    if let Some(existing) = by_id.get(&id) {
                        return Ok(existing.clone());
                    }
                    // Each deferred index gets its own distinct Arc allocation so
                    // pointer identity uniquely identifies it during fixup and can
                    // never collide with any real `IrType`.
                    let sentinel = TypeRef::new(IrType::Var(String::new()));
                    let ptr = Arc::as_ptr(&sentinel.0) as usize;
                    by_ptr.insert(ptr, id);
                    by_id.insert(id, sentinel.clone());
                    Ok(sentinel)
                }
            }
        })
    } else {
        Ok(TypeRef::new(IrType::deserialize(deserializer)?))
    }
}
