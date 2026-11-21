//! Audited `Stream.Combine` pure IR adapters (concat, flatMap, merge, zip, race, interleave, timeout) (Stage 4E).

mod control;
mod race;
mod templates_merge;
mod templates_zip;
mod zip;
mod zip_latest;
mod zip_neighbor;

use super::expr_builder::ExprBuilder;
use super::{Expr, Span};

pub(super) fn stream_combine_effect_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    super::std_stream_split::stream_split_effect_template(b, bind, stages)
}

pub(super) fn stream_combine_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = super::std_stream_split::stream_split_pure_template(b, bind, stages) {
        return Some(expr);
    }
    templates_merge::stream_merge_template(b, bind, stages)
        .or_else(|| templates_zip::stream_zip_template(b, bind, stages))
}

pub(super) fn stream_combine_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stcb");
    match name {
        "st_flat_map_par" => Some(control::st_flat_map_par_helper(&b)),
        "st_repeat_elements" => Some(control::st_repeat_elements_helper(&b)),
        "st_timeout_or_else" => Some(control::st_timeout_or_else_helper(&b)),
        "st_zip_with" => Some(zip::st_zip_with_helper(&b)),
        "st_zip_with_array" => Some(zip::st_zip_with_array_helper(&b)),
        "st_zip_with_index" => Some(zip::st_zip_with_index_helper(&b)),
        "st_zip_with_next" => Some(zip_neighbor::st_zip_with_next_helper(&b)),
        "st_zip_with_previous" => Some(zip_neighbor::st_zip_with_previous_helper(&b)),
        "st_zip_with_prev_next" => Some(zip_neighbor::st_zip_with_prev_next_helper(&b)),
        "st_zip_latest_all" => Some(zip_latest::st_zip_latest_all_helper(&b)),
        "st_race_all" => Some(race::st_race_all_helper(&b)),
        "st_interleave_with" => Some(race::st_interleave_with_helper(&b)),
        _ => None,
    }
}
