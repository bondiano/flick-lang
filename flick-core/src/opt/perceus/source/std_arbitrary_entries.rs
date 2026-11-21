//! Pure-IR `Arbitrary` array and record-entries shrink-tree helpers (Stage 4D).

mod array;
mod entries;

use super::expr_builder::ExprBuilder;
use super::{Expr, Span};
use array::{array_sample_helper, rebuild_array_helper};
use entries::{entries_sample_helper, rebuild_entries_helper};

pub(super) fn arb_entries_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$ae");
    match name {
        "arb_rebuild_array" => Some(rebuild_array_helper(&b)),
        "arb_array_sample" => Some(array_sample_helper(&b)),
        "arb_entries_sample" => Some(entries_sample_helper(&b)),
        "arb_rebuild_entries" => Some(rebuild_entries_helper(&b)),
        _ => None,
    }
}

fn rset_e(b: &ExprBuilder, r: Expr, v: Expr) -> Expr {
    b.app2(b.global("Ref", "set"), r, v)
}
