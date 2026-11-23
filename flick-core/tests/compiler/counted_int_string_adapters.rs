//! Exact counted adapters reject every unsupported calling-convention change.
use flick_core::build;
use flick_core::ir::{Expr, ExprKind, Foreign, ItemKind, Lit, memory};
use flick_core::opt::perceus;
use flick_core::source::{FileId, Span};
use flick_ir::{ForeignMode, Shape};

type Mutation = Box<dyn Fn(&mut Foreign)>;
fn mutations(arity: usize) -> Vec<Mutation> {
    let mut changes: Vec<Mutation> = vec![
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
        Box::new(|v| {
            v.adapter.words.push(flick_ir::AdapterWord {
                name: "this".into(),
                args: vec![],
                span: Span::new(FileId::new(0), 0, 0),
            });
        }),
        Box::new(|v| {
            v.adapter.words.push(flick_ir::AdapterWord {
                name: "cancel".into(),
                args: vec![flick_ir::AdapterArg::Ident("signal".into())],
                span: Span::new(FileId::new(0), 0, 0),
            });
        }),
        Box::new(|v| v.adapter.optional.push(0)),
        Box::new(|v| v.adapter.owned.push(0)),
        Box::new(|v| {
            v.adapter.words.push(flick_ir::AdapterWord {
                name: "variadic".into(),
                args: vec![],
                span: Span::new(FileId::new(0), 0, 0),
            });
        }),
        Box::new(|v| v.adapter.result.nullable = true),
        Box::new(|v| v.adapter.result.unit = true),
        Box::new(|v| v.adapter.callbacks.clear()),
        Box::new(|v| v.adapter.callbacks.push(None)),
        Box::new(|v| v.adapter.copies.args.clear()),
        Box::new(|v| v.adapter.copies.args.push(false)),
        Box::new(|v| v.adapter.copies.result = true),
        Box::new(|v| v.adapter.copies.callbacks.clear()),
        Box::new(|v| v.adapter.copies.callbacks.push(None)),
        Box::new(|v| v.adapter.shapes.clear()),
        Box::new(|v| v.adapter.shapes.push(Shape::Data)),
        Box::new(|v| v.adapter.result_shape = Shape::Layer),
    ];
    for i in 0..arity {
        changes.push(Box::new(move |v| v.stages[i] = true));
        changes.push(Box::new(move |v| v.adapter.callbacks[i] = Some(1)));
        changes.push(Box::new(move |v| v.adapter.copies.args[i] = true));
        changes.push(Box::new(move |v| {
            v.adapter.copies.callbacks[i] = Some((vec![true], false));
        }));
        changes.push(Box::new(move |v| v.adapter.shapes[i] = Shape::Layer));
    }
    changes
}

#[test]
fn exact_live_int_and_length_metadata_is_not_generic_foreign_erasure() {
    let mut seen = Vec::new();
    for module in build::builtin_modules() {
        for (index, item) in module.items.iter().enumerate() {
            let ItemKind::Foreign(original) = &item.kind else {
                continue;
            };
            if !matches!(
                (module.name.as_str(), original.binding.name.as_str()),
                ("Int", "intUnsafeDiv" | "intUnsafeRem") | ("String", "length")
            ) {
                continue;
            }
            seen.push((module.name.clone(), original.binding.name.clone()));
            let entry = memory::Name::new(&module.name, &original.name);
            let program = perceus::lower([module], &entry).expect("canonical adapter lowers");
            assert_eq!(memory::validate_program(&program), Ok(()));
            for (mutation_index, mutate) in mutations(original.arity).into_iter().enumerate() {
                let mut changed = module.clone();
                let ItemKind::Foreign(value) = &mut changed.items[index].kind else {
                    panic!("foreign item")
                };
                mutate(value);
                let failure =
                    perceus::lower([&changed], &entry).expect_err("changed contract must reject");
                assert!(
                    failure
                        .to_string()
                        .starts_with("memory lowering does not yet support foreign binding"),
                    "{} mutation{mutation_index}: {failure}",
                    original.binding.name
                );
                assert!(
                    failure.to_string().contains(&format!("at {:?}", item.span)),
                    "original declaration span"
                );
            }
        }
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            ("Int".to_owned(), "intUnsafeDiv".to_owned()),
            ("Int".to_owned(), "intUnsafeRem".to_owned()),
            ("String".to_owned(), "length".to_owned()),
        ]
    );
}
