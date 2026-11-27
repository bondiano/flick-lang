//! Scenario action types and oracle report types for hot-swap sessions.

use std::error::Error;
use std::fmt;

use super::types::{MigrationBatch, ModuleDef, SessionOutcomeSummary, SwapRequest};
use crate::syntax::{Name, Path, Term};
use crate::types::Type;

/// Classification of a generated swap action for property coverage and
/// differential assertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapKind {
    /// Replaces a cell with a new implementation of the same scheme.
    SameScheme,
    /// Generalizes a monomorphic scheme (e.g. `Int -> Int` to `forall a. a -> a`).
    GeneralizeScheme,
    /// Narrows an effectful scheme to a narrower/pure effect row.
    NarrowEffect,
    /// Widens a contravariant parameter capture qualifier (e.g. `^∅` to `^c`).
    WidenParamCapture,
    /// Rejected: narrows a polymorphic scheme to a monomorphic one.
    RejectNarrowScheme,
    /// Rejected: widens an effect row (pure -> runtime or adding a service/error).
    RejectWidenEffect,
    /// Rejected: narrows a parameter's region/capture qualifier (`^{s}` -> `^∅`).
    RejectNarrowParamCapture,
    /// Rejected: widens a returned callable's capture qualifier (`^∅` -> `^{s}`).
    RejectWidenCallableCapture,
}

impl SwapKind {
    /// Whether this swap kind is expected to be accepted by the compatibility rule.
    #[must_use]
    pub const fn expected_accepted(self) -> bool {
        matches!(
            self,
            Self::SameScheme
                | Self::GeneralizeScheme
                | Self::NarrowEffect
                | Self::WidenParamCapture
        )
    }
}

/// Classification of a generated state-cell migration action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationKind {
    /// Pure `Old -> New` migration that commits cleanly.
    Valid,
    /// Pure migration that panics (`Die`) and must roll back atomically.
    FailingDie,
    /// Pure migration that diverges (exhausts fuel) and must roll back atomically.
    Diverging,
    /// Multi-cell batch where the first cell succeeds and the second fails;
    /// the entire batch must roll back atomically.
    PartialBatchFailure,
}

impl MigrationKind {
    /// Whether this migration kind is expected to commit.
    #[must_use]
    pub const fn expected_committed(self) -> bool {
        matches!(self, Self::Valid)
    }
}

/// One action in a deterministic hot-swap session script.
#[derive(Clone, Debug)]
pub enum SessionAction {
    /// Advance the active entry computation by up to `steps` small steps.
    Step { steps: usize },
    /// Load an additional module into the live session.
    LoadModule(ModuleDef),
    /// Attempt to hot-swap a global cell.
    Swap {
        request: SwapRequest,
        kind: SwapKind,
    },
    /// Attempt an atomic batch migration over declared state cells.
    Migrate {
        batch: MigrationBatch,
        kind: MigrationKind,
    },
    /// Evaluate a fresh expression against the current globals and store.
    Call { expr: Term, expected_ty: Type },
    /// Read the current value of a declared state cell at its current type.
    ReadState { cell: Path },
}

/// A self-contained, deterministic live session scenario generated from a
/// choice vector, runnable on the reference model and exported for `#990`.
#[derive(Clone, Debug)]
pub struct SessionScenario {
    /// Initial modules loaded before starting `entry`.
    pub initial_modules: Vec<ModuleDef>,
    /// Active entry expression interleaved with swaps and migrations.
    pub entry: Term,
    /// Expected result type of `entry`.
    pub entry_ty: Type,
    /// Sequence of interleaved execution, swap, migration, and call actions.
    pub actions: Vec<SessionAction>,
}

/// Observable record of one executed [`SessionAction`], structured for
/// cross-engine differential comparison (`#990`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionObservation {
    Stepped {
        steps_taken: usize,
        finished: bool,
    },
    ModuleLoaded {
        name: Name,
    },
    SwapAccepted {
        target: Path,
        kind: SwapKind,
    },
    SwapRejected {
        target: Path,
        kind: SwapKind,
        rule: &'static str,
    },
    MigrationCommitted {
        kind: MigrationKind,
        migrated_cells: Vec<(Path, usize, usize)>,
    },
    MigrationRolledBack {
        kind: MigrationKind,
        reason: String,
    },
    CallResult {
        outcome: SessionOutcomeSummary,
    },
    StateRead {
        cell: Path,
        loc: usize,
        outcome: SessionOutcomeSummary,
    },
}

/// Full oracle execution report for a [`SessionScenario`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionOracleReport {
    pub observations: Vec<ActionObservation>,
    pub final_outcome: SessionOutcomeSummary,
    pub state_versions: Vec<(Path, usize, usize)>,
}

/// Failure detected while running a [`SessionScenario`] on the reference model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionOracleError {
    pub step_index: Option<usize>,
    pub message: String,
}

impl fmt::Display for SessionOracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.step_index {
            Some(i) => write!(f, "action #{i}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl Error for SessionOracleError {}
