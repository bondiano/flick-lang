//! Exact compare/hash and unsafe Int adapters, not arbitrary FFI erasure.
use flick_ir::{Adapter, Arith, Copies, Foreign, ForeignMode, Lit, Num, ResultAdapter, Shape};

#[path = "scalar.rs"]
mod scalar;
#[cfg(test)]
mod tests;

use super::{
    BinOp, Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Name, Operation, Param,
    Pat, Span, Stage, layers, scalar_context::ScalarDomain, strings,
};

impl Lower<'_> {
    pub(super) fn adapted_foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if super::list_range::is_binding(value) {
            return Some(self.list_range(value, name, span, block));
        }
        if let Some(result) = self.lazy_scope_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.record_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.seq_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(kind) = scalar::candidate(value) {
            return Some(self.scalar_foreign(value, name, kind, span, block));
        }
        if let Some(expression) = super::option_map2::expression(value, span)
            .or_else(|| super::result_map2::expression(value, span))
            .or_else(|| super::eff_combinators::expression(value, span))
        {
            return Some(self.expr(&expression, &Env::new(), block));
        }
        if let Some(expression) = super::list_collections::array_expression(value, span) {
            return Some(
                self.expr(&expression, &Env::new(), block)
                    .and_then(|function| {
                        self.intrinsic(Intrinsic::ArrayToList, false, vec![], block, span)
                            .and_then(|argument| {
                                block.emit(Operation::Apply { function, argument }, span)
                            })
                    }),
            );
        }
        if let Some(expression) = super::list_collections::expression(value, span) {
            return Some(self.expr(&expression, &Env::new(), block));
        }
        if let Some(expression) = strings::expression(value, span) {
            return Some(self.expr(&expression, &Env::new(), block));
        }
        if let Some(expression) = equality_expression(value, span) {
            return Some(self.expr(&expression, &Env::new(), block));
        }
        if let Some(kind) = strings::intrinsic(value)
            .or_else(|| layers::intrinsic(value))
            .or_else(|| intrinsic(value))
        {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }
        self.core_and_std_foreign(value, name, span, block)
    }

    fn core_and_std_foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if let Some(result) = self.core_scalar_foreign(value, name, span, block) {
            return Some(result);
        }
        if let Some(result) = self.core_string_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.core_list_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.core_record_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_functional_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_combine_foreign(value, name, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_chunk_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_hash_map_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_json_decimal_regexp_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_duration_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_outcome_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_test_gen_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_schema_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_codec_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_check_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_path_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_datetime_cron_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_graph_foreign(value, span, block) {
            return Some(result);
        }
        self.std_effect_and_runtime_foreign(value, span, block)
    }

    fn std_effect_and_runtime_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if let Some(result) = self.std_async_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_effect_ops_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_effect_coll_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.schedule
            && let Some(result) = self.std_schedule_foreign(value, span, block)
        {
            return Some(result);
        }
        if let Some(result) = self.std_sync_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_part_sem_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.queue {
            if let Some(result) = self.std_queue_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_pubsub_foreign(value, span, block) {
                return Some(result);
            }
        }
        if let Some(result) = self.std_scoped_res_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_rc_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_cache_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_scoped_cache_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_pool_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_layer_map_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_tx_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_tx_lock_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_tx_queue_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_tx_coll_foreign(value, span, block) {
            return Some(result);
        }
        self.std_observe_and_stream_foreign(value, span, block)
    }

    fn std_observe_and_stream_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if let Some(result) = self.std_sched_plan_foreign(value, span, block) {
            return Some(result);
        }
        if let Some(result) = self.std_events_rep_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.observe {
            if let Some(result) = self.std_observe_refs_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_observe_trace_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_observe_span_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_observe_log_foreign(value, span, block) {
                return Some(result);
            }
        }
        if let Some(result) = self.std_crypto_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.observe
            && let Some(result) = self.std_metric_foreign(value, span, block)
        {
            return Some(result);
        }
        if let Some(result) = self.std_request_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.config {
            if let Some(result) = self.std_config_prov_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_config_foreign(value, span, block) {
                return Some(result);
            }
        }
        if let Some(result) = self.std_arbitrary_foreign(value, span, block) {
            return Some(result);
        }
        if !self.bridge_families.stream {
            if let Some(result) = self.std_channel_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_sink_foreign(value, span, block) {
                return Some(result);
            }
            if let Some(result) = self.std_stream_foreign(value, span, block) {
                return Some(result);
            }
        }
        if let Some(result) = self.std_sql_foreign(value, span, block) {
            return Some(result);
        }
        self.std_layer_foreign(value, span, block)
    }

    fn scalar_foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        kind: Intrinsic,
        span: Span,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        let label = match kind {
            Intrinsic::NumberToFloat => "Number conversion",
            Intrinsic::ToBigInt => "BigInt conversion",
            _ => "Number selection",
        };
        let declarations = self.declarations.as_ref().ok_or_else(|| {
            super::unsupported(
                format!("{label} requires a checked declaration context"),
                span,
            )
        })?;
        let source = Name::new(
            &name.module,
            value.source_name.as_deref().unwrap_or(&value.name),
        );
        let domain = match declarations.adapter(&source, kind) {
            Ok(domain) => domain,
            Err(_) if self.allow_foreign_bridge => {
                return self.foreign_bridge(value, name, span, block);
            }
            Err(message) => {
                return Err(super::unsupported(format!("{label}: {message}"), span));
            }
        };
        if domain == ScalarDomain::IntSelection {
            self.expr(
                &scalar::int_selection(kind == Intrinsic::FloatMin, span),
                &Env::new(),
                block,
            )
        } else {
            self.intrinsic(kind, false, vec![], block, span)
        }
    }
}

