//! Scalar adapters pin all actual builtin declarations, not a guessed Int-only subset.
use flick_core::build;
use flick_core::ir::{Expr, ExprKind, Foreign, ItemKind, Lit, memory};
use flick_core::opt::perceus;
use flick_core::source::{FileId, Span};
use flick_ir::{ForeignMode, Shape};

type Mutation = Box<dyn Fn(&mut Foreign)>;
fn mutations(arity: usize) -> Vec<Mutation> {
    let mut rejections: Vec<Mutation> = vec![
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
            v.adapter.words.push(flick_ir::BackendAdapter {
                name: "this".into(),
                args: vec![],
                span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
            });
        }),
        Box::new(|v| {
            v.adapter.words.push(flick_ir::BackendAdapter {
                name: "cancel".into(),
                args: vec![flick_ir::AdapterArg::Word("signal".into())],
                span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
            });
        }),
        Box::new(|v| v.adapter.optional.push(0)),
        Box::new(|v| {
            v.adapter.words.push(flick_ir::BackendAdapter {
                name: "variadic".into(),
                args: vec![],
                span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0),
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
        rejections.push(Box::new(move |v| v.stages[i] = true));
        rejections.push(Box::new(move |v| v.adapter.callbacks[i] = Some(1)));
        rejections.push(Box::new(move |v| v.adapter.copies.args[i] = true));
        rejections.push(Box::new(move |v| {
            v.adapter.copies.callbacks[i] = Some((vec![true], false));
        }));
        rejections.push(Box::new(move |v| v.adapter.shapes[i] = Shape::Layer));
    }
    rejections
}

#[test]
fn all_canonical_number_and_minmax_bindings_require_the_complete_exact_adapter() {
    let mut seen = Vec::new();
    let mut accepted_copies = 0;
    for module in build::builtin_modules() {
        for (index, item) in module.items.iter().enumerate() {
            let ItemKind::Foreign(original) = &item.kind else {
                continue;
            };
            if !((original.binding.from.is_none() && original.binding.name == "Number")
                || (original.binding.from.as_deref() == Some("flick:Number")
                    && matches!(original.binding.name.as_str(), "min" | "max")))
            {
                continue;
            }
            seen.push((
                module.name.as_str(),
                original.name.as_str(),
                original.binding.name.as_str(),
            ));
            let entry = memory::Name::new(&module.name, &original.name);
            let program = perceus::lower_with_declarations(
                [module],
                build::builtin_declarations().iter().map(AsRef::as_ref),
                &entry,
            )
            .expect("canonical scalar adapter lowers");
            assert_eq!(memory::validate_program(&program), Ok(()));
            let mut rejections = mutations(original.arity);
            if original.stages.len() > original.arity {
                rejections.push(Box::new(|v| v.stages[1] = true));
                rejections.push(Box::new(|v| {
                    let _ = v.stages.pop();
                }));
            }
            rejections.retain(|mutate| {
                let mut changed = original.clone();
                mutate(&mut changed);
                changed != *original
            });
            for (mutation_index, mutate) in rejections.into_iter().enumerate() {
                let mut changed = module.clone();
                let ItemKind::Foreign(value) = &mut changed.items[index].kind else {
                    panic!("foreign item")
                };
                mutate(value);
                let mut copied_input = original.clone();
                copied_input.adapter.copies.args[0] = true;
                if original.binding.from.is_none() && *value == copied_input {
                    // Identity-copy is an audited variant ONLY after the exact
                    // checked numeric-domain proof. It does not relax any
                    // other metadata field or the no-evidence entry point.
                    let program = perceus::lower_with_declarations(
                        [&changed],
                        build::builtin_declarations().iter().map(AsRef::as_ref),
                        &entry,
                    )
                    .expect("checked numeric input copy has identity semantics");
                    assert_eq!(memory::validate_program(&program), Ok(()));
                    let missing = perceus::lower([&changed], &entry)
                        .expect_err("copy bit cannot manufacture type evidence");
                    assert!(
                        missing
                            .to_string()
                            .contains("requires a checked declaration context")
                    );
                    accepted_copies += 1;
                    continue;
                }
                let failure = perceus::lower_with_declarations(
                    [&changed],
                    build::builtin_declarations().iter().map(AsRef::as_ref),
                    &entry,
                )
                .expect_err("changed scalar contract rejects");
                assert!(
                    failure
                        .to_string()
                        .starts_with("memory lowering does not yet support foreign binding"),
                    "{} mutation{mutation_index}: {failure}",
                    original.binding.name
                );
                assert!(
                    failure.to_string().contains(&format!("at {:?}", item.span)),
                    "declaration span preserved"
                );
            }
        }
    }
    assert_eq!(
        accepted_copies, 4,
        "one explicit input-copy variant for each registered Number binding"
    );
    seen.sort_unstable();
    assert_eq!(
        seen,
        [
            ("BigInt", "to_number", "Number"),
            ("Float", "max", "max"),
            ("Float", "min", "min"),
            ("Instances", "float_of_int", "Number"),
            ("Int", "to_float", "Number"),
            ("Numeric", "float_of_bigint", "Number"),
        ]
    );
}

#[test]
fn no_evidence_entrypoint_does_not_guess_scalar_domains_from_builtin_names() {
    let mut rejected = Vec::new();
    for module in build::builtin_modules() {
        for item in &module.items {
            let ItemKind::Foreign(value) = &item.kind else {
                continue;
            };
            if (value.binding.from.is_none() && value.binding.name == "Number")
                || (value.binding.from.as_deref() == Some("flick:Number")
                    && matches!(value.binding.name.as_str(), "min" | "max"))
            {
                let entry = memory::Name::new(&module.name, &value.name);
                let error =
                    perceus::lower([module], &entry).expect_err("no hidden builtin/name allowlist");
                assert!(
                    error
                        .to_string()
                        .contains("requires a checked declaration context")
                );
                rejected.push((module.name.as_str(), value.name.as_str()));
            }
        }
    }
    rejected.sort_unstable();
    assert_eq!(
        rejected,
        [
            ("BigInt", "to_number"),
            ("Float", "max"),
            ("Float", "min"),
            ("Instances", "float_of_int"),
            ("Int", "to_float"),
            ("Numeric", "float_of_bigint"),
        ]
    );
}
