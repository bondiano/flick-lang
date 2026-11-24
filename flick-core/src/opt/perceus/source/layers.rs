//! Exact constant-Layer adapters; no generic host-copy erasure.
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

use super::Intrinsic;

pub(super) fn intrinsic(value: &Foreign) -> Option<Intrinsic> {
    if value.binding.from() != Some("flick:rt")
        || value.dicts != 0
        || value.arity != 2
        || value.catch.is_some()
    {
        return None;
    }
    let (kind, mode, stages, callbacks, copies, shapes, result_shape) =
        match value.binding.export_name().unwrap_or("") {
            "layerSucceed" => (
                Intrinsic::LayerSucceed,
                ForeignMode::Pure,
                [false, false],
                vec![None, None],
                Copies {
                    args: vec![false, false],
                    result: false,
                    callbacks: vec![None, None],
                },
                vec![Shape::Data, Shape::Data],
                Shape::Layer,
            ),
            "layerMerge" | "layerOverride" => (
                Intrinsic::LayerMerge,
                ForeignMode::Pure,
                [false, false],
                vec![None, None],
                Copies {
                    args: vec![false, false],
                    result: false,
                    callbacks: vec![None, None],
                },
                vec![Shape::Layer, Shape::Layer],
                Shape::Layer,
            ),
            "layerRun" => (
                Intrinsic::LayerRun,
                ForeignMode::Effect,
                [false, true],
                vec![None, Some(1)],
                Copies {
                    args: vec![false, false],
                    result: true,
                    callbacks: vec![None, Some((vec![false], false))],
                },
                vec![
                    Shape::Layer,
                    Shape::Fun {
                        args: vec![Shape::Data],
                        result: Box::new(Shape::Data),
                        runtime: true,
                        sync: false,
                    },
                ],
                Shape::Data,
            ),
            _ => return None,
        };
    // LayerRun's host result-copy contract is discharged ONLY for this exact
    // internal adapter: immutable counted results transfer an owned token,
    // while every surviving alias retains its own token. Nothing crosses a
    // mutable host boundary. Neither callback nor implementation is rewritten.
    (value.mode == mode
        && value.stages == stages
        && value.adapter
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks,
                copies,
                shapes,
                result_shape,
                words: vec![],
            })
    .then_some(kind)
}

