//! A counted heap for the explicit ownership IR, independent of Rust `Rc`.
//!
//! This executes `dup`/`drop`/`reuse` exactly as written. A successful return
//! transfers an owned token to the caller; [`Heap::finish`] only checks for
//! leaks and never sweeps them. Whole-source-corpus coverage is not yet complete.

use std::collections::{BTreeMap, BTreeSet};
use std::iter;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{error, fmt};

use flick_core::ir::Lit;
use flick_core::ir::memory::{
    self, CellLayout, Exit, Function, Instruction, Local, Operation, ValueRep,
};
use regex_automata::meta::Regex;

pub(crate) mod causes;
pub use causes::{CauseToken, DefectClass, DefectCode, DefectSpec, LanguageExit, Reason};

pub use crate::sandbox::{Capability, LimitKind};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub use crate::stdlib::HostBridge;
use crate::stdlib::cli::{CliMultiplicity, CliSpec};
pub use crate::stdlib::{DRAW, HostMode, HostServices, MILLIS, MONOTONIC_NANOS, NANOS};
use crate::stdlib::{Unmetered, eval_binary, eval_neg, eval_not};

pub(crate) mod patterns;
pub(crate) mod program;
mod record_update;
pub use program::{
    Executed, ExecutionReport, ProgramOutcome, TestCaseResult, execute_main_program,
    execute_main_program_outcome, execute_main_program_outcome_with_args,
    execute_main_program_outcome_with_host, execute_program, execute_program_outcome,
    execute_rooted_main_program_outcome, execute_rooted_main_program_outcome_with_args,
    execute_rooted_main_program_outcome_with_host, execute_sandboxed_main_program_outcome,
    execute_sandboxed_program_outcome, execute_tests_program, execute_tests_program_with_host,
};
pub(crate) use program::{
    SessionHandlerKind, SessionPersistentState, SessionRunSpec, SessionStepStatus,
    SessionSyntheticCodes, SuspendedSessionEval, SuspendedTier0, Tier0StepResult,
    abandon_session_eval, abort_tier0_step, allocate_execution_id, build_session_layer_value,
    execute_main_with_persistent_state, execute_program_with_persistent_state,
    execute_session_apply_unary, execute_session_step, shutdown_persistent_state,
    start_session_eval, start_tier0_step, step_session_eval, step_tier0_n,
};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub use program::{
    execute_main_program_outcome_with_bridge, execute_main_with_bridge_and_inits,
    execute_rooted_main_program_outcome_with_bridge, execute_tests_program_with_bridge,
};

/// An allocation's identity includes its heap and generation. Recycled backing
/// slots increment their generation, so a stale or foreign handle can never
/// silently name a later allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectId {
    heap: usize,
    index: usize,
    generation: usize,
}

/// An owned reference issued only by the counted heap. The private payload
/// prevents reconstructing ownership from a diagnostic [`ObjectId`]. This
/// type intentionally implements neither `Clone` nor `Copy`.
///
/// ```compile_fail,E0451
/// use flick_vm::tier0::{Heap, ObjectToken};
/// let mut heap = Heap::new();
/// let value = heap.construct("Value", vec![]).unwrap();
/// let id = value.object_id().unwrap();
/// let _ = ObjectToken { id };
/// ```
///
/// ```compile_fail,E0277
/// use flick_vm::tier0::{Heap, Value};
/// let mut heap = Heap::new();
/// let Value::Object(token) = heap.construct("Value", vec![]).unwrap() else { unreachable!() };
/// let _ = Clone::clone(&token);
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectToken {
    pub(crate) id: ObjectId,
}

/// A non-owning capability for a cell owned by one execution's root arena.
/// Copying this capability never owns the contents or extends arena life.
/// Its payload remains private so access always goes through the checked
/// execution machinery, and releasing a closed handle never reads storage.
#[derive(Debug, PartialEq, Eq)]
pub struct CellHandle {
    pub(crate) id: ObjectId,
    pub(crate) execution: usize,
}

/// Nonowning lexical owner identity; validated against one running machine.
#[derive(Debug, PartialEq, Eq)]
pub struct OwnerHandle {
    pub(crate) heap: usize,
    pub(crate) execution: usize,
    pub(crate) index: usize,
}

/// Nonowning arena identity. Retention does not keep its storage alive.
#[derive(Debug, PartialEq, Eq)]
pub struct ArenaHandle {
    pub(crate) id: ObjectId,
    pub(crate) execution: usize,
}

/// Nonowning index into an arena. Dropping a stale view reads no storage.
#[derive(Debug, PartialEq, Eq)]
pub struct ArenaView {
    pub(crate) arena: ArenaHandle,
    pub(crate) index: usize,
}

/// Nonowning handle to a frame-owned mutable local variable (`let mut`).
#[derive(Debug, PartialEq, Eq)]
pub struct VarHandle {
    pub(crate) id: ObjectId,
    pub(crate) execution: usize,
}

/// Nonowning handle to a root-owned single-flight `Lazy` cell.
#[derive(Debug, PartialEq, Eq)]
pub struct LazyHandle {
    pub(crate) id: ObjectId,
    pub(crate) execution: usize,
}

/// Nonowning handle to a frame-owned `Seq`/list iteration cursor.
#[derive(Debug, PartialEq, Eq)]
pub struct IteratorHandle {
    pub(crate) id: ObjectId,
    pub(crate) execution: usize,
}

