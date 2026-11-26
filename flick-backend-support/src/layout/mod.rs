//! Shared, deterministic data layout computation for layout-aware backends
//! (C, WebAssembly in linear memory, LLVM).
//!
//! Given a [`flick_ir::Program`] (or its modules and lang items), a backend's
//! [`PrimitiveTable`], and a monomorphic [`flick_ir::TypeRef`], this module
//! computes:
//!
//! - total byte size and power-of-two alignment;
//! - discriminant width and placement, or a niche encoding reusing unused bit
//!   patterns of a payload field (such as a non-null pointer for
//!   `Option (List Int)` or `Bool` for `Option Bool`);
//! - field byte offsets per constructor, tuple, and closed record (reordered
//!   to reduce padding while keeping canonical `Record.labels` / positional /
//!   declaration order in the output);
//! - payload-free constructors as discriminant values only;
//! - automatic pointer indirection determined **before** recursive layout
//!   expansion: a finite declaration-level pass boxes growing recursive
//!   instantiations (e.g. `Nested (List a)`), and Tarjan's strongly connected
//!   components break remaining by-value cycles without adding pointers to
//!   non-recursive types.
//!
//! Layouts are internal to backends and stay out of the value-ABI tables.

mod catalog;
mod engine;
mod indirection;
mod inst_graph;
pub mod memory;
mod pack;
mod primitives;
mod types;

#[cfg(test)]
mod tests;

use std::error;
use std::fmt;

use flick_ir::{Program, TypeRef};

pub use engine::LayoutEngine;
pub use memory::{cell_layout_of, value_rep_of, variant_cell_layout_of};
pub use primitives::{Niche, Primitive, PrimitiveLayout, PrimitiveTable};
pub use types::{
    DataLayout, DictFieldLayout, Discriminant, FieldLayout, FieldStorage, RecordFieldLayout,
    RecordLayout, TupleLayout, TypeLayout, TypeRepr, VariantLayout,
};

/// Computes the layout of one monomorphic IR type `ty` in `program` under
/// `primitives`.
///
/// For computing many layouts against the same program and primitive table,
/// reuse a [`LayoutEngine`] so static analysis and intermediate layouts are
/// cached across queries.
pub fn compute_layout(
    program: &Program,
    primitives: &PrimitiveTable,
    ty: &TypeRef,
) -> Result<TypeLayout, LayoutError> {
    let mut engine = LayoutEngine::new(program, primitives.clone())?;
    engine.layout(ty)
}

/// Errors reported when a primitive table is malformed or an input type is not
/// a valid monomorphic type for layout computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// A primitive table entry violates alignment, size, or niche invariants.
    InvalidPrimitiveTable(String),
    /// A type variable or higher-kinded variable application remains in the
    /// requested type.
    PolymorphicType(String),
    /// A record type still has an open row tail variable.
    OpenRecord(String),
    /// A named type is neither a known primitive/handle nor declared in any
    /// module of the program.
    UnknownType { module: String, name: String },
    /// A named type was applied to the wrong number of type arguments.
    ArityMismatch {
        module: String,
        name: String,
        expected: usize,
        found: usize,
    },
    /// An `opaque type alias` references itself without an intervening nominal
    /// data type.
    CyclicAlias { module: String, name: String },
    /// Layout size or offset calculation overflowed `u64`.
    SizeOverflow,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::InvalidPrimitiveTable(msg) => {
                write!(f, "invalid primitive table: {msg}")
            }
            LayoutError::PolymorphicType(ty) => {
                write!(f, "cannot compute layout of polymorphic type `{ty}`")
            }
            LayoutError::OpenRecord(tail) => {
                write!(f, "cannot compute layout of open record with tail `{tail}`")
            }
            LayoutError::UnknownType { module, name } => {
                write!(f, "unknown type `{module}.{name}`")
            }
            LayoutError::ArityMismatch {
                module,
                name,
                expected,
                found,
            } => write!(
                f,
                "type `{module}.{name}` expects {expected} type argument(s), found {found}"
            ),
            LayoutError::CyclicAlias { module, name } => {
                write!(f, "cyclic opaque type alias `{module}.{name}`")
            }
            LayoutError::SizeOverflow => write!(f, "data layout size overflowed 64 bits"),
        }
    }
}

impl error::Error for LayoutError {}
