//! Pure-IR `Arbitrary` core shrink-tree model helpers (`$rt.Helper.arb_*`, Stage 4D).

mod combinators;
mod core;
mod product;

use super::expr_builder::ExprBuilder;
use super::{Expr, Span};
use combinators::{
    filter_map_pull_helper, filter_map_sample_helper, filter_sample_helper, flat_map_sample_helper,
    flat_map_source_pull_helper,
};
use core::{
    concat_pulls_helper, from_retained_helper, lazy_shrinks_helper, map_pull_helper,
    map_retained_helper, map_sample_helper, pull_each_helper, retain_helper,
};
use product::{product_sample_helper, rebuild_product_helper, replace_at_helper};

pub(super) fn arb_model_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$am");
    match name {
        "arb_pull_each" => Some(pull_each_helper(&b)),
        "arb_concat_pulls" => Some(concat_pulls_helper(&b)),
        "arb_lazy_shrinks" => Some(lazy_shrinks_helper(&b)),
        "arb_retain" => Some(retain_helper(&b)),
        "arb_from_retained" => Some(from_retained_helper(&b)),
        "arb_map_retained" => Some(map_retained_helper(&b)),
        "arb_map_pull" => Some(map_pull_helper(&b)),
        "arb_map_sample" => Some(map_sample_helper(&b)),
        "arb_replace_at" => Some(replace_at_helper(&b)),
        "arb_product_sample" => Some(product_sample_helper(&b)),
        "arb_rebuild_product" => Some(rebuild_product_helper(&b)),
        "arb_filter_map_pull" => Some(filter_map_pull_helper(&b)),
        "arb_filter_map_sample" => Some(filter_map_sample_helper(&b)),
        "arb_filter_sample" => Some(filter_sample_helper(&b)),
        "arb_flat_map_sample" => Some(flat_map_sample_helper(&b)),
        "arb_flat_map_source_pull" => Some(flat_map_source_pull_helper(&b)),
        _ => None,
    }
}

fn rset_e(b: &ExprBuilder, r: Expr, v: Expr) -> Expr {
    b.app2(b.global("Ref", "set"), r, v)
}
