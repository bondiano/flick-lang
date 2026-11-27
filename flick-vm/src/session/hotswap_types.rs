//! Public types for live VM hot swap, invalidation hooks, typed state
//! migrations, and steppable two-version execution on [`super::VmSession`].

use std::error::Error;
use std::fmt;

use flick_core::iface::Interface;
use flick_core::ir::memory::{MemorySpan, Name, Program};
use flick_core::types::Type;
use flick_core::types::hot_swap::SchemeCompatibilityError;

use crate::sandbox::{BytecodeVerifyError, Capability, ResourceLimits};
use crate::tier0::ObjectId;

use super::EvalOutcome;

/// Execution tier selected for a [`super::VmSession`] evaluation or stepped task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionTier {
    /// Reference-counted memory-IR tree/block interpreter (full host/FFI support).
    #[default]
    Tier0,
    /// Fused, verified register bytecode VM with cached prepared images.
    Tier1,
}

/// Hook notified whenever a live hot swap commits so prepared code caches and
/// future native-compiled regions (`#982`/`#983`) invalidate affected bindings.
pub trait RegionInvalidator: Send {
    /// Invalidates any prepared or compiled region depending on `bindings`
    /// (containing every swapped binding and all of its transitive dependents).
    fn invalidate(&mut self, bindings: &[Name]);
}

/// One top-level binding targeted by a [`SwapDefinitionsRequest`].
#[derive(Debug, Clone)]
pub struct SwapBinding {
    /// Fully qualified module and binding name (`Module.name`).
    pub name: Name,
    /// Optional proposed replacement scheme, checked via exact scheme
    /// subsumption (`σ_new ≤ σ_old`) against the registered old scheme.
    pub new_scheme: Option<Type>,
}

/// Request to atomically hot-swap one or more top-level definitions.
#[derive(Debug, Clone)]
pub struct SwapDefinitionsRequest {
    /// Compiled memory-IR program supplying the replacement global initializers
    /// and any helper functions they reference.
    pub incoming: Program,
    /// Bindings whose global cells are replaced.
    pub targets: Vec<SwapBinding>,
    /// Optional updated module interfaces installed atomically on commit.
    pub updated_interfaces: Vec<Interface>,
    /// Whether to eagerly evaluate replacement initializers before commit so
    /// initialization faults roll back atomically.
    pub eager_initialize: bool,
}

impl SwapDefinitionsRequest {
    /// Creates a swap request that eagerly initializes and verifies `targets`.
    #[must_use]
    pub fn new(incoming: Program, targets: Vec<SwapBinding>) -> Self {
        Self {
            incoming,
            targets,
            updated_interfaces: Vec::new(),
            eager_initialize: true,
        }
    }

    /// Attaches updated module interfaces to install atomically on commit.
    #[must_use]
    pub fn with_interfaces(mut self, interfaces: Vec<Interface>) -> Self {
        self.updated_interfaces = interfaces;
        self
    }

    /// Defers global initialization until the binding's next dynamic read.
    #[must_use]
    pub const fn deferred(mut self) -> Self {
        self.eager_initialize = false;
        self
    }
}

/// Declaration of a live state cell eligible for typed `Old -> New` migrations.
#[derive(Debug, Clone)]
pub struct StateCellSpec {
    /// Top-level binding holding the state cell (either a `Ref` cell or value).
    pub name: Name,
    /// Current declared type of the value stored in the cell.
    pub ty: Type,
    /// `true` when the global holds a mutable `Ref` (`Value::Cell`), `false`
    /// when the global cell stores the value directly.
    pub is_ref: bool,
}

/// One pure `Old -> New` migration step for a declared state cell.
#[derive(Debug, Clone)]
pub struct StateMigration {
    /// Declared state cell to migrate.
    pub cell: Name,
    /// Expected current type of the cell.
    pub old_ty: Type,
    /// Target type of the cell after migration.
    pub new_ty: Type,
    /// Optional Flick type scheme of `migrate_entry`, verified to be a pure
    /// function subsuming `old_ty -> new_ty`.
    pub migrate_scheme: Option<Type>,
    /// Entry global name inside `migrate_program` implementing `Old -> New`.
    pub migrate_entry: Name,
    /// Pure memory-IR program containing `migrate_entry` and its helpers.
    pub migrate_program: Program,
}

/// Atomic batch of typed state-cell migrations executed under pure sandbox limits.
#[derive(Debug, Clone)]
pub struct MigrationBatchRequest {
    /// Ordered migrations applied together; any failure rolls back all cells.
    pub migrations: Vec<StateMigration>,
    /// Resource limits (fuel, heap bytes, wall time, stack depth) applied to
    /// every migration evaluation. Migrations always run with pure capabilities.
    pub limits: ResourceLimits,
}

impl MigrationBatchRequest {
    /// Creates a migration batch with an explicit fuel budget.
    #[must_use]
    pub fn with_fuel(migrations: Vec<StateMigration>, fuel: u64) -> Self {
        let mut limits = ResourceLimits::default();
        limits.fuel = Some(fuel);
        Self { migrations, limits }
    }
}