#[cfg(test)]
mod tests {
    use super::intrinsic;
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit, Module, memory, tests::lower_text};
    use crate::opt::perceus::source::{LowerError, lower};
    use crate::source::{FileId, Span};
    use flick_ir::{Foreign, ForeignMode, Shape};

    #[test]
    fn live_layer_builtins_match_exactly_and_mutations_preserve_rejection_spans() {
        for binding in ["layerSucceed", "layerRun", "layerMerge", "layerOverride"] {
            let module =
                builtin_modules()
                    .iter()
                    .find(|module| {
                        module.items.iter().any(|item| matches!(
                    &item.kind, ItemKind::Foreign(value) if value.binding.export_name() == Some(binding)
                ))
                    })
                    .expect("live Layer builtin module");
            let (span, original) = module
                .items
                .iter()
                .find_map(|item| match &item.kind {
                    ItemKind::Foreign(value) if value.binding.export_name() == Some(binding) => {
                        Some((item.span, value))
                    }
                    _ => None,
                })
                .expect("live Layer binding");
            assert!(intrinsic(original).is_some(), "{original:#?}");
            let entry = memory::Name::new(&module.name, &original.name);
            assert!(lower([module], &entry).is_ok());
            let mutations = common_mutations();
            for (index, mutate) in mutations.into_iter().enumerate() {
                reject(module, &entry, span, &original.name, index, mutate);
            }
            if binding == "layerRun" {
                let nested = run_mutations();
                for (index, mutate) in nested.into_iter().enumerate() {
                    reject(module, &entry, span, &original.name, index, mutate);
                }
            }
        }
    }

    #[test]
    fn service_tag_preserves_the_exact_nominal_key_as_an_owned_operation() {
        let text = "module App exposing (answer)\nservice Read =\n  read : Unit ->{} String\n\nlet answer = Read\n";
        let module = lower_text("App", text).expect("checked ServiceTag source");
        let program = lower([&module], &memory::Name::new("App", "answer"))
            .expect("nominal service tag lowering");
        let operations: Vec<_> = program
            .functions
            .values()
            .flat_map(|function| {
                function
                    .body
                    .instructions
                    .iter()
                    .filter_map(|instruction| match instruction {
                        memory::Instruction::Let { value, .. } => Some(value),
                        _ => None,
                    })
            })
            .collect();
        assert!(operations.iter().any(|op| matches!(op,
            memory::Operation::ServiceTag(key) if key == "App.Read")));
        assert!(!operations.iter().any(|op| matches!(op,
            memory::Operation::Literal(Lit::String(key)) if key == "App.Read")));
        assert_eq!(memory::validate_program(&program), Ok(()));
    }

    fn common_mutations() -> Vec<fn(&mut Foreign)> {
        vec![
            |v| {
                v.binding = flick_ir::Binding::Global {
                    name: v.binding.export_name().unwrap_or("").to_owned(),
                }
            },
            |v| {
                v.binding = flick_ir::Binding::Module {
                    from: "other".into(),
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
            |v| v.arity = 1,
            |v| v.arity = 3,
            |v| v.dicts = 1,
            |v| v.stages[0] = !v.stages[0],
            |v| v.stages[1] = !v.stages[1],
            |v| v.stages.clear(),
            |v| v.stages.push(false),
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
            |v| v.adapter.callbacks[1] = Some(2),
            |v| v.adapter.callbacks.clear(),
            |v| v.adapter.callbacks.push(None),
            |v| v.adapter.copies.args[0] = true,
            |v| v.adapter.copies.args[1] = true,
            |v| v.adapter.copies.args.clear(),
            |v| v.adapter.copies.args.push(false),
            |v| v.adapter.copies.result = !v.adapter.copies.result,
            |v| v.adapter.copies.callbacks[0] = Some((vec![false], false)),
            |v| v.adapter.copies.callbacks[1] = Some((vec![true], false)),
            |v| v.adapter.copies.callbacks[1] = Some((vec![false], true)),
            |v| v.adapter.copies.callbacks.clear(),
            |v| v.adapter.copies.callbacks.push(None),
            |v| v.adapter.shapes[0] = Shape::Option(Box::new(Shape::Data)),
            |v| {
                v.adapter.shapes[1] = if v.adapter.shapes[1] == Shape::Layer {
                    Shape::Data
                } else {
                    Shape::Layer
                };
            },
            |v| v.adapter.shapes.clear(),
            |v| v.adapter.shapes.push(Shape::Data),
            |v| v.adapter.result_shape = Shape::Option(Box::new(Shape::Data)),
        ]
    }

    fn run_mutations() -> Vec<fn(&mut Foreign)> {
        vec![
            |v| v.adapter.callbacks[1] = None,
            |v| v.adapter.copies.callbacks[1] = None,
            |v| v.adapter.copies.callbacks[1] = Some((vec![], false)),
            |v| v.adapter.copies.callbacks[1] = Some((vec![false, false], false)),
            |v| {
                if let Shape::Fun { args, .. } = &mut v.adapter.shapes[1] {
                    args.clear();
                }
            },
            |v| {
                if let Shape::Fun { args, .. } = &mut v.adapter.shapes[1] {
                    args.push(Shape::Data);
                }
            },
            |v| {
                if let Shape::Fun { args, .. } = &mut v.adapter.shapes[1] {
                    args[0] = Shape::Layer;
                }
            },
            |v| {
                if let Shape::Fun { result, .. } = &mut v.adapter.shapes[1] {
                    **result = Shape::Layer;
                }
            },
            |v| {
                if let Shape::Fun { runtime, .. } = &mut v.adapter.shapes[1] {
                    *runtime = false;
                }
            },
            |v| {
                if let Shape::Fun { sync, .. } = &mut v.adapter.shapes[1] {
                    *sync = true;
                }
            },
        ]
    }

    #[test]
    fn all_dynamic_layer_builtins_lower_cleanly() {
        let builtins = builtin_modules();
        let module = builtins
            .iter()
            .find(|module| module.name == "Layer")
            .expect("Layer module");
        let mut lowered = 0;
        for item in &module.items {
            let ItemKind::Foreign(value) = &item.kind else {
                continue;
            };
            let entry = memory::Name::new(&module.name, &value.name);
            assert!(
                lower(builtins.iter(), &entry).is_ok(),
                "Layer.{} failed to lower: {:?}",
                value.name,
                lower(builtins.iter(), &entry).err()
            );
            lowered += 1;
        }
        assert!(lowered >= 25);
    }

    fn reject(
        module: &Module,
        entry: &memory::Name,
        span: Span,
        name: &str,
        index: usize,
        mutate: fn(&mut Foreign),
    ) {
        let mut changed = module.clone();
        let foreign = changed
            .items
            .iter_mut()
            .find_map(|item| match &mut item.kind {
                ItemKind::Foreign(value) if value.name == name => Some(value),
                _ => None,
            })
            .expect("copied binding");
        mutate(foreign);
        assert!(intrinsic(foreign).is_none(), "{name} mutation {index}");
        assert!(
            matches!(lower([&changed], entry), Err(LowerError::Unsupported { span: actual, .. }) if actual == span),
            "{name} mutation {index}"
        );
    }
}
