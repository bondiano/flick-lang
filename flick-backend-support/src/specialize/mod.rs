//! Whole-program polymorphic specialization with boxed-cell fallback for
//! layout-aware backends (C, WebAssembly in linear memory, LLVM).
//!
//! Given a typed [`flick_ir::Program`], entry roots, a target
//! [`PrimitiveTable`], and a [`SpecializeConfig`], [`specialize`] produces a
//! deterministic [`SpecializedProgram`] that:
//!
//! - monomorphizes specializable polymorphic functions, constructors, and
//!   class instances to concrete [`flick_ir::TypeRef`]s with layouts computed
//!   by [`crate::layout::LayoutEngine`], with **zero boxed cells** on
//!   specialized paths;
//! - statically classifies non-specializable forms before instantiation —
//!   polymorphic recursion against a `forall` signature, higher-rank
//!   callbacks, packed modules / existentials, GADT constructors holding
//!   dictionaries or existentials, polymorphic `foreign` bindings, and
//!   open-row polymorphic functions crossing fallback boundaries — and
//!   compiles each once over uniform boxed cells;
//! - inserts explicit [`SpecExprKind::Box`] / [`SpecExprKind::Unbox`]
//!   coercions strictly at boundaries between specialized and boxed-fallback
//!   code;
//! - resolves closed-record accesses to concrete layout byte offsets
//!   ([`SpecExprKind::FieldOffset`]) in specialized code and by-label lookups
//!   ([`SpecExprKind::FieldByLabel`]) in boxed fallback code;
//! - preserves checked class dictionary arguments and precedence, replacing
//!   dictionary calls with direct calls **only** when exact dictionary
//!   evidence is statically known;
//! - enforces explicit instantiation and output-size budgets, reporting the
//!   offending definition when exceeded.

mod classify;
mod engine;
mod ir;
mod lower;
mod type_util;
mod types;
mod work;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::error;
use std::fmt;

use flick_ir::{Program, TypeRef};

use crate::layout::{LayoutError, PrimitiveTable, TypeLayout};

pub use ir::{
    SpecArm, SpecDict, SpecExpr, SpecExprKind, SpecHandled, SpecHandler, SpecOpArm, SpecParam,
    SpecPat, SpecStmt,
};
pub use types::{
    CtorKey, DefKey, DefMode, DictSpec, FallbackReason, InstanceKey, SpecializedCtor,
    SpecializedDef, SpecializedForeign, SpecializedInstance, SymbolId, ValueRep,
};

/// Configuration and budgets for [`specialize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializeConfig {
    /// Maximum number of distinct specialized instances allowed across the
    /// program before reporting [`SpecializeError::BudgetExceeded`].
    pub max_instances: usize,
    /// Optional upper bound on estimated specialized output size in bytes.
    pub max_output_bytes: Option<usize>,
    /// Whether statically known class dictionary method calls are rewritten
    /// into direct specialized calls (`true` by default).
    pub devirtualize_known_dicts: bool,
}

impl Default for SpecializeConfig {
    fn default() -> Self {
        SpecializeConfig {
            max_instances: 4096,
            max_output_bytes: None,
            devirtualize_known_dicts: true,
        }
    }
}

/// Which budget was exceeded during specialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BudgetKind {
    /// Total specialized instances exceeded [`SpecializeConfig::max_instances`].
    MaxInstances,
    /// Estimated emitted output bytes exceeded [`SpecializeConfig::max_output_bytes`].
    MaxOutputBytes,
}

impl fmt::Display for BudgetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetKind::MaxInstances => write!(f, "instantiation count"),
            BudgetKind::MaxOutputBytes => write!(f, "output size (bytes)"),
        }
    }
}

/// Structured diagnostic emitted during specialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializeDiagnostic {
    /// Definition (`(module, name)`) associated with the diagnostic.
    pub def: DefKey,
    /// Diagnostic severity/category.
    pub kind: DiagnosticKind,
    /// Human-readable explanation.
    pub message: String,
}

/// Category of a [`SpecializeDiagnostic`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// Definition was statically routed to uniform boxed-cell fallback.
    FallbackSelected(FallbackReason),
    /// A specialization budget was exceeded while instantiating `def`.
    BudgetExceeded {
        kind: BudgetKind,
        limit: usize,
        count: usize,
    },
}

