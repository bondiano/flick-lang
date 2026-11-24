//! Exact typed Result bridge and try-only closure lowering.

use super::kept_for_call;
use flick_ir::{Adapter, Arm, Copies, Foreign, ForeignMode, Handler, ResultAdapter, Shape};

use super::{
    Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Operation, Param, Pat, Span,
    Stage, free, lookup,
};

pub(super) fn supports(value: &Foreign) -> bool {
    value.binding.module_export() == Some(("flick:rt", "fromResult"))
        && value.mode == ForeignMode::Effect
        && value.arity == 1
        && value.dicts == 0
        && value.stages == [true]
        && value.catch.is_none()
        && kept_for_call(&value.adapter)
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None],
                copies: Copies {
                    args: vec![true],
                    result: true,
                    callbacks: vec![None],
                },
                shapes: vec![Shape::Result(Box::new(Shape::Data), Box::new(Shape::Data))],
                result_shape: Shape::Data,
                words: vec![],
                retention: None,
            }
}

impl Lower<'_> {
    pub(super) fn try_only(
        &mut self,
        handler: &Handler,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        // Body code is delayed until Try has installed its catch.
        let body = self.lambda(
            &[Param {
                pat: Pat::Wild,
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            }],
            &handler.body,
            env,
            None,
            block,
            span,
        )?;
        let caught = self.failure_handler(&handler.errors, env, block, span)?;
        block.emit(
            Operation::Intrinsic {
                kind: Intrinsic::Try,
                args: vec![body, caught],
            },
            span,
        )
    }

    pub(super) fn failure_handler(
        &mut self,
        errors: &[Arm],
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        // A synthetic Match is used only for its binder-aware free-variable walk.
        // Its scrutinee has no free variables; local 1 is the actual failure.
        let arms = Expr::new(
            ExprKind::Match(
                Box::new(Expr::new(ExprKind::Lit(super::Lit::Unit), span)),
                errors.to_vec(),
            ),
            span,
        );
        let names: Vec<_> = free(&arms).into_iter().collect();
        let captures = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<_, _>>()?;
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Env::new();
        for (field, name) in names.into_iter().enumerate() {
            let value = function.emit(Operation::Project { object: 0, field }, span)?;
            inside.insert(name, value);
        }
        let result = self.matching_local(
            1,
            errors,
            &inside,
            &mut function,
            span,
            super::matches::MatchMode::RecoverySelection,
        )?;
        self.finish(code, &function.finish(result))?;
        block.emit(Operation::Closure { code, captures }, span)
    }
}

#[cfg(test)]
mod tests {
    use super::supports;
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit, memory};
    use crate::opt::perceus::source::{LowerError, lower};
    use crate::source::{FileId, Span};
    use flick_ir::{Foreign, ForeignMode, Shape};

    #[test]
    fn builtin_from_result_is_exact_and_each_adapter_dimension_is_pinned() {
        let module = builtin_modules()
            .iter()
            .find(|module| module.name == "Prelude")
            .expect("Prelude builtin");
        let original = module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(value) if value.name == "from_result" => Some((item.span, value)),
                _ => None,
            })
            .expect("real from_result");
        assert!(supports(original.1), "{:#?}", original.1);
        let entry = memory::Name::new("Prelude", "from_result");
        assert!(lower([module], &entry).is_ok());
        let mutations: Vec<fn(&mut Foreign)> = vec![
            |v| {
                v.binding = flick_ir::Binding::Global {
                    name: v.binding.export_name().unwrap_or("").to_owned(),
                }
            },
            |v| {
                if let flick_ir::Binding::Module { name, .. } | flick_ir::Binding::Global { name } =
                    &mut v.binding
                {
                    name.push('!');
                }
            },
            |v| v.mode = ForeignMode::Sync,
            |v| v.arity = 2,
            |v| v.dicts = 1,
            |v| v.stages[0] = false,
            |v| v.stages.push(true),
            |v| {
                v.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            },
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "this".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "cancel".into(),
                    args: vec![flick_ir::AdapterArg::Word("signal".into())],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| v.adapter.optional.push(0),
            |v| {
                v.adapter.words.push(flick_ir::BackendAdapter {
                    name: "variadic".into(),
                    args: vec![],
                    span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
                });
            },
            |v| v.adapter.result.nullable = true,
            |v| v.adapter.result.unit = true,
            |v| v.adapter.callbacks[0] = Some(1),
            |v| v.adapter.callbacks.clear(),
            |v| v.adapter.copies.args[0] = false,
            |v| v.adapter.copies.args.clear(),
            |v| v.adapter.copies.result = false,
            |v| v.adapter.copies.callbacks[0] = Some((vec![true], true)),
            |v| v.adapter.copies.callbacks.clear(),
            |v| v.adapter.shapes[0] = Shape::Data,
            |v| {
                v.adapter.shapes[0] = Shape::Result(
                    Box::new(Shape::Option(Box::new(Shape::Data))),
                    Box::new(Shape::Data),
                );
            },
            |v| v.adapter.shapes.clear(),
            |v| v.adapter.result_shape = Shape::Option(Box::new(Shape::Data)),
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut changed = module.clone();
            let foreign = changed
                .items
                .iter_mut()
                .find_map(|item| match &mut item.kind {
                    ItemKind::Foreign(value) if value.name == "from_result" => Some(value),
                    _ => None,
                })
                .expect("copied foreign");
            mutate(foreign);
            assert!(!supports(foreign), "mutation {index}");
            assert!(
                matches!(lower([&changed], &entry), Err(LowerError::Unsupported { span, .. }) if span == original.0),
                "mutation {index}"
            );
        }
    }
}
