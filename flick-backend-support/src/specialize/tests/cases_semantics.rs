//! Unit tests for determinism, polymorphic foreign boundaries, and dictionary
//! devirtualization vs. distinct local dictionaries.

use flick_ir::{
    Adapter, BinOp, Binding, Dict, Expr, ExprKind, Foreign, ForeignMode, InstanceInfo, Item,
    ItemKind, Lit,
};

use crate::layout::PrimitiveTable;

use super::super::{SpecializeConfig, specialize};
use super::eval::{FixtureEval, Val};
use super::{app, bare, base_program, empty_mod, fun, lam, span, var};

#[test]
fn specialization_is_deterministic_across_runs_and_module_order() {
    let int_t = bare("Prelude", "Int");
    let a = var("a");
    let mod_a = empty_mod(
        "ModA",
        vec![Item {
            kind: ItemKind::Value {
                name: "id".into(),
                type_params: vec!["a".into()],
                dicts: Vec::new(),
                dict_constraints: Vec::new(),
                ty: Some(fun(a.clone(), a.clone())),
                body: lam(
                    "x",
                    a.clone(),
                    Expr::with_ty(ExprKind::Local("x".into()), span(), Some(a)),
                ),
            },
            span: span(),
        }],
        Vec::new(),
    );
    let mod_b = empty_mod(
        "ModB",
        vec![Item {
            kind: ItemKind::Value {
                name: "main".into(),
                type_params: Vec::new(),
                dicts: Vec::new(),
                dict_constraints: Vec::new(),
                ty: Some(int_t.clone()),
                body: app(
                    Expr::with_ty(
                        ExprKind::Global {
                            module: "ModA".into(),
                            name: "id".into(),
                            type_args: vec![int_t.clone()],
                        },
                        span(),
                        Some(fun(int_t.clone(), int_t.clone())),
                    ),
                    Expr::with_ty(ExprKind::Lit(Lit::Int(99)), span(), Some(int_t.clone())),
                    Some(int_t),
                ),
            },
            span: span(),
        }],
        Vec::new(),
    );

    let p1 = base_program(vec![mod_a.clone(), mod_b.clone()]);
    let p2 = base_program(vec![mod_b, mod_a]);
    let table = PrimitiveTable::target_64();
    let cfg = SpecializeConfig::default();

    let r1 =
        specialize(&p1, &[("ModB".into(), "main".into())], &table, &cfg).expect("run 1 succeeds");
    let r2 =
        specialize(&p1, &[("ModB".into(), "main".into())], &table, &cfg).expect("run 2 succeeds");
    let r3 =
        specialize(&p2, &[("ModB".into(), "main".into())], &table, &cfg).expect("run 3 succeeds");

    assert_eq!(r1, r2);
    assert_eq!(r1, r3);
    assert_eq!(r1.stats.boxed_cells, 0);
}