pub(super) fn intrinsic(value: &Foreign) -> Option<Intrinsic> {
    let kind = match value.binding.name.as_str() {
        "compare" => Intrinsic::PrimitiveCompare,
        "hash" => Intrinsic::PrimitiveHash,
        "intUnsafeDiv" => Intrinsic::IntUnsafeDiv,
        "intUnsafeRem" => Intrinsic::IntUnsafeRem,
        _ => return None,
    };
    supports(
        value,
        kind.arity(),
        matches!(kind, Intrinsic::PrimitiveCompare | Intrinsic::PrimitiveHash),
        kind == Intrinsic::PrimitiveCompare,
    )
    .then_some(kind)
}

pub(super) fn equality_expression(value: &Foreign, span: Span) -> Option<Expr> {
    let at = |kind| Expr::new(kind, span);
    let params = |left: &str, right: &str| {
        [left, right]
            .into_iter()
            .map(|name| Param {
                pat: Pat::Bind(name.into()),
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            })
            .collect()
    };
    if value.binding.name == "equals" && supports(value, 2, true, false) {
        let left = "$primitiveEquals.left";
        let right = "$primitiveEquals.right";
        return Some(at(ExprKind::Lambda {
            params: params(left, right),
            body: Box::new(at(ExprKind::Binary(
                BinOp::Eq,
                Box::new(at(ExprKind::Local(left.into()))),
                Box::new(at(ExprKind::Local(right.into()))),
            ))),
        }));
    }
    if value.binding.name == "numberStrictEqual" && supports(value, 2, false, false) {
        let left = "$strictEq.left";
        let right = "$strictEq.right";
        return Some(at(ExprKind::Lambda {
            params: params(left, right),
            body: Box::new(at(ExprKind::Binary(
                BinOp::Same,
                Box::new(at(ExprKind::Local(left.into()))),
                Box::new(at(ExprKind::Local(right.into()))),
            ))),
        }));
    }
    if value.binding.name == "numberSameValue" && supports(value, 2, false, false) {
        // Object.is(a, b): if a === b then (a !== 0.0 || 1.0 / a === 1.0 / b) else (a !== a && b !== b)
        let left = "$sameVal.left";
        let right = "$sameVal.right";
        let a = || at(ExprKind::Local(left.into()));
        let b = || at(ExprKind::Local(right.into()));
        let zero = at(ExprKind::Lit(Lit::Float(0.0)));
        let one_div = |x| {
            at(ExprKind::Binary(
                BinOp::Arith(Arith::Div, Num::Float),
                Box::new(at(ExprKind::Lit(Lit::Float(1.0)))),
                Box::new(x),
            ))
        };
        let same_eq = at(ExprKind::Binary(BinOp::Same, Box::new(a()), Box::new(b())));
        let nonzero = at(ExprKind::Binary(
            BinOp::NotSame,
            Box::new(a()),
            Box::new(zero),
        ));
        let same_sign = at(ExprKind::Binary(
            BinOp::Same,
            Box::new(one_div(a())),
            Box::new(one_div(b())),
        ));
        let nonzero_or_same_sign = at(ExprKind::Binary(
            BinOp::Or,
            Box::new(nonzero),
            Box::new(same_sign),
        ));
        let a_nan = at(ExprKind::Binary(
            BinOp::NotSame,
            Box::new(a()),
            Box::new(a()),
        ));
        let b_nan = at(ExprKind::Binary(
            BinOp::NotSame,
            Box::new(b()),
            Box::new(b()),
        ));
        let both_nan = at(ExprKind::Binary(
            BinOp::And,
            Box::new(a_nan),
            Box::new(b_nan),
        ));
        return Some(at(ExprKind::Lambda {
            params: params(left, right),
            body: Box::new(at(ExprKind::If(
                Box::new(same_eq),
                Box::new(nonzero_or_same_sign),
                Box::new(both_nan),
            ))),
        }));
    }
    None
}

fn supports(value: &Foreign, arity: usize, copies_args: bool, copies_result: bool) -> bool {
    // These exact host operations read immutable values and return only a
    // primitive or a closed Ordering constructor. No mutable value escapes;
    // this discharges their written defensive-copy adapter, not other FFI.
    value.binding.from.as_deref() == Some("flick:rt")
        && value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == arity
        && value.stages == vec![false; arity]
        && value.catch.is_none()
        && value.adapter
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None; arity],
                copies: Copies {
                    args: vec![copies_args; arity],
                    result: copies_result,
                    callbacks: vec![None; arity],
                },
                shapes: vec![Shape::Data; arity],
                result_shape: Shape::Data,
                words: vec![],
            }
}
