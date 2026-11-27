//! Live hot-swap and typed state-cell migration model (`docs/formal/hot-swap.md`,
//! `formal/Formal/HotSwap.lean`).
//!
//! Extends the λFlick reference model with:
//! - [`LiveSession`]: a persistent session over modules with global cells and
//!   declared state cells, supporting step-by-step execution interleaved with
//!   compatible global swaps and atomic pure `Old -> New` state migrations;
//! - [`check_scheme_compatibility`], [`check_swap_value`], and
//!   [`check_migration_fn`] (re-exported from [`crate::check::compat`]): the
//!   exact scheme-subsumption compatibility rule (`σ_new ≤ σ_old`);
//! - [`generate_session`] and [`run_session_oracle`]: deterministic session
//!   generator and reference oracle runner exported for model properties and
//!   differential VM testing (`#990`).

mod builders;
pub mod generate;
mod migrate;
pub mod oracle;
pub mod scenario;
pub mod session;
pub mod types;

pub use crate::check::compat::{
    CompatibilityError, check_migration_fn, check_scheme_compatibility, check_swap_value,
    format_scheme, is_closed_source_term, is_closed_source_value,
};
pub use generate::generate_session;
pub use oracle::run_session_oracle;
pub use scenario::{
    ActionObservation, MigrationKind, SessionAction, SessionOracleError, SessionOracleReport,
    SessionScenario, SwapKind,
};
pub use session::LiveSession;
pub use types::{
    CellMigration, GlobalDecl, MigratedCell, MigrationBatch, MigrationOutcome,
    MigrationRollbackReason, ModuleDef, SessionError, SessionOutcomeSummary, SessionSnapshot,
    StateCellDecl, StateCellEntry, StepResult, SwapError, SwapRequest, render_term,
    summarize_outcome,
};
