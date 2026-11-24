#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::{LowerError, free::free, lower};
    use super::*;
    use crate::{
        build::builtin_modules,
        ir::{ItemKind, memory},
        source::FileId,
    };

    #[test]
    fn every_live_contract_lowers_to_closed_pure_ordinary_code() {
        let mut contracts = BTreeSet::new();
        let mut mutation_count = 0;
        for module in builtin_modules() {
            for item in &module.items {
                let ItemKind::Foreign(foreign) = &item.kind else {
                    continue;
                };
                let Some(kind) = kind(foreign) else { continue };
                contracts.insert((
                    module.name.as_str(),
                    foreign.name.as_str(),
                    foreign.binding.export_name().unwrap_or(""),
                ));
                assert!(
                    supports(foreign, kind),
                    "{}.{}: {foreign:#?}",
                    module.name,
                    foreign.name
                );
                let template = expression(foreign, item.span).expect("exact live adapter");
                assert!(free(&template).is_empty());
                pure(&template);
                assert_eq!(
                    callback_mentions(&template),
                    1,
                    "one callback application site per loop iteration"
                );
                let program = lower([module], &memory::Name::new(&module.name, &foreign.name))
                    .expect("ordinary template lowers");
                assert_eq!(memory::validate_program(&program), Ok(()));
                assert!(program.functions.len() > 2);
                for (index, mutate) in mutations(foreign).into_iter().enumerate() {
                    let mut changed = module.clone();
                    let ItemKind::Foreign(value) = &mut changed
                        .items
                        .iter_mut()
                        .find(|candidate| candidate.span == item.span)
                        .expect("declaration")
                        .kind
                    else {
                        unreachable!()
                    };
                    mutate(value);
                    mutation_count += 1;
                    assert!(
                        expression(value, item.span).is_none(),
                        "{} mutation {index}: {value:#?}",
                        foreign.name
                    );
                    assert!(
                        matches!(lower([&changed], &memory::Name::new(&module.name, &foreign.name)), Err(LowerError::Unsupported { span, .. }) if span == item.span),
                        "{} mutation {index} cannot erase metadata",
                        foreign.name
                    );
                }
            }
        }
        assert_eq!(
            contracts,
            BTreeSet::from([
                ("Instances", "compare_list", "listCompare"),
                ("Instances", "eq_list", "listEquals"),
                ("Instances", "fold_list", "listFold"),
                ("Instances", "hash_list", "listHash"),
                ("Instances", "map_list", "listMap"),
                ("Instances", "map2_list", "listMap2"),
                ("Instances", "flat_map_list", "listFlatMap"),
                ("List", "map", "listMap"),
                ("List.Core", "fold_left", "listFold"),
                ("List.Core", "flat_map", "listFlatMap"),
                ("List.Core", "fold_right", "listFoldRight"),
                ("NonEmpty", "map", "listMap"),
            ]),
            "canonical declarations in Instances.fl, List.fl, List/Core.fl and NonEmpty.fl"
        );
        eprintln!(
            "{} exact live contracts, {mutation_count} rejected metadata mutations",
            contracts.len()
        );
    }
    pub(super) fn callback_mentions(expression: &Expr) -> usize {
        let mut count = usize::from(
            matches!(&expression.kind, ExprKind::Local(name) if name == "$collection.callback"),
        );
        expression.each_child(&mut |child| count += callback_mentions(child));
        count
    }
    pub(super) fn pure(expr: &Expr) {
        match &expr.kind {
            ExprKind::App {
                stage, arg_first, ..
            } => {
                assert_eq!(*stage, Stage::Pure);
                assert!(!arg_first);
            }
            ExprKind::Lambda { params, .. } => {
                assert!(params.iter().all(|p| p.stage == Stage::Pure));
            }
            ExprKind::Global { .. }
            | ExprKind::Task(_)
            | ExprKind::Await(_)
            | ExprKind::Within(..) => panic!("no host or runtime shortcut"),
            _ => {}
        }
        expr.each_child(&mut pure);
    }
    type Mutation = Box<dyn Fn(&mut Foreign)>;
    pub(super) fn mutations(original: &Foreign) -> Vec<Mutation> {
        let mut mutations: Vec<Mutation> = vec![
            Box::new(|f| f.binding = flick_ir::Binding::Global { name: f.binding.export_name().unwrap_or("").to_owned() }),
            Box::new(|f| f.binding = flick_ir::Binding::Module { from: "other".into(), name: f.binding.export_name().unwrap_or("").to_owned() }),
            Box::new(|f| if let flick_ir::Binding::Module { name, .. } | flick_ir::Binding::Global { name } = &mut f.binding { name.push('!'); }),
            Box::new(|f| f.mode = ForeignMode::Sync),
            Box::new(|f| f.mode = ForeignMode::Async),
            Box::new(|f| f.mode = ForeignMode::Effect),
            Box::new(|f| f.dicts = 1),
            Box::new(|f| f.arity += 1),
            Box::new(|f| f.arity = 0),
            Box::new(|f| f.stages.clear()),
            Box::new(|f| f.stages.push(false)),
            Box::new(|f| {
                f.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            }),
            Box::new(|f| f.adapter.words.push(flick_ir::BackendAdapter { name: "this".into(), args: vec![], span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0) })),
            Box::new(|f| f.adapter.words.push(flick_ir::BackendAdapter { name: "cancel".into(), args: vec![flick_ir::AdapterArg::Word("signal".into())], span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0) })),
            Box::new(|f| f.adapter.optional.push(1)),
            Box::new(|f| f.adapter.words.push(flick_ir::BackendAdapter { name: "variadic".into(), args: vec![], span: flick_ir::Span::new(flick_ir::FileId::new(0), 0, 0) })),
            Box::new(|f| f.adapter.result.nullable = true),
            Box::new(|f| f.adapter.result.unit = true),
            Box::new(|f| f.adapter.callbacks.clear()),
            Box::new(|f| f.adapter.callbacks.push(None)),
            Box::new(|f| f.adapter.copies.args.clear()),
            Box::new(|f| f.adapter.copies.args.push(false)),
            Box::new(|f| f.adapter.copies.result = !f.adapter.copies.result),
            Box::new(|f| f.adapter.copies.callbacks.clear()),
            Box::new(|f| f.adapter.copies.callbacks.push(None)),
            Box::new(|f| f.adapter.shapes.clear()),
            Box::new(|f| f.adapter.shapes.push(Shape::Data)),
            Box::new(|f| f.adapter.result_shape = Shape::Layer),
        ];
        if original.adapter.copies.callbacks[0].is_some() {
            mutations.push(Box::new(|f| {
                f.adapter.copies.callbacks[0].as_mut().expect("callback").1 =
                    !f.adapter.copies.callbacks[0].as_ref().expect("callback").1;
            }));
            mutations.push(Box::new(|f| {
                f.adapter.copies.callbacks[0]
                    .as_mut()
                    .expect("callback")
                    .0
                    .push(true);
            }));
        }
        for index in 0..original.arity {
            mutations.push(Box::new(move |f| f.stages[index] = true));
            mutations.push(Box::new(move |f| {
                f.adapter.callbacks[index] = if index == 0 { None } else { Some(1) }
            }));
            mutations.push(Box::new(move |f| f.adapter.callbacks[index] = Some(99)));
            mutations.push(Box::new(move |f| {
                f.adapter.copies.args[index] = !f.adapter.copies.args[index];
            }));
            mutations.push(Box::new(move |f| f.adapter.copies.callbacks[index] = None));
            if index > 0 {
                mutations.push(Box::new(move |f| {
                    f.adapter.copies.callbacks[index] = Some((vec![true], false));
                }));
            }
            mutations.push(Box::new(move |f| f.adapter.shapes[index] = Shape::Layer));
        }
        // Non-callback slots already contain None; only mutate real changes.
        mutations.retain(|mutate| {
            let mut changed = original.clone();
            mutate(&mut changed);
            changed != *original
        });
        for index in 0..original.adapter.copies.callbacks[0]
            .as_ref()
            .map_or(0, |copy| copy.0.len())
        {
            mutations.push(Box::new(move |f| {
                f.adapter.copies.callbacks[0].as_mut().expect("callback").0[index] = false;
            }));
        }
        mutations
    }
}

