//! Exact pure string adapters, never generic host-adapter erasure.
use super::kept_for_call;
use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

use super::Intrinsic;
use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use crate::source::Span;

/// Immutable text stays inside Flick: no host callback, owner, or mutable
/// reference crosses this exact adapter. Fresh tails/concatenation are a
/// quadratic-copy baseline, not an FBIP or linear-time join claim.
pub(super) fn expression(value: &Foreign, span: Span) -> Option<Expr> {
    supports_join(value).then(|| join(span))
}

fn supports_join(value: &Foreign) -> bool {
    value.binding.module_export() == Some(("flick:rt", "stringJoin"))
        && value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == 2
        && value.stages == [false, false]
        && value.catch.is_none()
        && kept_for_call(&value.adapter)
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None, None],
                copies: Copies {
                    args: vec![false, true],
                    result: false,
                    callbacks: vec![None, None],
                },
                shapes: vec![Shape::Data, Shape::Data],
                result_shape: Shape::Data,
                words: vec![],
                retention: None,
            }
}

fn join(span: Span) -> Expr {
    let at = |kind| Expr::new(kind, span);
    let local = |name: &str| at(ExprKind::Local(format!("$stringJoin.{name}")));
    let bind = |name: &str| Pat::Bind(format!("$stringJoin.{name}"));
    let lambda = |name: &str, body| {
        at(ExprKind::Lambda {
            params: vec![Param {
                pat: bind(name),
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            }],
            body: Box::new(body),
        })
    };
    let concat = |left, right| {
        at(ExprKind::Binary(
            BinOp::Concat,
            Box::new(left),
            Box::new(right),
        ))
    };
    let recurse = at(ExprKind::App {
        func: Box::new(local("loop")),
        arg: Box::new(local("tail")),
        stage: Stage::Pure,
        arg_first: false,
    });
    let body = at(ExprKind::Match(
        Box::new(local("items")),
        vec![
            Arm {
                pat: Pat::List(vec![], None),
                binder_types: Vec::new(),
                guard: None,
                body: at(ExprKind::Lit(Lit::String(String::new()))),
            },
            Arm {
                pat: Pat::List(vec![bind("head")], None),
                binder_types: Vec::new(),
                guard: None,
                body: local("head"),
            },
            Arm {
                pat: Pat::List(vec![bind("head")], Some(Box::new(bind("tail")))),
                binder_types: Vec::new(),
                guard: None,
                body: concat(local("head"), concat(local("separator"), recurse)),
            },
        ],
    ));
    lambda(
        "separator",
        at(ExprKind::Block(
            vec![Stmt::Fun {
                name: "$stringJoin.loop".into(),
                rec: true,
                type_params: Vec::new(),
                dicts: vec![],
                dict_constraints: Vec::new(),
                ty: None,
                value: lambda("items", body),
            }],
            Box::new(local("loop")),
        )),
    )
}

pub(super) fn intrinsic(value: &Foreign) -> Option<Intrinsic> {
    let kind = match value.binding.export_name().unwrap_or("") {
        "startsWith" => Intrinsic::StringStartsWith,
        "includes" => Intrinsic::StringIncludes,
        "length" => Intrinsic::StringLength,
        _ => return None,
    };
    // Host arity is one; predicates return another unary pure function.
    // Counted closures preserve those stages and subject-last order.
    (value.binding.from() == Some("flick:String")
        && value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == 1
        && value.stages == vec![false; kind.arity()]
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
                    args: vec![false],
                    result: false,
                    callbacks: vec![None],
                },
                shapes: vec![Shape::Data],
                result_shape: Shape::Data,
                words: vec![],
                retention: None,
            })
    .then_some(kind)
}