/// Errors that stop specialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecializeError {
    /// A requested root `(module, name)` was not found in the program.
    UnknownRoot(DefKey),
    /// Specialization exceeded a configured budget while instantiating `def`.
    BudgetExceeded {
        def: DefKey,
        kind: BudgetKind,
        limit: usize,
        count: usize,
    },
    /// Layout computation failed for a concrete monomorphic type.
    Layout(LayoutError),
}

impl fmt::Display for SpecializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecializeError::UnknownRoot((m, n)) => {
                write!(f, "unknown specialization root `{m}.{n}`")
            }
            SpecializeError::BudgetExceeded {
                def: (m, n),
                kind,
                limit,
                count,
            } => write!(
                f,
                "specialization {kind} budget exceeded while instantiating `{m}.{n}` ({count} > limit {limit})"
            ),
            SpecializeError::Layout(err) => write!(f, "layout error during specialization: {err}"),
        }
    }
}

impl error::Error for SpecializeError {}

impl From<LayoutError> for SpecializeError {
    fn from(err: LayoutError) -> Self {
        SpecializeError::Layout(err)
    }
}

/// Build report statistics for a [`SpecializedProgram`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecializeStats {
    /// Number of monomorphized definitions emitted.
    pub specialized_instances: usize,
    /// Number of uniform boxed-cell fallback definitions emitted.
    pub fallback_definitions: usize,
    /// Number of polymorphic `foreign` fallback wrappers emitted.
    pub polymorphic_foreigns: usize,
    /// Total `Box` coercions emitted across all definitions and instances.
    pub box_count: usize,
    /// Total `Unbox` coercions emitted across all definitions and instances.
    pub unbox_count: usize,
    /// Total boxed-cell boundary coercions (`box_count + unbox_count`).
    pub boxed_cells: usize,
    /// Number of statically resolved direct dictionary method calls.
    pub direct_dict_calls: usize,
    /// Number of runtime dictionary method calls preserved.
    pub runtime_dict_calls: usize,
    /// Number of closed-record field accesses lowered to concrete layout offsets.
    pub offset_field_reads: usize,
    /// Number of open/boxed-record field accesses lowered to by-label lookups.
    pub label_field_reads: usize,
    /// Estimated emitted IR size in bytes across all definitions and instances.
    pub estimated_output_bytes: usize,
    /// Specialized instance count per source definition `(module, name)`.
    pub instances_per_def: BTreeMap<DefKey, usize>,
}

/// Complete, deterministic output of [`specialize`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpecializedProgram {
    /// Entry roots mapped to their lowered symbol IDs.
    pub roots: BTreeMap<DefKey, SymbolId>,
    /// All lowered definitions (monomorphized and boxed-fallback), in
    /// deterministic [`SymbolId`] order.
    pub definitions: BTreeMap<SymbolId, SpecializedDef>,
    /// Reachable constructors with their concrete layouts (or boxed fallback).
    pub constructors: BTreeMap<CtorKey, SpecializedCtor>,
    /// Reachable runtime class instance tables (for unresolved/dynamic dicts).
    pub instances: BTreeMap<SymbolId, SpecializedInstance>,
    /// Reachable `foreign` bindings (monomorphic or boxed-cell polymorphic).
    pub foreigns: BTreeMap<SymbolId, SpecializedForeign>,
    /// Deterministic layouts computed via [`crate::layout::LayoutEngine`] for
    /// every concrete monomorphic type used in specialized code.
    pub layouts: BTreeMap<TypeRef, TypeLayout>,
    /// Aggregate specialization statistics and budget report.
    pub stats: SpecializeStats,
    /// Structured informational diagnostics (e.g., fallback classifications).
    pub diagnostics: Vec<SpecializeDiagnostic>,
}

/// Runs deterministic whole-program specialization with boxed-cell fallback on
/// `program` starting from `roots`.
pub fn specialize(
    program: &Program,
    roots: &[DefKey],
    primitives: &PrimitiveTable,
    config: &SpecializeConfig,
) -> Result<SpecializedProgram, SpecializeError> {
    engine::run(program, roots, primitives, config)
}
