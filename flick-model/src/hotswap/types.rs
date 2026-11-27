//! Public data types, error types, snapshots, and rendering helpers for
//! [`LiveSession`](super::session::LiveSession).

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::check::TypeError;
use crate::check::compat::CompatibilityError;
use crate::eval::Outcome;
use crate::syntax::{Lit, Name, Path, Term};
use crate::types::{Scheme, Type};

/// A global value declaration loaded into a live session.
#[derive(Clone, Debug)]
pub struct GlobalDecl {
    pub path: Path,
    pub scheme: Scheme,
    pub value: Term,
}

/// A declared mutable state cell loaded into a live session.
#[derive(Clone, Debug)]
pub struct StateCellDecl {
    pub path: Path,
    pub ty: Type,
    pub init: Term,
}

/// A module unit loaded into a [`LiveSession`].
#[derive(Clone, Debug)]
pub struct ModuleDef {
    pub name: Name,
    pub globals: Vec<GlobalDecl>,
    pub state_cells: Vec<StateCellDecl>,
}

/// A request to hot-swap an existing global cell with a closed value of a
/// compatible scheme.
#[derive(Clone, Debug)]
pub struct SwapRequest {
    pub target: Path,
    pub new_scheme: Scheme,
    pub new_value: Term,
}

/// One declared state-cell migration `old_ty -> new_ty` via pure function
/// `migrate_fn`.
#[derive(Clone, Debug)]
pub struct CellMigration {
    pub cell: Path,
    pub old_ty: Type,
    pub new_ty: Type,
    pub migrate_fn: Term,
}

/// An atomic batch of declared state-cell migrations executed under a bounded
/// pure-evaluation fuel budget.
#[derive(Clone, Debug)]
pub struct MigrationBatch {
    pub migrations: Vec<CellMigration>,
    pub fuel: u64,
}

/// Record of one state cell's version transition after a committed migration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigratedCell {
    pub cell: Path,
    pub old_loc: usize,
    pub new_loc: usize,
    pub old_ty: Type,
    pub new_ty: Type,
}

/// Why an atomic migration rolled back without changing the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationRollbackReason {
    UnknownStateCell(Path),
    OldTypeMismatch {
        cell: Path,
        expected: Box<Type>,
        actual: Box<Type>,
    },
    DuplicateCellInBatch(Path),
    TypeError(TypeError),
    RuntimeRaise(String),
    RuntimeDie(String),
    RuntimeStuck(String),
    Diverged,
    ImpureSideEffect(String),
}

impl fmt::Display for MigrationRollbackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStateCell(p) => write!(f, "unknown declared state cell `{p}`"),
            Self::OldTypeMismatch {
                cell,
                expected,
                actual,
            } => write!(
                f,
                "state cell `{cell}` has type `{actual}`, not `{expected}`"
            ),
            Self::DuplicateCellInBatch(p) => {
                write!(f, "state cell `{p}` appears twice in one migration batch")
            }
            Self::TypeError(err) => write!(f, "migration type error: {err}"),
            Self::RuntimeRaise(v) => write!(f, "migration raised error `{v}`"),
            Self::RuntimeDie(why) => write!(f, "migration panicked: {why}"),
            Self::RuntimeStuck(why) => write!(f, "migration stuck: {why}"),
            Self::Diverged => write!(f, "migration diverged (out of fuel)"),
            Self::ImpureSideEffect(why) => write!(f, "migration had impure side effect: {why}"),
        }
    }
}

/// Result of attempting an atomic [`MigrationBatch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    Committed { migrated: Vec<MigratedCell> },
    RolledBack { reason: MigrationRollbackReason },
}

/// Error returned when a hot swap is rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwapError {
    UnknownGlobal(Path),
    Incompatible(CompatibilityError),
    IllTypedReplacement(TypeError),
}

impl fmt::Display for SwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownGlobal(p) => write!(f, "unknown global cell `{p}`"),
            Self::Incompatible(err) => write!(f, "{err}"),
            Self::IllTypedReplacement(err) => write!(f, "replacement value failed typing: {err}"),
        }
    }
}

impl Error for SwapError {}

/// Error returned when loading a module or starting an entry in a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    Swap(SwapError),
    Type(TypeError),
    StateInitFailed { cell: Path, reason: String },
    DuplicateStateCell(Path),
    ActiveExecutionInProgress,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Swap(err) => write!(f, "{err}"),
            Self::Type(err) => write!(f, "{err}"),
            Self::StateInitFailed { cell, reason } => {
                write!(f, "state cell `{cell}` initialization failed: {reason}")
            }
            Self::DuplicateStateCell(p) => write!(f, "duplicate state cell `{p}`"),
            Self::ActiveExecutionInProgress => {
                write!(f, "cannot start a new entry while another is running")
            }
        }
    }
}

impl Error for SessionError {}

/// Bookkeeping for one declared state cell, retaining its full version history
/// so both old frames (pointing to earlier locations) and new code (pointing
/// to `current_loc`) remain well typed against the extended store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateCellEntry {
    pub current_loc: usize,
    pub current_ty: Type,
    pub history: Vec<(usize, Type)>,
}

/// Comparable summary of an execution outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOutcomeSummary {
    Running,
    Value(String),
    Raise(String),
    Die(String),
    Stuck(String),
    OutOfFuel,
}

/// Result of advancing a [`LiveSession`] by one or more small steps.
#[derive(Clone, Debug)]
pub enum StepResult {
    Running {
        steps_taken: usize,
    },
    Finished {
        steps_taken: usize,
        outcome: Outcome,
    },
    Idle,
}

/// Structural snapshot of a [`LiveSession`] used to verify rollback atomicity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub mem_rendered: Vec<String>,
    pub region_live: Vec<bool>,
    pub active_config_rendered: Option<String>,
    pub state_cells: BTreeMap<Path, StateCellEntry>,
    pub global_schemes: BTreeMap<Path, Scheme>,
    pub global_values_rendered: BTreeMap<Path, String>,
}

/// Summarizes an [`Outcome`] into a comparable [`SessionOutcomeSummary`].
#[must_use]
pub fn summarize_outcome(outcome: &Outcome) -> SessionOutcomeSummary {
    match outcome {
        Outcome::Value(v) => SessionOutcomeSummary::Value(render_term(v)),
        Outcome::Raise(v) => SessionOutcomeSummary::Raise(render_term(v)),
        Outcome::Die(why) => SessionOutcomeSummary::Die(why.clone()),
        Outcome::Stuck(why) => SessionOutcomeSummary::Stuck(why.clone()),
        Outcome::OutOfFuel => SessionOutcomeSummary::OutOfFuel,
    }
}

/// Deterministically renders a [`Term`] into a canonical string for oracle
/// comparisons and rollback equality checks.
#[must_use]
pub fn render_term(term: &Term) -> String {
    match term {
        Term::Lit(Lit::Int(n)) => n.to_string(),
        Term::Lit(Lit::Bool(b)) => b.to_string(),
        Term::Lit(Lit::String(s)) => format!("{s:?}"),
        Term::Lit(Lit::Unit) => "()".into(),
        Term::Tuple(xs) => {
            let parts: Vec<String> = xs.iter().map(render_term).collect();
            format!("({})", parts.join(", "))
        }
        Term::List(xs) => {
            let parts: Vec<String> = xs.iter().map(render_term).collect();
            format!("[{}]", parts.join(", "))
        }
        Term::Record(fs) => {
            let parts: Vec<String> = fs
                .iter()
                .map(|(k, v)| format!("{k} = {}", render_term(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Term::Loc(l) => format!("ℓ{l}"),
        other => format!("{other:?}"),
    }
}