#[cfg(test)]
mod tests {
    use super::{expression, intrinsic};
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit, memory};
    use crate::opt::perceus::source::{LowerError, lower};
    use crate::source::{FileId, Span};
    use flick_ir::{Foreign, ForeignMode, Shape};

    #[test]
    fn exact_live_string_adapters_and_every_metadata_field_are_checked() {
        let mut checked = 0;
        for module in builtin_modules()
            .iter()
            .filter(|m| matches!(m.name.as_str(), "String" | "Instances"))
        {
            for item in &module.items {
                let ItemKind::Foreign(original) = &item.kind else {
                    continue;
                };
                if original.binding.from() != Some("flick:String")
                    || !matches!(
                        original.binding.export_name().unwrap_or(""),
                        "startsWith" | "includes"
                    )
                {
                    continue;
                }
                checked += 1;
                assert!(
                    intrinsic(original).is_some(),
                    "canonical adapter: {original:#?}"
                );
                let entry = memory::Name::new(&module.name, &original.name);
                let program = lower([module], &entry).expect("live exact builtin");
                assert_eq!(memory::validate_program(&program), Ok(()));
                for (index, mutate) in mutations().into_iter().enumerate() {
                    let mut changed = module.clone();
                    let declaration = changed
                        .items
                        .iter_mut()
                        .find(|candidate| candidate.span == item.span)
                        .expect("declaration");
                    let ItemKind::Foreign(value) = &mut declaration.kind else {
                        panic!("foreign")
                    };
                    mutate(value);
                    assert!(
                        matches!(lower([&changed], &entry), Err(LowerError::Unsupported { span, .. }) if span == item.span),
                        "{}:{} mutation {index} must reject before execution at the declaration span",
                        module.name,
                        original.name
                    );
                }
            }
        }
        assert_eq!(checked, 4, "both predicates in String and Instances");
    }

    #[test]
    fn exact_live_join_adapter_and_every_metadata_field_are_checked() {
        let mut checked = 0;
        for module in builtin_modules() {
            for item in &module.items {
                let ItemKind::Foreign(original) = &item.kind else {
                    continue;
                };
                if original.binding.export_name() != Some("stringJoin") {
                    continue;
                }
                checked += 1;
                let template = expression(original, item.span).expect("canonical join contract");
                let captures = super::super::free::free(&template);
                assert!(captures.is_empty(), "closed template: {captures:?}");
                let entry = memory::Name::new(&module.name, &original.name);
                let program = lower([module], &entry).expect("exact builtin join lowers");
                assert_eq!(memory::validate_program(&program), Ok(()));
                let mut rejections = mutations();
                let second_argument: [fn(&mut Foreign); 4] = [
                    |v| v.adapter.callbacks[1] = Some(1),
                    |v| v.adapter.copies.args[1] = false,
                    |v| v.adapter.copies.callbacks[1] = Some((vec![true], false)),
                    |v| v.adapter.shapes[1] = Shape::Layer,
                ];
                rejections.extend(second_argument);
                for (index, mutate) in rejections.into_iter().enumerate() {
                    let mut changed = module.clone();
                    let declaration = changed
                        .items
                        .iter_mut()
                        .find(|candidate| candidate.span == item.span)
                        .expect("declaration");
                    let ItemKind::Foreign(value) = &mut declaration.kind else {
                        panic!("foreign")
                    };
                    mutate(value);
                    assert!(
                        matches!(lower([&changed], &entry), Err(LowerError::Unsupported { span, .. }) if span == item.span),
                        "{}:{} mutation {index} must reject at the declaration span",
                        module.name,
                        original.name
                    );
                }
            }
        }
        assert_eq!(checked, 1, "String.join's canonical declaration");
    }

    fn mutations() -> Vec<fn(&mut Foreign)> {
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
            |v| v.mode = ForeignMode::Async,
            |v| v.mode = ForeignMode::Effect,
            |v| v.arity = 0,
            |v| v.arity += 1,
            |v| v.dicts = 1,
            |v| v.stages[0] = true,
            |v| v.stages[1] = true,
            |v| {
                let _ = v.stages.pop();
            },
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
            |v| v.adapter.callbacks.clear(),
            |v| v.adapter.callbacks.push(None),
            |v| v.adapter.copies.args[0] = true,
            |v| v.adapter.copies.args.clear(),
            |v| v.adapter.copies.args.push(false),
            |v| v.adapter.copies.result = true,
            |v| v.adapter.copies.callbacks[0] = Some((vec![], false)),
            |v| v.adapter.copies.callbacks[0] = Some((vec![true], true)),
            |v| v.adapter.copies.callbacks.clear(),
            |v| v.adapter.copies.callbacks.push(None),
            |v| v.adapter.shapes[0] = Shape::Layer,
            |v| v.adapter.shapes.clear(),
            |v| v.adapter.shapes.push(Shape::Data),
            |v| v.adapter.result_shape = Shape::Layer,
            |v| {
                v.adapter.result_shape = Shape::Fun {
                    args: vec![Shape::Data],
                    result: Box::new(Shape::Data),
                    runtime: false,
                    sync: false,
                }
            },
        ]
    }
}
