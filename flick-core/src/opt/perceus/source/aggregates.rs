//! Explicit abstract aggregate-representation policy for source-to-memory lowering.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::Constructor;

use crate::ir::Pat;
use crate::ir::memory::CellLayout;

/// Explicit abstract policy selecting which constructors or types lower as
/// unboxed by-value aggregates (`ValueRep::Aggregate`) instead of boxed heap
/// cells (`ValueRep::Cell`), independently of automatic polymorphic
/// specialization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregatePolicy {
    /// Fully qualified type names (`"Module.Type"`) whose constructors lower as
    /// unboxed `ValueRep::Aggregate` values.
    pub types: BTreeSet<String>,
    /// Fully qualified constructor tags (`"Module.Type.Con"` or `"$tuple"`)
    /// that lower as unboxed `ValueRep::Aggregate` values.
    pub constructors: BTreeSet<String>,
    /// Optional abstract `CellLayout` descriptors attached to boxed heap
    /// constructor cells (`ValueRep::Cell`), keyed by qualified constructor tag.
    pub cell_layouts: BTreeMap<String, CellLayout>,
}

impl AggregatePolicy {
    pub fn is_aggregate_constructor(&self, con: &Constructor) -> bool {
        if self.types.is_empty() && self.constructors.is_empty() {
            return false;
        }
        let ty_key = format!("{}.{}", con.module, con.ty);
        if self.types.contains(&ty_key) {
            return true;
        }
        let tag = format!("{ty_key}.{}", con.name);
        self.constructors.contains(&tag)
    }

    pub fn is_aggregate_tag(&self, tag: &str) -> bool {
        self.constructors.contains(tag)
    }

    pub fn cell_layout(&self, tag: &str) -> Option<CellLayout> {
        self.cell_layouts.get(tag).cloned()
    }

    pub fn pattern_is_aggregate(&self, pat: &Pat) -> bool {
        if self.types.is_empty() && self.constructors.is_empty() {
            return false;
        }
        match pat {
            Pat::Con(con, _) => self.is_aggregate_constructor(con),
            Pat::Tuple(items) if !items.is_empty() => self.is_aggregate_tag("$tuple"),
            Pat::Or(alts) => alts.iter().any(|alt| self.pattern_is_aggregate(alt)),
            _ => false,
        }
    }
}