/// Metadata for one state cell migrated during a committed swap/migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedCellReport {
    pub cell: Name,
    pub version: usize,
    pub old_cell_id: Option<ObjectId>,
    pub new_cell_id: Option<ObjectId>,
}

/// Summary returned when a live hot swap and/or state migration commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapReport {
    /// Bindings whose global cells were updated.
    pub swapped_bindings: Vec<Name>,
    /// All affected bindings (swapped + transitive dependents) invalidated in
    /// tier-1 caches and reported to every registered [`RegionInvalidator`].
    pub invalidated_bindings: Vec<Name>,
    /// State cells migrated atomically during this transaction.
    pub migrated_cells: Vec<MigratedCellReport>,
    /// `true` when replacement bytecode passed structural verification.
    pub bytecode_verified: bool,
}

/// Identifier for a steppable computation suspended on [`super::VmSession`].
pub type StepTaskId = u64;

/// Result of advancing a steppable computation on [`super::VmSession`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepEvalOutcome {
    /// Computation is still active and paused at a safe point.
    Running { steps_taken: usize },
    /// Computation finished and released its frame roots.
    Finished(EvalOutcome),
}

/// Structured error returned when a hot swap or state migration is refused
/// atomically with no changes to live code, cells, or heap state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotSwapError {
    /// A target global binding is not loaded in the live session or incoming IR.
    UnknownBinding(Name),
    /// A migration targeted a binding not declared as a live state cell.
    UnknownStateCell(Name),
    /// A suspended step task identifier does not exist.
    UnknownTask(StepTaskId),
    /// Replacement scheme failed `σ_new ≤ σ_old`; lists every transitively
    /// dependent binding by `(module, binding)`.
    Incompatible {
        target: Name,
        compatibility: Box<SchemeCompatibilityError>,
        dependents: Vec<Name>,
    },
    /// Replacement or migration IR failed ownership validation or tier-1 prep.
    InvalidIr(String),
    /// Replacement bytecode failed structural verification (`#992`).
    BytecodeVerification(BytecodeVerifyError),
    /// Replacement or migration required a capability disallowed by the sandbox.
    CapabilityDenied {
        capability: Capability,
        binding: String,
        span: Option<MemorySpan>,
    },
    /// Declared state cell type did not match the migration's expected `old_ty`.
    StateTypeMismatch {
        cell: Name,
        expected_old: String,
        actual: String,
    },
    /// Migration function scheme failed pure `Old -> New` type checking.
    MigrationTypeError {
        cell: Name,
        compatibility: Box<SchemeCompatibilityError>,
    },
    /// Migration raised, defected, exhausted fuel/limits, or attempted an
    /// effectful/mutating side effect; entire batch rolled back atomically.
    MigrationFailed { cell: Name, reason: String },
    /// Eager initialization of a swapped binding failed; swap rolled back.
    InitializationFailed { target: Name, reason: String },
    /// Internal VM error during swap/migration orchestration.
    Vm(String),
}

impl fmt::Display for HotSwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBinding(name) => {
                write!(f, "unknown binding `{}.{}`", name.module, name.name)
            }
            Self::UnknownStateCell(name) => {
                write!(
                    f,
                    "unknown declared state cell `{}.{}`",
                    name.module, name.name
                )
            }
            Self::UnknownTask(id) => write!(f, "unknown stepped task #{id}"),
            Self::Incompatible {
                target,
                compatibility,
                dependents,
            } => {
                write!(
                    f,
                    "incompatible swap for `{}.{}`: {compatibility}",
                    target.module, target.name
                )?;
                if !dependents.is_empty() {
                    let list: Vec<String> = dependents
                        .iter()
                        .map(|d| format!("{}.{}", d.module, d.name))
                        .collect();
                    write!(f, " (transitive dependents: {})", list.join(", "))?;
                }
                Ok(())
            }
            Self::InvalidIr(msg) => write!(f, "invalid memory IR: {msg}"),
            Self::BytecodeVerification(err) => write!(f, "bytecode verification failed: {err}"),
            Self::CapabilityDenied {
                capability,
                binding,
                ..
            } => write!(f, "capability `{capability}` denied for `{binding}`"),
            Self::StateTypeMismatch {
                cell,
                expected_old,
                actual,
            } => write!(
                f,
                "state cell `{}.{}` has type `{actual}`, expected `{expected_old}`",
                cell.module, cell.name
            ),
            Self::MigrationTypeError {
                cell,
                compatibility,
            } => write!(
                f,
                "invalid migration type for `{}.{}`: {compatibility}",
                cell.module, cell.name
            ),
            Self::MigrationFailed { cell, reason } => write!(
                f,
                "migration for `{}.{}` failed (rolled back): {reason}",
                cell.module, cell.name
            ),
            Self::InitializationFailed { target, reason } => write!(
                f,
                "initializer for swapped `{}.{}` failed (rolled back): {reason}",
                target.module, target.name
            ),
            Self::Vm(msg) => write!(f, "VM hot-swap error: {msg}"),
        }
    }
}

impl Error for HotSwapError {}
