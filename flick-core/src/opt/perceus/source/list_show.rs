//! Exact known listShow adapter, implemented as closed Flick IR.

use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use crate::source::Span;

/// Only this known pure host adapter is discharged by ordinary language
/// ownership. No arbitrary callback/copy/Effect bridge is silently erased.
pub(super) fn supports(value: &Foreign) -> bool {
    value.binding.from.as_deref() == Some("flick:rt")
        && value.binding.name == "listShow"
        && value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == 2
        && value.stages == [false, false]
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
                callbacks: vec![Some(1), None],
                copies: Copies {
                    args: vec![false, true],
                    result: false,
                    callbacks: vec![Some((vec![true], false)), None],
                },
                shapes: vec![Shape::Data, Shape::Data],
                result_shape: Shape::Data,
            }
}

/// A closed, pure renderer implemented through existing closure, match and
/// concatenation lowering. No host export occurs: elements stay immutable
/// owned values. Fresh bound tails and string concatenation make this a
/// quadratic-copy baseline, not an FBIP or linear-time rendering claim.
pub(super) fn expression(span: Span) -> Expr {
    const RENDER: &str = "$listShow.render";
    const LOOP: &str = "$listShow.loop";
    const ITEMS: &str = "$listShow.items";
    const INPUT: &str = "$listShow.input";
    const LAST: &str = "$listShow.last";
    const HEAD: &str = "$listShow.head";
    const TAIL: &str = "$listShow.tail";
    const SHOWN: &str = "$listShow.shown";
    const SUFFIX: &str = "$listShow.suffix";
    let at = |kind| Expr::new(kind, span);
    let local = |name: &str| at(ExprKind::Local(name.into()));
    let text = |value: &str| at(ExprKind::Lit(Lit::String(value.into())));
    let apply = |name: &str, arg: Expr| {
        at(ExprKind::App {
            func: Box::new(local(name)),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        })
    };
    let concat = |left, right| {
        at(ExprKind::Binary(
            BinOp::Concat,
            Box::new(left),
            Box::new(right),
        ))
    };
    let lambda = |name: &str, body| {
        at(ExprKind::Lambda {
            params: vec![Param {
                pat: Pat::Bind(name.into()),
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            }],
            body: Box::new(body),
        })
    };
    // A head's callback runs once, before any recursive tail rendering.
    // Returned strings are concatenated verbatim, never shown/quoted again.
    let more = at(ExprKind::Block(
        vec![
            Stmt::let_bind(Pat::Bind(SHOWN.into()), apply(RENDER, local(HEAD))),
            Stmt::let_bind(Pat::Bind(SUFFIX.into()), apply(LOOP, local(TAIL))),
        ],
        Box::new(concat(concat(local(SHOWN), text(", ")), local(SUFFIX))),
    ));
    let body = at(ExprKind::Match(
        Box::new(local(ITEMS)),
        vec![
            Arm {
                pat: Pat::List(vec![], None),
                binder_types: Vec::new(),
                guard: None,
                body: text(""),
            },
            Arm {
                pat: Pat::List(vec![Pat::Bind(LAST.into())], None),
                binder_types: Vec::new(),
                guard: None,
                body: apply(RENDER, local(LAST)),
            },
            Arm {
                pat: Pat::List(
                    vec![Pat::Bind(HEAD.into())],
                    Some(Box::new(Pat::Bind(TAIL.into()))),
                ),
                binder_types: Vec::new(),
                guard: None,
                body: more,
            },
        ],
    ));
    let rendered = concat(concat(text("["), apply(LOOP, local(INPUT))), text("]"));
    lambda(
        RENDER,
        at(ExprKind::Block(
            vec![Stmt::Fun {
                name: LOOP.into(),
                rec: true,
                type_params: vec![],
                dicts: vec![],
                dict_constraints: vec![],
                ty: None,
                value: lambda(ITEMS, body),
            }],
            Box::new(lambda(INPUT, rendered)),
        )),
    )
}

