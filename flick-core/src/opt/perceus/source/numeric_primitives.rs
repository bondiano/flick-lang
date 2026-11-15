//! Exact compare/hash and unsafe Int adapters, not arbitrary FFI erasure.
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

#[path = "scalar.rs"]
mod scalar;

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
        strings::intrinsic(value)
            .or_else(|| layers::intrinsic(value))
            .or_else(|| intrinsic(value))
            .map(|kind| self.intrinsic(kind, false, vec![], block, span))
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
        let domain = declarations
            .adapter(&source, kind)
            .map_err(|message| super::unsupported(format!("{label}: {message}"), span))?;
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
    if value.binding.name != "equals" || !supports(value, 2, true, false) {
        return None;
    }
    let left = "$primitiveEquals.left";
    let right = "$primitiveEquals.right";
    Some(Expr::new(
        ExprKind::Lambda {
            params: [left, right]
                .into_iter()
                .map(|name| Param {
                    pat: Pat::Bind(name.into()),
                    stage: Stage::Pure,
                    ty: None,
                })
                .collect(),
            body: Box::new(Expr::new(
                ExprKind::Binary(
                    BinOp::Eq,
                    Box::new(Expr::new(ExprKind::Local(left.into()), span)),
                    Box::new(Expr::new(ExprKind::Local(right.into()), span)),
                ),
                span,
            )),
        },
        span,
    ))
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
                receiver: false,
                cancel: false,
                optional: vec![],
                variadic: false,
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
            }
}

#[cfg(test)]
mod tests {
    use super::{equality_expression, intrinsic};
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit, memory};
    use crate::opt::perceus::source::{LowerError, lower};
    use crate::source::{FileId, Span};
    use flick_ir::{Foreign, ForeignMode, Shape};

    #[test]
    fn actual_numeric_adapters_match_the_complete_declared_contract() {
        let mut count = 0;
        for module in builtin_modules() {
            for item in &module.items {
                let ItemKind::Foreign(value) = &item.kind else {
                    continue;
                };
                if module.name == "Instances"
                    && matches!(value.binding.name.as_str(), "compare" | "hash" | "equals")
                {
                    count += 1;
                    assert!(
                        intrinsic(value).is_some()
                            || equality_expression(value, item.span).is_some(),
                        "{value:#?}"
                    );
                    let entry = memory::Name::new(&module.name, &value.name);
                    assert!(lower([module], &entry).is_ok(), "live adapter lowers");
                    for (index, mutate) in mutations(value.arity).into_iter().enumerate() {
                        let mut changed = module.clone();
                        let declaration = changed
                            .items
                            .iter_mut()
                            .find(|candidate| candidate.span == item.span)
                            .expect("same declaration");
                        let ItemKind::Foreign(foreign) = &mut declaration.kind else {
                            unreachable!()
                        };
                        mutate(foreign);
                        assert!(
                            matches!(lower([&changed], &entry), Err(LowerError::Unsupported { span, .. }) if span == item.span),
                            "{} mutation{index}",
                            value.binding.name
                        );
                    }
                }
            }
        }
        assert_eq!(count, 3);
    }

    type Mutation = Box<dyn Fn(&mut Foreign)>;

    fn mutations(arity: usize) -> Vec<Mutation> {
        let mut out: Vec<Mutation> = vec![
            Box::new(|v| v.binding.from = None),
            Box::new(|v| v.binding.from = Some("other".into())),
            Box::new(|v| v.binding.name.push('!')),
            Box::new(|v| v.mode = ForeignMode::Sync),
            Box::new(|v| v.mode = ForeignMode::Async),
            Box::new(|v| v.mode = ForeignMode::Effect),
            Box::new(|v| v.arity = 0),
            Box::new(|v| v.arity += 1),
            Box::new(|v| v.dicts = 1),
            Box::new(|v| v.stages.clear()),
            Box::new(|v| v.stages.push(false)),
            Box::new(|v| {
                v.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            }),
            Box::new(|v| v.adapter.receiver = true),
            Box::new(|v| v.adapter.cancel = true),
            Box::new(|v| v.adapter.optional.push(0)),
            Box::new(|v| v.adapter.variadic = true),
            Box::new(|v| v.adapter.result.nullable = true),
            Box::new(|v| v.adapter.result.unit = true),
            Box::new(|v| v.adapter.callbacks.clear()),
            Box::new(|v| v.adapter.callbacks.push(None)),
            Box::new(|v| v.adapter.copies.args.clear()),
            Box::new(|v| v.adapter.copies.args.push(true)),
            Box::new(|v| v.adapter.copies.result = !v.adapter.copies.result),
            Box::new(|v| v.adapter.copies.callbacks.clear()),
            Box::new(|v| v.adapter.copies.callbacks.push(None)),
            Box::new(|v| v.adapter.shapes.clear()),
            Box::new(|v| v.adapter.shapes.push(Shape::Data)),
            Box::new(|v| v.adapter.result_shape = Shape::Layer),
            Box::new(|v| {
                v.adapter.result_shape = Shape::Fun {
                    args: vec![Shape::Data],
                    result: Box::new(Shape::Data),
                    runtime: false,
                    sync: false,
                }
            }),
        ];
        for index in 0..arity {
            out.push(Box::new(move |v| v.stages[index] = true));
            out.push(Box::new(move |v| v.adapter.callbacks[index] = Some(1)));
            out.push(Box::new(move |v| v.adapter.copies.args[index] = false));
            out.push(Box::new(move |v| {
                v.adapter.copies.callbacks[index] = Some((vec![true], false));
            }));
            out.push(Box::new(move |v| {
                v.adapter.copies.callbacks[index] = Some((vec![], true));
            }));
            out.push(Box::new(move |v| v.adapter.shapes[index] = Shape::Layer));
        }
        out
    }
}
