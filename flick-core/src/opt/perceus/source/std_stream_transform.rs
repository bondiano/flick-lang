//! Audited `Stream.Transform` element/filter/slice/buffer pure IR adapters (Stage 4E).

mod drop;
mod map_par;
mod take;
mod templates;

use super::expr_builder::ExprBuilder;
use super::{Expr, Span};

pub(super) fn stream_transform_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    if let Some(expr) = super::std_stream_window::stream_window_template(b, bind, stages) {
        return Some(expr);
    }
    templates::stream_transform_template(b, bind, stages)
}

pub(super) fn stream_transform_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stt");
    match name {
        "st_map_effect" => Some(map_par::st_map_effect_helper(&b)),
        "st_map_effect_par" => Some(map_par::st_map_effect_par_helper(&b)),
        "st_take" => Some(take::st_take_helper(&b)),
        "st_take_right" => Some(take::st_take_right_helper(&b)),
        "st_take_until" => Some(take::st_take_until_helper(&b)),
        "st_take_while_filter" => Some(take::st_take_while_filter_helper(&b)),
        "st_drop" => Some(drop::st_drop_helper(&b)),
        "st_drop_right" => Some(drop::st_drop_right_helper(&b)),
        "st_drop_while" => Some(drop::st_drop_while_helper(&b)),
        _ => None,
    }
}