#[cfg(test)]
mod tests {
    use flick_ir::{Adapter, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

    use super::super::free::free;
    use super::super::{LowerError, lower};
    use super::expression;
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit, Module, Pat, Stage, Stmt, memory};
    use crate::source::{FileId, Span};

    fn module() -> Module {
        builtin_modules()
            .iter()
            .find(|module| module.name == "Instances")
            .expect("canonical instance module")
            .clone()
    }
    fn binding(module: &mut Module) -> &mut Foreign {
        module
            .items
            .iter_mut()
            .find_map(|item| match &mut item.kind {
                ItemKind::Foreign(value) if value.name == "show_list" => Some(value),
                _ => None,
            })
            .expect("canonical primitive listShow declaration")
    }
    fn adapter() -> Adapter {
        Adapter {
            receiver: false,
            cancel: false,
            optional: vec![],
            variadic: false,
            result: ResultAdapter {
                nullable: false,
                unit: false,
            },
            callbacks: vec![Some(1), None],
            copies: Copies {
                args: vec![false, true],
                result: false,
                callbacks: vec![Some((vec![true], false)), None],
            },
            shapes: vec![Shape::Data, Shape::Data],
            result_shape: Shape::Data,
        }
    }
    #[test]
    fn canonical_pure_list_show_adapter_lowers_without_an_intrinsic() {
        let mut module = module();
        let foreign = binding(&mut module);
        assert_eq!(foreign.mode, ForeignMode::Pure);
        assert_eq!(foreign.dicts, 0);
        assert_eq!(foreign.arity, 2);
        assert_eq!(foreign.stages, [false, false]);
        assert!(foreign.catch.is_none());
        assert_eq!(
            foreign.adapter,
            adapter(),
            "the accepted contract is pinned to the real builtin"
        );
        let program = lower([&module], &memory::Name::new("Instances", "show_list"))
            .expect("known pure listShow is synthesized as ordinary owned code");
        assert_eq!(memory::validate_program(&program), Ok(()));
        assert!(
            program.functions.len() > 2,
            "real recursive/matching code, not a host shortcut"
        );
    }
    fn changes() -> Vec<fn(&mut Foreign)> {
        vec![
            |f| f.binding.from = None,
            |f| f.binding.name = "listShowOther".into(),
            |f| f.mode = ForeignMode::Sync,
            |f| f.dicts = 1,
            |f| f.arity = 1,
            |f| f.stages = vec![false, true],
            |f| f.stages.push(false),
            |f| {
                let _ = f.stages.pop();
            },
            |f| {
                f.catch = Some(Expr::new(
                    ExprKind::Lit(Lit::Unit),
                    Span::new(FileId::new(0), 0, 0),
                ));
            },
            |f| f.adapter.receiver = true,
            |f| f.adapter.cancel = true,
            |f| f.adapter.optional.push(1),
            |f| f.adapter.variadic = true,
            |f| f.adapter.result.nullable = true,
            |f| f.adapter.result.unit = true,
            |f| f.adapter.callbacks[0] = Some(2),
            |f| f.adapter.callbacks[0] = None,
            |f| f.adapter.callbacks[1] = Some(1),
            |f| f.adapter.callbacks.push(None),
            |f| {
                let _ = f.adapter.callbacks.pop();
            },
            |f| f.adapter.copies.args[0] = true,
            |f| f.adapter.copies.args[1] = false,
            |f| f.adapter.copies.args.push(false),
            |f| {
                let _ = f.adapter.copies.args.pop();
            },
            |f| f.adapter.copies.result = true,
            |f| f.adapter.copies.callbacks[0] = Some((vec![false], false)),
            |f| f.adapter.copies.callbacks[0] = Some((vec![true], true)),
            |f| f.adapter.copies.callbacks[0] = Some((vec![true, true], false)),
            |f| f.adapter.copies.callbacks[0] = Some((vec![], false)),
            |f| f.adapter.copies.callbacks[0] = None,
            |f| f.adapter.copies.callbacks[1] = Some((vec![true], false)),
            |f| f.adapter.copies.callbacks.clear(),
            |f| {
                let _ = f.adapter.copies.callbacks.pop();
            },
            |f| f.adapter.copies.callbacks.push(None),
            |f| f.adapter.shapes[0] = Shape::Eff(Box::new(Shape::Data)),
            |f| f.adapter.shapes[1] = Shape::List(Box::new(Shape::Data)),
            |f| f.adapter.shapes.push(Shape::Data),
            |f| {
                let _ = f.adapter.shapes.pop();
            },
            |f| f.adapter.result_shape = Shape::Eff(Box::new(Shape::Data)),
        ]
    }

