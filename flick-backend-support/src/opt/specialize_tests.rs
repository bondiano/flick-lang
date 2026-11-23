//! Regression tests for numeric specialization over mixed typed/erased calls.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{
    Constraint, Dict, Expr, ExprKind, FileId, Item, ItemKind, LangItem, Lit, Module, Namespace,
    Param, Pat, Program, Span, Stage, TypeInterner,
};

use crate::Known;

#[test]
#[allow(clippy::too_many_lines)]
fn specialize_handles_mixed_erased_and_typed_call_sites() {
    let at = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());
    let var_a = t.var("a");
    let var_b = t.var("b");
    let pair_ab = t.tuple(vec![var_a.clone(), var_b.clone()]);
    let pair_int_str = t.tuple(vec![int.clone(), str_ty.clone()]);
    let f_ret = t.fun(var_b.clone(), Stage::Pure, pair_ab.clone());
    let f_ty = t.fun(var_a.clone(), Stage::Pure, f_ret);
    let f_int_str_ret = t.fun(str_ty.clone(), Stage::Pure, pair_int_str.clone());
    let f_int_str = t.fun(int.clone(), Stage::Pure, f_int_str_ret.clone());
    let add_ret = t.fun(var_a.clone(), Stage::Pure, var_a.clone());
    let add_ty = t.fun(var_a.clone(), Stage::Pure, add_ret.clone());

    let lang = vec![
        LangItem {
            lang: "numeric".into(),
            module: "Numeric".into(),
            namespace: Namespace::Type,
            name: "Numeric".into(),
        },
        LangItem {
            lang: "int".into(),
            module: "Prelude".into(),
            namespace: Namespace::Type,
            name: "Int".into(),
        },
        LangItem {
            lang: "string".into(),
            module: "Prelude".into(),
            namespace: Namespace::Type,
            name: "String".into(),
        },
        LangItem {
            lang: "add".into(),
            module: "Numeric".into(),
            namespace: Namespace::Value,
            name: "add".into(),
        },
    ];
    let known = Known::of(&lang);

    let add_x_x = Expr::with_ty(
        ExprKind::App {
            func: Box::new(Expr::with_ty(
                ExprKind::App {
                    func: Box::new(Expr::with_ty(
                        ExprKind::Method {
                            dict: Dict::Given("d".into()),
                            name: "add".into(),
                            type_args: vec![var_a.clone()],
                        },
                        at,
                        Some(add_ty),
                    )),
                    arg: Box::new(Expr::with_ty(
                        ExprKind::Local("x".into()),
                        at,
                        Some(var_a.clone()),
                    )),
                    stage: Stage::Pure,
                    arg_first: false,
                },
                at,
                Some(add_ret),
            )),
            arg: Box::new(Expr::with_ty(
                ExprKind::Local("x".into()),
                at,
                Some(var_a.clone()),
            )),
            stage: Stage::Pure,
            arg_first: false,
        },
        at,
        Some(var_a.clone()),
    );
    let f_item = Item {
        span: at,
        kind: ItemKind::Value {
            name: "f".into(),
            type_params: vec!["a".into(), "b".into()],
            dicts: vec!["d".into()],
            dict_constraints: vec![Constraint {
                module: "Numeric".into(),
                class: "Numeric".into(),
                args: vec![var_a.clone()],
            }],
            ty: Some(f_ty.clone()),
            body: Expr::with_ty(
                ExprKind::Lambda {
                    params: vec![
                        Param {
                            pat: Pat::Bind("x".into()),
                            stage: Stage::Pure,
                            ty: Some(var_a.clone()),
                            binder_types: vec![("x".into(), var_a)],
                        },
                        Param {
                            pat: Pat::Bind("y".into()),
                            stage: Stage::Pure,
                            ty: Some(var_b.clone()),
                            binder_types: vec![("y".into(), var_b)],
                        },
                    ],
                    body: Box::new(Expr::with_ty(
                        ExprKind::Tuple(vec![
                            add_x_x,
                            Expr::with_ty(ExprKind::Local("y".into()), at, Some(t.var("b"))),
                        ]),
                        at,
                        Some(pair_ab),
                    )),
                },
                at,
                Some(f_ty),
            ),
        },
    };
    let int_dict = Dict::Instance {
        module: "Numeric".into(),
        name: "Numeric Int".into(),
        args: Vec::new(),
    };
    let make_caller = |name: &str, type_args: Vec<flick_ir::TypeRef>| {
        let head_ty = (!type_args.is_empty()).then(|| f_int_str.clone());
        Item {
            span: at,
            kind: ItemKind::Value {
                name: name.into(),
                type_params: Vec::new(),
                dicts: Vec::new(),
                dict_constraints: Vec::new(),
                ty: Some(pair_int_str.clone()),
                body: Expr::with_ty(
                    ExprKind::App {
                        func: Box::new(Expr::with_ty(
                            ExprKind::App {
                                func: Box::new(Expr::with_ty(
                                    ExprKind::Dicts(
                                        Box::new(Expr::with_ty(
                                            ExprKind::Global {
                                                module: "App".into(),
                                                name: "f".into(),
                                                type_args,
                                            },
                                            at,
                                            head_ty.clone(),
                                        )),
                                        vec![int_dict.clone()],
                                    ),
                                    at,
                                    head_ty,
                                )),
                                arg: Box::new(Expr::with_ty(
                                    ExprKind::Lit(Lit::Int(3)),
                                    at,
                                    Some(int.clone()),
                                )),
                                stage: Stage::Pure,
                                arg_first: false,
                            },
                            at,
                            Some(f_int_str_ret.clone()),
                        )),
                        arg: Box::new(Expr::with_ty(
                            ExprKind::Lit(Lit::String("hi".into())),
                            at,
                            Some(str_ty.clone()),
                        )),
                        stage: Stage::Pure,
                        arg_first: false,
                    },
                    at,
                    Some(pair_int_str.clone()),
                ),
            },
        }
    };

    for erased_first in [true, false] {
        let items = if erased_first {
            vec![
                f_item.clone(),
                make_caller("call_erased", Vec::new()),
                make_caller("call_typed", vec![int.clone(), str_ty.clone()]),
            ]
        } else {
            vec![
                f_item.clone(),
                make_caller("call_typed", vec![int.clone(), str_ty.clone()]),
                make_caller("call_erased", Vec::new()),
            ]
        };
        let mut module = Module {
            name: "App".into(),
            items,
            types: Vec::new(),
            labels: BTreeMap::new(),
            initialization: flick_ir::Initialization::default(),
            records: BTreeMap::new(),
            package: None,
            exports: flick_ir::Exports::default(),
        };
        super::specialize::specialize(&mut module, &|_, _| None, &known);

        let spec = module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Value {
                    name,
                    type_params,
                    dicts,
                    dict_constraints,
                    ty,
                    ..
                } if name == "f$Numeric_Int" => Some((type_params, dicts, dict_constraints, ty)),
                _ => None,
            })
            .expect("specialized f$Numeric_Int");
        assert_eq!(spec.0, &vec!["b".to_owned()], "erased_first={erased_first}");
        assert!(spec.1.is_empty());
        assert!(spec.2.is_empty());
        assert_eq!(
            spec.3.as_ref().map(ToString::to_string).as_deref(),
            Some("Prelude.Int -> b -> (Prelude.Int, b)"),
            "erased_first={erased_first}"
        );

        let call_args = |caller: &str| {
            let body = module.items.iter().find_map(|item| match &item.kind {
                ItemKind::Value { name, body, .. } if name == caller => Some(body),
                _ => None,
            });
            let (head, _) = body.expect("caller").spine();
            match &head.kind {
                ExprKind::Global {
                    name, type_args, ..
                } => {
                    assert_eq!(name, "f$Numeric_Int");
                    type_args
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                }
                other => panic!("expected specialized global, got {other:?}"),
            }
        };
        assert_eq!(call_args("call_erased"), Vec::<String>::new());
        assert_eq!(call_args("call_typed"), vec!["Prelude.String".to_owned()]);

        let program = Program {
            ir_version: flick_ir::VERSION.into(),
            compiler: "test".into(),
            files: Vec::new(),
            modules: vec![module],
            lang: lang.clone(),
        };
        let files = BTreeSet::from([FileId::new(0)]);
        let errs = flick_ir::validate(&program, &|m| m == "Prelude" || m == "Numeric", &files);
        assert!(
            errs.is_empty(),
            "erased_first={erased_first}: validation errors: {errs:?}"
        );
    }
}
