//! Unit tests for static polymorphic recursion classification and
//! instantiation/output-size budget enforcement.

use flick_ir::{
    Arith, Arm, BinOp, Constructor, ConstructorDecl, Expr, ExprKind, IrType, Item, ItemKind, Lit,
    Num, Pat, Rep, TypeDecl, TypeRef,
};

use crate::layout::PrimitiveTable;

use super::super::{
    BudgetKind, DefMode, FallbackReason, SpecializeConfig, SpecializeError, specialize,
};
use super::eval::{FixtureEval, Val};
use super::{app, bare, base_program, empty_mod, fun, lam, named, span, var};

#[test]
#[allow(clippy::too_many_lines)]
fn polymorphic_recursion_is_statically_classified_and_never_hits_budget() {
    let int_t = bare("Prelude", "Int");
    let a = var("a");
    let list_a = named("Prelude", "List", vec![a.clone()]);
    let nested_a = named("App", "Nested", vec![a.clone()]);
    let nested_list_a = named("App", "Nested", vec![list_a.clone()]);
    let nested_int = named("App", "Nested", vec![int_t.clone()]);

    let flat_ctor = |ta: TypeRef| Constructor {
        module: "App".into(),
        ty: "Nested".into(),
        name: "Flat".into(),
        arity: 1,
        rep: Rep::Tagged,
        dicts: 0,
        fields: vec![ta.clone()],
        type_args: vec![ta],
    };
    let nest_ctor = |ta: TypeRef| {
        let la = named("Prelude", "List", vec![ta.clone()]);
        Constructor {
            module: "App".into(),
            ty: "Nested".into(),
            name: "Nest".into(),
            arity: 1,
            rep: Rep::Tagged,
            dicts: 0,
            fields: vec![named("App", "Nested", vec![la])],
            type_args: vec![ta],
        }
    };

    let depth_body = lam(
        "v",
        nested_a.clone(),
        Expr::with_ty(
            ExprKind::Match(
                Box::new(Expr::with_ty(
                    ExprKind::Local("v".into()),
                    span(),
                    Some(nested_a.clone()),
                )),
                vec![
                    Arm {
                        pat: Pat::Con(flat_ctor(a.clone()), vec![Pat::Wild]),
                        binder_types: Vec::new(),
                        guard: None,
                        body: Expr::with_ty(
                            ExprKind::Lit(Lit::Int(0)),
                            span(),
                            Some(int_t.clone()),
                        ),
                    },
                    Arm {
                        pat: Pat::Con(nest_ctor(a.clone()), vec![Pat::Bind("inner".into())]),
                        binder_types: vec![("inner".into(), nested_list_a.clone())],
                        guard: None,
                        body: Expr::with_ty(
                            ExprKind::Binary(
                                BinOp::Arith(Arith::Add, Num::Int),
                                Box::new(Expr::with_ty(
                                    ExprKind::Lit(Lit::Int(1)),
                                    span(),
                                    Some(int_t.clone()),
                                )),
                                Box::new(app(
                                    Expr::with_ty(
                                        ExprKind::Global {
                                            module: "App".into(),
                                            name: "depth".into(),
                                            type_args: vec![list_a.clone()],
                                        },
                                        span(),
                                        Some(fun(nested_list_a.clone(), int_t.clone())),
                                    ),
                                    Expr::with_ty(
                                        ExprKind::Local("inner".into()),
                                        span(),
                                        Some(nested_list_a.clone()),
                                    ),
                                    Some(int_t.clone()),
                                )),
                            ),
                            span(),
                            Some(int_t.clone()),
                        ),
                    },
                ],
            ),
            span(),
            Some(int_t.clone()),
        ),
    );

    let list_int = named("Prelude", "List", vec![int_t.clone()]);
    let flat_list = app(
        Expr::with_ty(
            ExprKind::Con(flat_ctor(list_int.clone())),
            span(),
            Some(fun(
                list_int.clone(),
                named("App", "Nested", vec![list_int.clone()]),
            )),
        ),
        Expr::with_ty(
            ExprKind::List(vec![Expr::with_ty(
                ExprKind::Lit(Lit::Int(42)),
                span(),
                Some(int_t.clone()),
            )]),
            span(),
            Some(list_int.clone()),
        ),
        Some(named("App", "Nested", vec![list_int])),
    );
    let nest_val = app(
        Expr::with_ty(
            ExprKind::Con(nest_ctor(int_t.clone())),
            span(),
            Some(fun(
                named(
                    "App",
                    "Nested",
                    vec![named("Prelude", "List", vec![int_t.clone()])],
                ),
                nested_int.clone(),
            )),
        ),
        flat_list,
        Some(nested_int.clone()),
    );
    let main_body = app(
        Expr::with_ty(
            ExprKind::Global {
                module: "App".into(),
                name: "depth".into(),
                type_args: vec![int_t.clone()],
            },
            span(),
            Some(fun(nested_int, int_t.clone())),
        ),
        nest_val,
        Some(int_t.clone()),
    );

    let app_mod = empty_mod(
        "App",
        vec![
            Item {
                kind: ItemKind::Value {
                    name: "depth".into(),
                    type_params: vec!["a".into()],
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: Some(fun(nested_a, int_t.clone())),
                    body: depth_body,
                },
                span: span(),
            },
            Item {
                kind: ItemKind::Value {
                    name: "main".into(),
                    type_params: Vec::new(),
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: Some(int_t),
                    body: main_body,
                },
                span: span(),
            },
        ],
        vec![TypeDecl {
            name: "Nested".into(),
            params: vec!["a".into()],
            error: false,
            constructors: vec![
                ConstructorDecl {
                    name: "Flat".into(),
                    rep: Rep::Tagged,
                    type_params: vec!["a".into()],
                    existentials: Vec::new(),
                    constraints: Vec::new(),
                    fields: vec![a],
                    record: None,
                    result_args: None,
                },
                ConstructorDecl {
                    name: "Nest".into(),
                    rep: Rep::Tagged,
                    type_params: vec!["a".into()],
                    existentials: Vec::new(),
                    constraints: Vec::new(),
                    fields: vec![nested_list_a],
                    record: None,
                    result_args: None,
                },
            ],
            alias: None,
        }],
    );

    let program = base_program(vec![app_mod]);
    let cfg = SpecializeConfig {
        max_instances: 3,
        ..SpecializeConfig::default()
    };
    let spec = specialize(
        &program,
        &[("App".into(), "main".into())],
        &PrimitiveTable::target_64(),
        &cfg,
    )
    .expect("polymorphic recursion must compile once over boxed cells");

    assert_eq!(spec.stats.fallback_definitions, 1);
    assert!(spec.definitions.values().any(|d| {
        d.source == ("App".into(), "depth".into())
            && d.mode == DefMode::Fallback(FallbackReason::PolymorphicRecursion)
    }));
    assert!(spec.stats.boxed_cells > 0);

    let mut ev = FixtureEval::new(&spec);
    assert!(matches!(ev.eval_root("App", "main"), Val::Int(1)));
}

