//! Contract tests for numeric primitive adapters.

use flick_ir::{Foreign, ForeignMode, Shape};

use super::{equality_expression, intrinsic};
use crate::build::builtin_modules;
use crate::ir::{Expr, ExprKind, ItemKind, Lit, memory};
use crate::opt::perceus::source::{LowerError, lower};
use crate::source::{FileId, Span};

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
                    intrinsic(value).is_some() || equality_expression(value, item.span).is_some(),
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
            };
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