/// An owned memory-IR token. Inspecting an [`ObjectId`] does not duplicate
/// ownership: a second owned handle must be obtained with [`Heap::dup`].
///
/// A diagnostic identity cannot be turned into an owned token. In
/// particular, moving a child into its parent leaves no public route to
/// forge a second child token for a reuse operation:
///
/// ```compile_fail,E0308
/// use flick_vm::tier0::{Heap, Value};
/// let mut heap = Heap::new();
/// let child = heap.construct("Child", vec![]).unwrap();
/// let child_id = child.object_id().unwrap();
/// let parent = heap.construct("Old", vec![child]).unwrap();
/// let forged = Value::Object(child_id);
/// let _ = heap.reuse(parent, "New", vec![forged]);
/// ```
#[derive(Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i32),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float(f64),
    Float32(f32),
    Object(ObjectToken),
    Cell(CellHandle),
    Owner(OwnerHandle),
    Arena(ArenaHandle),
    ArenaView(ArenaView),
    Var(VarHandle),
    Lazy(LazyHandle),
    Iterator(IteratorHandle),
    /// Unboxed aggregate value: no heap cell, no `ObjectId`, no refcount header.
    /// Owns its ordinary fields and optional dictionary evidence suffix inline.
    Aggregate {
        tag: String,
        ordinary_fields: usize,
        fields: Vec<Value>,
    },
    /// Immediate sentinel produced by `DropReuse` when the operand was shared
    /// or non-reusable; consumes zero heap storage and causes `Reuse*` to allocate.
    ReuseNull,
}

impl Value {
    pub fn is_aggregate(&self) -> bool {
        matches!(self, Self::Aggregate { .. })
    }

    pub fn object_id(&self) -> Option<ObjectId> {
        match self {
            Self::Object(token) => Some(token.id),
            _ => None,
        }
    }

    /// Diagnostic identity of cell storage, not an owned object reference.
    pub fn cell_id(&self) -> Option<ObjectId> {
        match self {
            Self::Cell(cell) => Some(cell.id),
            _ => None,
        }
    }

    /// Copies representation, not ownership. Used only immediately before
    /// incrementing the reference count, or for immediate values.
    pub(crate) fn representation(&self) -> Value {
        match self {
            Self::Unit => Self::Unit,
            Self::Bool(value) => Self::Bool(*value),
            Self::Int(value) => Self::Int(*value),
            Self::Int8(value) => Self::Int8(*value),
            Self::Int16(value) => Self::Int16(*value),
            Self::Int64(value) => Self::Int64(*value),
            Self::UInt8(value) => Self::UInt8(*value),
            Self::UInt16(value) => Self::UInt16(*value),
            Self::UInt32(value) => Self::UInt32(*value),
            Self::UInt64(value) => Self::UInt64(*value),
            Self::Float(value) => Self::Float(*value),
            Self::Float32(value) => Self::Float32(*value),
            Self::Object(token) => Self::Object(ObjectToken { id: token.id }),
            Self::Owner(owner) => Self::Owner(OwnerHandle {
                heap: owner.heap,
                execution: owner.execution,
                index: owner.index,
            }),
            Self::Arena(arena) => Self::Arena(ArenaHandle {
                id: arena.id,
                execution: arena.execution,
            }),
            Self::ArenaView(view) => Self::ArenaView(ArenaView {
                arena: ArenaHandle {
                    id: view.arena.id,
                    execution: view.arena.execution,
                },
                index: view.index,
            }),
            Self::Cell(cell) => Self::Cell(CellHandle {
                id: cell.id,
                execution: cell.execution,
            }),
            Self::Var(var) => Self::Var(VarHandle {
                id: var.id,
                execution: var.execution,
            }),
            Self::Lazy(lazy) => Self::Lazy(LazyHandle {
                id: lazy.id,
                execution: lazy.execution,
            }),
            Self::Iterator(iterator) => Self::Iterator(IteratorHandle {
                id: iterator.id,
                execution: iterator.execution,
            }),
            Self::Aggregate {
                tag,
                ordinary_fields,
                fields,
            } => Self::Aggregate {
                tag: tag.clone(),
                ordinary_fields: *ordinary_fields,
                fields: fields.iter().map(Self::representation).collect(),
            },
            Self::ReuseNull => Self::ReuseNull,
        }
    }
}

/// Heap-object operations only; immediate token copies/releases do not
/// increment `dups`/`drops`. Moving a token does not change its count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub allocations: usize,
    pub frees: usize,
    pub dups: usize,
    pub drops: usize,
    pub reuses: usize,
    /// Number of reuse/update sites that fell back to fresh allocation
    /// (shared object, incompatible layout, `ReuseNull`, or reuse disabled).
    pub reuse_fallbacks: usize,
    pub live_objects: usize,
    pub peak_live_objects: usize,
    pub cells_allocated: usize,
    pub cells_freed: usize,
    /// Number of root-arena `Ref` cells still holding a counted heap object
    /// when root-arena teardown detached their slots.
    pub cells_detached_on_teardown: usize,
    pub arenas_allocated: usize,
    pub arenas_freed: usize,
}

include!("tier0/errors.rs");
include!("tier0/representation.rs");
include!("tier0/heap_limits.rs");
include!("tier0/allocation.rs");
include!("tier0/aggregates.rs");
include!("tier0/ownership.rs");
include!("tier0/reuse.rs");
include!("tier0/standalone.rs");
include!("tier0/image_support.rs");
include!("tier0/heap_tests.rs");
#[cfg(test)]
include!("tier0/aggregate_tests.rs");