#[test]
#[allow(clippy::too_many_lines)]
fn exponential_instantiation_triggers_budget_error_naming_definition() {
    let int_t = bare("Prelude", "Int");
    let a = var("a");
    let mut items = Vec::new();
    let depth = 6usize;

    items.push(Item {
        kind: ItemKind::Value {
            name: format!("step_{depth}"),
            type_params: vec!["a".into()],
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: Some(fun(a.clone(), int_t.clone())),
            body: lam(
                "x",
                a.clone(),
                Expr::with_ty(ExprKind::Lit(Lit::Int(1)), span(), Some(int_t.clone())),
            ),
        },
        span: span(),
    });

    for k in (0..depth).rev() {
        let pair_a = TypeRef::new(IrType::Tuple(vec![a.clone(), a.clone()]));
        let opt_a = named("Prelude", "Option", vec![a.clone()]);
        let next = format!("step_{}", k + 1);
        let c1 = app(
            Expr::with_ty(
                ExprKind::Global {
                    module: "App".into(),
                    name: next.clone(),
                    type_args: vec![pair_a.clone()],
                },
                span(),
                Some(fun(pair_a.clone(), int_t.clone())),
            ),
            Expr::with_ty(
                ExprKind::Tuple(vec![
                    Expr::with_ty(ExprKind::Local("x".into()), span(), Some(a.clone())),
                    Expr::with_ty(ExprKind::Local("x".into()), span(), Some(a.clone())),
                ]),
                span(),
                Some(pair_a),
            ),
            Some(int_t.clone()),
        );
        let some_con = Constructor {
            module: "Prelude".into(),
            ty: "Option".into(),
            name: "Some".into(),
            arity: 1,
            rep: Rep::Tagged,
            dicts: 0,
            fields: vec![a.clone()],
            type_args: vec![a.clone()],
        };
        let c2 = app(
            Expr::with_ty(
                ExprKind::Global {
                    module: "App".into(),
                    name: next,
                    type_args: vec![opt_a.clone()],
                },
                span(),
                Some(fun(opt_a.clone(), int_t.clone())),
            ),
            app(
                Expr::with_ty(
                    ExprKind::Con(some_con),
                    span(),
                    Some(fun(a.clone(), opt_a.clone())),
                ),
                Expr::with_ty(ExprKind::Local("x".into()), span(), Some(a.clone())),
                Some(opt_a),
            ),
            Some(int_t.clone()),
        );
        items.push(Item {
            kind: ItemKind::Value {
                name: format!("step_{k}"),
                type_params: vec!["a".into()],
                dicts: Vec::new(),
                dict_constraints: Vec::new(),
                ty: Some(fun(a.clone(), int_t.clone())),
                body: lam(
                    "x",
                    a.clone(),
                    Expr::with_ty(
                        ExprKind::Binary(
                            BinOp::Arith(Arith::Add, Num::Int),
                            Box::new(c1),
                            Box::new(c2),
                        ),
                        span(),
                        Some(int_t.clone()),
                    ),
                ),
            },
            span: span(),
        });
    }

    items.push(Item {
        kind: ItemKind::Value {
            name: "main".into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: Some(int_t.clone()),
            body: app(
                Expr::with_ty(
                    ExprKind::Global {
                        module: "App".into(),
                        name: "step_0".into(),
                        type_args: vec![int_t.clone()],
                    },
                    span(),
                    Some(fun(int_t.clone(), int_t.clone())),
                ),
                Expr::with_ty(ExprKind::Lit(Lit::Int(7)), span(), Some(int_t.clone())),
                Some(int_t),
            ),
        },
        span: span(),
    });

    let program = base_program(vec![empty_mod("App", items, Vec::new())]);
    let cfg = SpecializeConfig {
        max_instances: 16,
        ..SpecializeConfig::default()
    };
    let err = specialize(
        &program,
        &[("App".into(), "main".into())],
        &PrimitiveTable::target_64(),
        &cfg,
    )
    .expect_err("exponential instantiation must hit max_instances");

    match err {
        SpecializeError::BudgetExceeded {
            def,
            kind,
            limit,
            count,
        } => {
            assert_eq!(kind, BudgetKind::MaxInstances);
            assert_eq!(limit, 16);
            assert!(count > 16);
            assert_eq!(def.0, "App");
            assert!(def.1.starts_with("step_"), "got {def:?}");
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }

    // Output-size budget also names the offending definition.
    let out_cfg = SpecializeConfig {
        max_instances: 512,
        max_output_bytes: Some(256),
        ..SpecializeConfig::default()
    };
    let out_err = specialize(
        &program,
        &[("App".into(), "main".into())],
        &PrimitiveTable::target_64(),
        &out_cfg,
    )
    .expect_err("tiny max_output_bytes must trigger BudgetExceeded");
    assert!(matches!(
        out_err,
        SpecializeError::BudgetExceeded {
            kind: BudgetKind::MaxOutputBytes,
            ..
        }
    ));
}