    #[test]
    fn list_show_rejects_all_unrecognized_adapter_changes() {
        let original = module();
        let declaration = original
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(value) if value.name == "show_list" => Some(item.span),
                _ => None,
            })
            .expect("original declaration span");
        for (index, change) in changes().into_iter().enumerate() {
            let mut module = original.clone();
            change(binding(&mut module));
            assert!(
                matches!(
                    lower([&module], &memory::Name::new("Instances", "show_list")),
                    Err(LowerError::Unsupported { form, span })
                        if span == declaration && (index < 2 || form == "listShow ownership adapter")
                ),
                "adapter mutation {index} must not be silently erased"
            );
        }
    }
    #[test]
    fn synthetic_expression_is_closed_pure_and_renders_head_before_recursing() {
        fn pure_tree(expression: &Expr) {
            match &expression.kind {
                ExprKind::App {
                    stage, arg_first, ..
                } => {
                    assert_eq!(*stage, Stage::Pure);
                    assert!(!arg_first);
                }
                ExprKind::Lambda { params, .. } => {
                    assert_eq!(params.len(), 1);
                    assert_eq!(params[0].stage, Stage::Pure);
                }
                ExprKind::Task(_)
                | ExprKind::Await(_)
                | ExprKind::Within(..)
                | ExprKind::Global { .. } => {
                    panic!("synthetic listShow must not add task, owner or host/global boundaries")
                }
                _ => {}
            }
            expression.each_child(&mut pure_tree);
        }
        let expression = expression(Span::new(FileId::new(0), 10, 20));
        assert!(
            free(&expression).is_empty(),
            "synthetic binder spellings cannot capture user locals"
        );
        pure_tree(&expression);
        let ExprKind::Lambda { params, body } = &expression.kind else {
            panic!("pure renderer stage")
        };
        let Pat::Bind(renderer) = &params[0].pat else {
            panic!("renderer binder")
        };
        let ExprKind::Block(statements, _) = &body.kind else {
            panic!("closed recursive binding")
        };
        let Stmt::Fun {
            name, rec, value, ..
        } = &statements[0]
        else {
            panic!("local loop")
        };
        assert!(rec);
        let ExprKind::Lambda { body, .. } = &value.kind else {
            panic!("unary loop")
        };
        let ExprKind::Match(_, arms) = &body.kind else {
            panic!("list cases")
        };
        assert!(matches!(&arms[0].pat, Pat::List(items, None) if items.is_empty()));
        assert!(matches!(&arms[0].body.kind, ExprKind::Lit(Lit::String(text)) if text.is_empty()));
        let ExprKind::Block(statements, _) = &arms[2].body.kind else {
            panic!("ordered renderer/recursion lets")
        };
        assert_eq!(statements.len(), 2);
        for (statement, function) in statements.iter().zip([renderer, name]) {
            let Stmt::Let { value, .. } = statement else {
                panic!("explicit let before string assembly")
            };
            let ExprKind::App { func, .. } = &value.kind else {
                panic!("one direct application")
            };
            assert!(matches!(&func.kind, ExprKind::Local(callee) if callee == function));
        }
    }
}
