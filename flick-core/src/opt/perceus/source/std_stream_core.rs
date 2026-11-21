//! Audited `Stream.Core`, `Stream.Run`, and `Stream.Platform` (non-FS) pure IR adapters (Stage 4E).
//!
//! A `Stream r e a` is a `Channel` emitting `NonEmpty a` chunks with done value `Unit`.

mod run;
mod sources;
mod templates;
mod unfold;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn std_stream_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Stream" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let allow_variadic = matches!(bind, "make" | "fromArrays" | "zipLatestAll" | "raceAll")
            && value.adapter.variadic;
        if !allow_variadic && !has_standard_shape(value) {
            return None;
        }
        if allow_variadic
            && (value.catch.is_some()
                || value.adapter.receiver
                || value.adapter.cancel
                || !value.adapter.optional.is_empty()
                || value.adapter.result.nullable)
        {
            return None;
        }
        let b = ExprBuilder::new(span, "$st");
        let expr = if value.mode == ForeignMode::Effect {
            templates::stream_run_template(&b, bind, stages)
                .or_else(|| {
                    super::std_stream_combine::stream_combine_effect_template(&b, bind, stages)
                })
                .or_else(|| {
                    super::std_stream_errors::stream_pipe_effect_template(&b, bind, stages)
                })?
        } else if value.mode == ForeignMode::Pure {
            templates::stream_core_template(&b, bind, stages)
                .or_else(|| {
                    super::std_stream_transform::stream_transform_template(&b, bind, stages)
                })
                .or_else(|| super::std_stream_combine::stream_combine_template(&b, bind, stages))
                .or_else(|| super::std_stream_errors::stream_errors_template(&b, bind, stages))?
        } else {
            return None;
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn stream_core_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stc");
    match name {
        "st_from_lists" => Some(sources::st_from_lists_helper(&b)),
        "st_from_effect_repeat" => Some(sources::st_from_effect_repeat_helper(&b)),
        "st_from_effect_schedule" => Some(sources::st_from_effect_schedule_helper(&b)),
        "st_from_list_effect" => Some(sources::st_from_list_effect_helper(&b)),
        "st_from_list_effect_repeat" => Some(sources::st_from_list_effect_repeat_helper(&b)),
        "st_tick" => Some(sources::st_tick_helper(&b)),
        "st_from_schedule" => Some(sources::st_from_schedule_helper(&b)),
        "st_unfold" => Some(unfold::st_unfold_helper(&b)),
        "st_paginate" => Some(unfold::st_paginate_helper(&b)),
        "st_iterate" => Some(unfold::st_iterate_helper(&b)),
        "st_range" => Some(unfold::st_range_helper(&b)),
        "st_limit_bytes" => Some(unfold::st_limit_bytes_helper(&b)),
        "st_run_collect" => Some(run::st_run_collect_helper(&b)),
        "st_run_fold" => Some(run::st_run_fold_helper(&b)),
        "st_run_for_each" => Some(run::st_run_for_each_helper(&b)),
        "st_run_for_each_while" => Some(run::st_run_for_each_while_helper(&b)),
        "st_mk_string" => Some(run::st_mk_string_helper(&b)),
        _ => None,
    }
}
