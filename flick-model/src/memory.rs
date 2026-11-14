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
use flick_core::ir::memory::{self, Exit, Function, Instruction, Local, Operation};

mod causes;
pub use causes::{CauseToken, DefectClass, DefectCode, DefectSpec, LanguageExit, Reason};

mod patterns;
mod primitives;
mod program;
pub use program::{
    Executed, ExecutionReport, ProgramOutcome, execute_main_program, execute_main_program_outcome,
    execute_program, execute_program_outcome,
};

/// An allocation's identity includes its heap. Slots are never recycled, so
/// a stale or foreign handle cannot silently name a later allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectId {
    heap: usize,
    index: usize,
}

/// An owned reference issued only by the counted heap. The private payload
/// prevents reconstructing ownership from a diagnostic [`ObjectId`]. This
/// type intentionally implements neither `Clone` nor `Copy`.
///
/// ```compile_fail,E0451
/// use flick_model::memory::{Heap, ObjectToken};
/// let mut heap = Heap::new();
/// let value = heap.construct("Value", vec![]).unwrap();
/// let id = value.object_id().unwrap();
/// let _ = ObjectToken { id };
/// ```
///
/// ```compile_fail,E0277
/// use flick_model::memory::{Heap, Value};
/// let mut heap = Heap::new();
/// let Value::Object(token) = heap.construct("Value", vec![]).unwrap() else { unreachable!() };
/// let _ = Clone::clone(&token);
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectToken {
    id: ObjectId,
}

/// A non-owning capability for a cell owned by one execution's root arena.
/// Copying this capability never owns the contents or extends arena life.
/// Its payload remains private so access always goes through the checked
/// execution machinery, and releasing a closed handle never reads storage.
#[derive(Debug, PartialEq, Eq)]
pub struct CellHandle {
    id: ObjectId,
    execution: usize,
}

/// Nonowning lexical owner identity; validated against one running machine.
#[derive(Debug, PartialEq, Eq)]
pub struct OwnerHandle {
    heap: usize,
    execution: usize,
    index: usize,
}

/// Nonowning arena identity. Retention does not keep its storage alive.
#[derive(Debug, PartialEq, Eq)]
pub struct ArenaHandle {
    id: ObjectId,
    execution: usize,
}

/// Nonowning index into an arena. Dropping a stale view reads no storage.
#[derive(Debug, PartialEq, Eq)]
pub struct ArenaView {
    arena: ArenaHandle,
    index: usize,
}

/// An owned memory-IR token. Inspecting an [`ObjectId`] does not duplicate
/// ownership: a second owned handle must be obtained with [`Heap::dup`].
///
/// A diagnostic identity cannot be turned into an owned token. In
/// particular, moving a child into its parent leaves no public route to
/// forge a second child token for a reuse operation:
///
/// ```compile_fail,E0308
/// use flick_model::memory::{Heap, Value};
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
    Float(f64),
    Object(ObjectToken),
    Cell(CellHandle),
    Owner(OwnerHandle),
    Arena(ArenaHandle),
    ArenaView(ArenaView),
}

impl Value {
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
    fn representation(&self) -> Value {
        match self {
            Self::Unit => Self::Unit,
            Self::Bool(value) => Self::Bool(*value),
            Self::Int(value) => Self::Int(*value),
            Self::Float(value) => Self::Float(*value),
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
    pub live_objects: usize,
    pub peak_live_objects: usize,
    pub cells_allocated: usize,
    pub cells_freed: usize,
    pub arenas_allocated: usize,
    pub arenas_freed: usize,
}

include!("memory/errors.rs");
include!("memory/representation.rs");
include!("memory/allocation.rs");
include!("memory/ownership.rs");
include!("memory/standalone.rs");
include!("memory/heap_tests.rs");