#[test]
#[allow(clippy::too_many_lines)]
fn polymorphic_foreign_boxes_at_boundary_while_monomorphic_foreign_does_not() {
    let int_t = bare("Prelude", "Int");
    let a = var("a");
    let poly_f = Foreign {
        name: "poly_id".into(),
        source_name: None,
        mode: ForeignMode::Pure,
        binding: Binding::Global {
            name: "poly_id".into(),
        },
        stages: vec![false],
        dicts: 0,
        arity: 1,
        adapter: Adapter::default(),
        catch: None,
        type_params: vec!["a".into()],
        ty: Some(fun(a.clone(), a)),
    };
    let mono_f = Foreign {
        name: "inc_int".into(),
        source_name: None,
        mode: ForeignMode::Pure,
        binding: Binding::Global {
            name: "inc_int".into(),
        },
        stages: vec![false],
        dicts: 0,
        arity: 1,
        adapter: Adapter::default(),
        catch: None,
        type_params: Vec::new(),
        ty: Some(fun(int_t.clone(), int_t.clone())),
    };

    let call_mono = app(
        Expr::with_ty(
            ExprKind::Global {
                module: "App".into(),
                name: "inc_int".into(),
                type_args: Vec::new(),
            },
            span(),
            Some(fun(int_t.clone(), int_t.clone())),
        ),
        Expr::with_ty(ExprKind::Lit(Lit::Int(10)), span(), Some(int_t.clone())),
        Some(int_t.clone()),
    );
    let call_poly = app(
        Expr::with_ty(
            ExprKind::Global {
                module: "App".into(),
                name: "poly_id".into(),
                type_args: vec![int_t.clone()],
            },
            span(),
            Some(fun(int_t.clone(), int_t.clone())),
        ),
        call_mono,
        Some(int_t.clone()),
    );

    let program = base_program(vec![empty_mod(
        "App",
        vec![
            Item {
                kind: ItemKind::Foreign(poly_f),
                span: span(),
            },
            Item {
                kind: ItemKind::Foreign(mono_f),
                span: span(),
            },
            Item {
                kind: ItemKind::Value {
                    name: "main".into(),
                    type_params: Vec::new(),
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: Some(int_t),
                    body: call_poly,
                },
                span: span(),
            },
        ],
        Vec::new(),
    )]);

    let spec = specialize(
        &program,
        &[("App".into(), "main".into())],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("foreign specialization succeeds");

    assert_eq!(spec.stats.polymorphic_foreigns, 1);
    assert_eq!(spec.stats.box_count, 1);
    assert_eq!(spec.stats.unbox_count, 1);

    let mut ev = FixtureEval::with_foreign(&spec, |_, name, args| match name {
        "inc_int" => match args[0] {
            Val::Int(n) => Val::Int(n + 1),
            ref other => panic!("inc_int expected unboxed Int, got {other:?}"),
        },
        "poly_id" => {
            assert!(matches!(args[0], Val::Boxed(_)));
            args[0].clone()
        }
        other => panic!("unexpected foreign {other}"),
    });
    assert!(matches!(ev.eval_root("App", "main"), Val::Int(11)));
    assert_eq!(ev.stats.runtime_boxes, 1);
    assert_eq!(ev.stats.runtime_unboxes, 1);
}

#[test]
#[allow(clippy::too_many_lines)]
fn static_dict_is_devirtualized_while_two_local_dicts_stay_distinct() {
    let int_t = bare("Prelude", "Int");
    let str_t = bare("Prelude", "String");
    let a = var("a");

    let tag_inst = Item {
        kind: ItemKind::Instance {
            info: Box::new(InstanceInfo {
                module: "App".into(),
                name: "$i_Tag_Int".into(),
                class: ("App".into(), "Tag".into()),
                span: (0, 0),
                context: Vec::new(),
                supers: Vec::new(),
                derived: None,
                defaults: Vec::new(),
                generic: None,
                enumerates: None,
            }),
            methods: vec![(
                "tag".into(),
                lam(
                    "_n",
                    int_t.clone(),
                    Expr::with_ty(
                        ExprKind::Lit(Lit::String("G".into())),
                        span(),
                        Some(str_t.clone()),
                    ),
                ),
            )],
        },
        span: span(),
    };

    let wrap_item = Item {
        kind: ItemKind::Value {
            name: "wrap".into(),
            type_params: vec!["a".into()],
            dicts: vec!["dTag".into()],
            dict_constraints: Vec::new(),
            ty: Some(fun(a.clone(), str_t.clone())),
            body: lam(
                "x",
                a.clone(),
                app(
                    Expr::with_ty(
                        ExprKind::Method {
                            dict: Dict::Given("dTag".into()),
                            name: "tag".into(),
                            type_args: Vec::new(),
                        },
                        span(),
                        Some(fun(a.clone(), str_t.clone())),
                    ),
                    Expr::with_ty(ExprKind::Local("x".into()), span(), Some(a)),
                    Some(str_t.clone()),
                ),
            ),
        },
        span: span(),
    };

    let make_local_dict = |name: &str, label: &str| Item {
        kind: ItemKind::Value {
            name: name.into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body: Expr::new(
                ExprKind::Dictionary(vec![(
                    "tag".into(),
                    lam(
                        "_n",
                        int_t.clone(),
                        Expr::with_ty(
                            ExprKind::Lit(Lit::String(label.into())),
                            span(),
                            Some(str_t.clone()),
                        ),
                    ),
                )]),
                span(),
            ),
        },
        span: span(),
    };

    let call_wrap = |dict: Dict| {
        app(
            Expr::with_ty(
                ExprKind::Dicts(
                    Box::new(Expr::with_ty(
                        ExprKind::Global {
                            module: "App".into(),
                            name: "wrap".into(),
                            type_args: vec![int_t.clone()],
                        },
                        span(),
                        Some(fun(int_t.clone(), str_t.clone())),
                    )),
                    vec![dict],
                ),
                span(),
                Some(fun(int_t.clone(), str_t.clone())),
            ),
            Expr::with_ty(ExprKind::Lit(Lit::Int(5)), span(), Some(int_t.clone())),
            Some(str_t.clone()),
        )
    };

    let concat = |l: Expr, r: Expr| {
        Expr::with_ty(
            ExprKind::Binary(BinOp::Concat, Box::new(l), Box::new(r)),
            span(),
            Some(str_t.clone()),
        )
    };
    let main_item = Item {
        kind: ItemKind::Value {
            name: "main".into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: Some(str_t.clone()),
            body: concat(
                concat(
                    call_wrap(Dict::Instance {
                        module: "App".into(),
                        name: "$i_Tag_Int".into(),
                        args: Vec::new(),
                    }),
                    call_wrap(Dict::Value {
                        module: "App".into(),
                        name: "d1".into(),
                    }),
                ),
                call_wrap(Dict::Value {
                    module: "App".into(),
                    name: "d2".into(),
                }),
            ),
        },
        span: span(),
    };

    let program = base_program(vec![empty_mod(
        "App",
        vec![
            tag_inst,
            wrap_item,
            make_local_dict("d1", "L1"),
            make_local_dict("d2", "L2"),
            main_item,
        ],
        Vec::new(),
    )]);

    let spec = specialize(
        &program,
        &[("App".into(), "main".into())],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("foreign specialization succeeds");

    assert!(spec.stats.direct_dict_calls >= 1);
    assert!(spec.stats.runtime_dict_calls >= 1);

    let mut ev = FixtureEval::new(&spec);
    let res = ev.eval_root("App", "main");
    assert!(
        matches!(res, Val::String(ref s) if s == "GL1L2"),
        "got {res:?}"
    );
}