#[cfg(test)]
mod sequencing_tests {
    use super::{Builder, Collection};
    use crate::{
        ir::{Expr, ExprKind, Stmt},
        source::{FileId, Span},
    };

    fn callee(expression: &Expr) -> &str {
        match &expression.kind {
            ExprKind::Local(name) => name,
            ExprKind::App { func, .. } => callee(func),
            _ => panic!("ordinary pure local application"),
        }
    }
    fn arguments(expression: &Expr) -> Vec<&str> {
        let mut expression = expression;
        let mut args = Vec::new();
        while let ExprKind::App { func, arg, .. } = &expression.kind {
            let ExprKind::Local(name) = &arg.kind else {
                panic!("explicitly bound local argument")
            };
            args.push(name.as_str());
            expression = func;
        }
        args.reverse();
        args
    }
    #[test]
    fn right_fold_recurses_before_the_first_callback_stage_with_head_then_accumulator() {
        let builder = Builder(Span::new(FileId::new(0), 0, 0));
        let expression = builder.collection(Collection::FoldRight);
        let ExprKind::Lambda { params, body } = &expression.kind else {
            panic!("callback stage")
        };
        assert_eq!(params.len(), 1);
        let ExprKind::Block(functions, final_stages) = &body.kind else {
            panic!("closed local loop")
        };
        assert_eq!(functions.len(), 1);
        let ExprKind::Lambda { params, body } = &final_stages.kind else {
            panic!("seed and input remain callable stages")
        };
        assert_eq!(params.len(), 2);
        assert_eq!(callee(body), "$collection.loop");
        assert_eq!(arguments(body), ["$collection.acc", "$collection.input"]);
        let Stmt::Fun { value, .. } = &functions[0] else {
            panic!("loop")
        };
        let ExprKind::Lambda { body, .. } = &value.kind else {
            panic!("loop parameters")
        };
        let ExprKind::Match(_, arms) = &body.kind else {
            panic!("list traversal")
        };
        let ExprKind::Block(statements, callback) = &arms[1].body.kind else {
            panic!("explicit recursion-before-callback sequencing")
        };
        assert_eq!(statements.len(), 1);
        let Stmt::Let { value: recur, .. } = &statements[0] else {
            panic!("tail result bound first")
        };
        assert_eq!(callee(recur), "$collection.loop");
        assert_eq!(arguments(recur), ["$collection.acc", "$collection.tail"]);
        assert_eq!(callee(callback), "$collection.callback");
        assert_eq!(
            arguments(callback),
            ["$collection.head", "$collection.rest"]
        );
    }
    #[test]
    fn map_hash_and_fold_evaluate_callback_once_before_recursive_step() {
        let builder = Builder(Span::new(FileId::new(0), 0, 0));
        for kind in [Collection::Map, Collection::Hash, Collection::Fold] {
            let expression = builder.collection(kind);
            let ExprKind::Lambda { body, .. } = &expression.kind else {
                panic!("callback stage")
            };
            let ExprKind::Block(functions, _) = &body.kind else {
                panic!("local recursion")
            };
            let Stmt::Fun { value, .. } = &functions[0] else {
                panic!("loop")
            };
            let ExprKind::Lambda { body, .. } = &value.kind else {
                panic!("loop parameters")
            };
            let ExprKind::Match(_, arms) = &body.kind else {
                panic!("list traversal")
            };
            let ExprKind::Block(statements, result) = &arms[1].body.kind else {
                panic!("sequenced evaluation")
            };
            let Stmt::Let {
                value: callback, ..
            } = &statements[0]
            else {
                panic!("callback result bound once")
            };
            assert_eq!(callee(callback), "$collection.callback");
            match kind {
                Collection::Map => {
                    assert_eq!(statements.len(), 2);
                    let Stmt::Let { value: recur, .. } = &statements[1] else {
                        panic!("tail bound after callback")
                    };
                    assert_eq!(callee(recur), "$collection.loop");
                }
                Collection::Hash | Collection::Fold => {
                    assert_eq!(statements.len(), 1);
                    assert_eq!(callee(result), "$collection.loop");
                }
                _ => unreachable!(),
            }
        }
    }
}
