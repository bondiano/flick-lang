//! Exact global `BigInt` metadata is necessary, never a substitute for type proof.
use flick_core::{
    build,
    ir::{Expr, ExprKind, Foreign, ItemKind, Lit, memory},
    opt::perceus,
    source::{FileId, Span},
};
use flick_ir::{ForeignMode, Shape};

type Mutation = Box<dyn Fn(&mut Foreign)>;
fn mutations() -> Vec<Mutation> {
    vec![
        Box::new(|v| v.binding.from = Some("other".into())),
        Box::new(|v| v.binding.name.push('!')),
        Box::new(|v| v.mode = ForeignMode::Sync),
        Box::new(|v| v.mode = ForeignMode::Async),
        Box::new(|v| v.mode = ForeignMode::Effect),
        Box::new(|v| v.arity = 0),
        Box::new(|v| v.arity = 2),
        Box::new(|v| v.dicts = 1),
        Box::new(|v| v.stages.clear()),
        Box::new(|v| v.stages.push(false)),
        Box::new(|v| v.stages[0] = true),
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
        Box::new(|v| v.adapter.callbacks[0] = Some(1)),
        Box::new(|v| v.adapter.copies.args.clear()),
        Box::new(|v| v.adapter.copies.args.push(false)),
        Box::new(|v| v.adapter.copies.callbacks.clear()),
        Box::new(|v| v.adapter.copies.callbacks.push(None)),
        Box::new(|v| v.adapter.copies.callbacks[0] = Some((vec![false], false))),
        Box::new(|v| v.adapter.shapes.clear()),
        Box::new(|v| v.adapter.shapes.push(Shape::Data)),
        Box::new(|v| v.adapter.shapes[0] = Shape::Layer),
        Box::new(|v| v.adapter.result_shape = Shape::Layer),
    ]
}

#[test]
fn every_real_bigint_global_requires_type_evidence_and_the_complete_audited_adapter() {
    let mut seen = Vec::new();
    let mut rejected = 0;
    let mut accepted = 0;
    for module in build::builtin_modules() {
        for (index, item) in module.items.iter().enumerate() {
            let ItemKind::Foreign(original) = &item.kind else {
                continue;
            };
            if original.binding.from.is_some() || original.binding.name != "BigInt" {
                continue;
            }
            seen.push((module.name.as_str(), original.name.as_str()));
            let entry = memory::Name::new(&module.name, &original.name);
            for input_copy in [false, true] {
                for result_copy in [false, true] {
                    let mut changed = module.clone();
                    let ItemKind::Foreign(value) = &mut changed.items[index].kind else {
                        unreachable!()
                    };
                    value.adapter.copies.args = vec![input_copy];
                    value.adapter.copies.result = result_copy;
                    let program = perceus::lower_with_declarations(
                        [&changed],
                        build::builtin_declarations().iter().map(AsRef::as_ref),
                        &entry,
                    )
                    .expect(
                        "both copy bits are identities after the checked primitive/BigInt proof",
                    );
                    assert_eq!(memory::validate_program(&program), Ok(()));
                    let error = perceus::lower([&changed], &entry)
                        .expect_err("no evidence, no guessed global domain");
                    assert!(
                        error
                            .to_string()
                            .contains("BigInt conversion requires a checked declaration context")
                    );
                    accepted += 1;
                }
            }
            for mutate in mutations() {
                let mut changed = module.clone();
                let ItemKind::Foreign(value) = &mut changed.items[index].kind else {
                    unreachable!()
                };
                mutate(value);
                let error = perceus::lower_with_declarations(
                    [&changed],
                    build::builtin_declarations().iter().map(AsRef::as_ref),
                    &entry,
                )
                .expect_err("every other contract mutation rejects");
                assert!(
                    error
                        .to_string()
                        .contains("memory lowering does not yet support foreign binding")
                );
                assert!(error.to_string().contains(&format!("at {:?}", item.span)));
                rejected += 1;
            }
        }
    }
    seen.sort_unstable();
    assert_eq!(
        seen,
        [("Instances", "bigint_of_int"), ("Numeric", "bigint_of")]
    );
    assert_eq!(accepted, 8);
    assert_eq!(rejected, 60);
}
