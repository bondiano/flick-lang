//! Unit tests for `OptRepr::of` layout selection and substitution invariance.

use std::collections::BTreeMap;

use flick_backend_support::Known;
use flick_ir::{
    Constraint, ConstructorDecl, IrType, Lang, LangItem, Namespace, Rep, TypeDecl, TypeRef,
};

use super::adt::{AdtShape, AdtSlot};
use super::arith::Native;
use super::go_type::{GoType, OptPayload, OptRepr, TypeDeclInfo, TypeDecls};
use super::record::{RecordShape, RecordSlot};

fn own_decl(decl: &TypeDecl) -> TypeDeclInfo<'_> {
    TypeDeclInfo {
        decl,
        own: true,
        package: None,
    }
}

fn lang_type(role: Lang, name: &str) -> LangItem {
    LangItem {
        lang: role.name().into(),
        module: "Prelude".into(),
        namespace: Namespace::Type,
        name: name.into(),
    }
}

fn opt_of(payload: IrType) -> IrType {
    IrType::Named {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![TypeRef::new(payload)],
    }
}

#[test]
fn unconstrained_forall_errors_service_impl_and_opaque_alias_stay_erased() {
    let items = [
        lang_type(Lang::Option, "Option"),
        lang_type(Lang::Unknown, "Unknown"),
        lang_type(Lang::Int, "Int"),
        lang_type(Lang::Errors, "Errors"),
        lang_type(Lang::ServiceImpl, "ServiceImpl"),
    ];
    let lang = Known::of(&items);
    let alias_decl = TypeDecl {
        name: "UserId".into(),
        params: vec![],
        error: false,
        alias: Some(TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Int".into(),
            args: vec![],
        })),
        constructors: vec![],
    };
    let mut decls = TypeDecls::new();
    decls.insert(("App".into(), "UserId".into()), own_decl(&alias_decl));

    // Unconstrained `Forall` (e.g. `forall a. Unknown` or `forall a. Int`) has
    // no `*flickrt.Poly` wrapper and can agree with its instantiated body.
    for body in [
        IrType::Named {
            module: "Prelude".into(),
            name: "Unknown".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "Int".into(),
            args: vec![],
        },
    ] {
        let unconstrained = IrType::Forall {
            vars: vec!["a".into()],
            constraints: vec![],
            body: TypeRef::new(body),
        };
        assert_eq!(
            OptRepr::of(&lang, &decls, Some(&opt_of(unconstrained))),
            None
        );
    }

    // Constrained `Forall` is wrapped in `*flickrt.Poly` at runtime (non-nil).
    let constrained = IrType::Forall {
        vars: vec!["a".into()],
        constraints: vec![Constraint {
            module: "Prelude".into(),
            class: "Show".into(),
            args: vec![TypeRef::new(IrType::Var("a".into()))],
        }],
        body: TypeRef::new(IrType::Var("a".into())),
    };
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(constrained))),
        Some(OptRepr::RefSentinel)
    );

    // Opaque type aliases, `Errors`, and `ServiceImpl` unify with other types
    // and therefore stay erased (`None`).
    for erased in [
        IrType::Named {
            module: "App".into(),
            name: "UserId".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "Errors".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "ServiceImpl".into(),
            args: vec![],
        },
    ] {
        assert_eq!(OptRepr::of(&lang, &decls, Some(&opt_of(erased))), None);
    }
}

#[test]
fn record_field_option_matches_record_ref_sentinel() {
    let items = [
        lang_type(Lang::Option, "Option"),
        lang_type(Lang::Int, "Int"),
        LangItem {
            lang: Lang::Field.name().into(),
            module: "Record".into(),
            namespace: Namespace::Type,
            name: "Field".into(),
        },
    ];
    let lang = Known::of(&items);
    let decls = TypeDecls::new();

    let int_ty = IrType::Named {
        module: "Prelude".into(),
        name: "Int".into(),
        args: vec![],
    };
    let rec_ty = IrType::Record {
        fields: vec![("x".into(), TypeRef::new(int_ty.clone()))],
        tail: None,
    };
    let field_int = IrType::Named {
        module: "Record".into(),
        name: "Field".into(),
        args: vec![TypeRef::new(int_ty)],
    };
    let field_poly = IrType::Named {
        module: "Record".into(),
        name: "Field".into(),
        args: vec![TypeRef::new(IrType::Var("a".into()))],
    };

    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(rec_ty.clone()))),
        Some(OptRepr::RefSentinel)
    );
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(field_int.clone()))),
        Some(OptRepr::RefSentinel)
    );
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(field_poly.clone()))),
        Some(OptRepr::RefSentinel)
    );
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(opt_of(field_poly)))),
        Some(OptRepr::Flag(OptPayload::Nested(Box::new(
            OptRepr::RefSentinel
        ))))
    );
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(opt_of(field_int)))),
        OptRepr::of(&lang, &decls, Some(&opt_of(opt_of(rec_ty))))
    );
}

#[test]
fn record_shape_canonical_order_and_substitution_invariance() {
    let items = [
        lang_type(Lang::Int, "Int"),
        lang_type(Lang::Float, "Float"),
        lang_type(Lang::Bool, "Bool"),
        lang_type(Lang::String, "String"),
        lang_type(Lang::List, "List"),
    ];
    let lang = Known::of(&items);
    let alias_decl = TypeDecl {
        name: "UserId".into(),
        params: vec![],
        error: false,
        alias: Some(TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Int".into(),
            args: vec![],
        })),
        constructors: vec![],
    };
    let mut decls = TypeDecls::new();
    decls.insert(("App".into(), "UserId".into()), own_decl(&alias_decl));

    let int_ty = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Int".into(),
        args: vec![],
    });
    let float_ty = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Float".into(),
        args: vec![],
    });
    let list_poly = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "List".into(),
        args: vec![TypeRef::new(IrType::Var("a".into()))],
    });

    // Empty closed record gets R_empty.
    let empty = IrType::Record {
        fields: vec![],
        tail: None,
    };
    let empty_shape = RecordShape::of(&lang, &decls, Some(&empty)).expect("empty closed record");
    assert_eq!(empty_shape.go_type(), "R_empty");
    assert!(empty_shape.is_go_comparable());

    // Canonical label order: array indices numerically ascending, then UTF-16 order.
    let canonical = IrType::Record {
        fields: vec![
            ("b".into(), int_ty.clone()),
            ("10".into(), float_ty.clone()),
            ("2".into(), list_poly),
            ("a".into(), int_ty.clone()),
        ],
        tail: None,
    };
    let shape = RecordShape::of(&lang, &decls, Some(&canonical)).expect("closed record shape");
    assert_eq!(
        shape.fields,
        vec![
            ("2".into(), RecordSlot::Value),
            ("10".into(), RecordSlot::Scalar(Native::Float)),
            ("a".into(), RecordSlot::Scalar(Native::Int)),
            ("b".into(), RecordSlot::Scalar(Native::Int)),
        ]
    );
    assert!(!shape.is_go_comparable());

    // Open-row records stay erased.
    let open = IrType::Record {
        fields: vec![("x".into(), int_ty.clone())],
        tail: Some("r".into()),
    };
    assert_eq!(RecordShape::of(&lang, &decls, Some(&open)), None);

    // Records with polymorphic, higher-rank, or opaque-alias fields stay erased.
    for field_ty in [
        IrType::Var("a".into()),
        IrType::Named {
            module: "App".into(),
            name: "UserId".into(),
            args: vec![],
        },
        IrType::Forall {
            vars: vec!["a".into()],
            constraints: vec![],
            body: int_ty.clone(),
        },
    ] {
        let rec = IrType::Record {
            fields: vec![("x".into(), TypeRef::new(field_ty))],
            tail: None,
        };
        assert_eq!(RecordShape::of(&lang, &decls, Some(&rec)), None);
        assert_eq!(GoType::of(&lang, &decls, Some(&rec)), None);
    }

    // All non-float scalar closed record is directly Go-comparable.
    let int_pair = IrType::Record {
        fields: vec![("x".into(), int_ty.clone()), ("y".into(), int_ty)],
        tail: None,
    };
    let pair_shape = RecordShape::of(&lang, &decls, Some(&int_pair)).expect("int pair shape");
    assert!(pair_shape.is_go_comparable());
}

fn named_ir(module: &str, name: &str, args: Vec<IrType>) -> IrType {
    IrType::Named {
        module: module.into(),
        name: name.into(),
        args: args.into_iter().map(TypeRef::new).collect(),
    }
}

fn tagged_ctor(name: &str, fields: Vec<IrType>) -> ConstructorDecl {
    ConstructorDecl {
        name: name.into(),
        fields: fields.into_iter().map(TypeRef::new).collect(),
        rep: Rep::Tagged,
        record: None,
        type_params: vec![],
        existentials: vec![],
        constraints: vec![],
        result_args: None,
    }
}

fn adt_lang_items() -> Vec<LangItem> {
    vec![
        lang_type(Lang::Int, "Int"),
        lang_type(Lang::Float, "Float"),
        lang_type(Lang::Bool, "Bool"),
        lang_type(Lang::String, "String"),
        lang_type(Lang::Option, "Option"),
        lang_type(Lang::Result, "Result"),
        LangItem {
            lang: Lang::Err.name().into(),
            module: "Prelude".into(),
            namespace: Namespace::Value,
            name: "Err".into(),
        },
        LangItem {
            lang: Lang::Ok.name().into(),
            module: "Prelude".into(),
            namespace: Namespace::Value,
            name: "Ok".into(),
        },
    ]
}

#[test]
fn adt_shape_slot_pooling_and_ineligible_shapes() {
    let items = adt_lang_items();
    let lang = Known::of(&items);
    let int_ty = named_ir("Prelude", "Int", vec![]);
    let float_ty = named_ir("Prelude", "Float", vec![]);
    let str_ty = named_ir("Prelude", "String", vec![]);

    let shape_decl = TypeDecl {
        name: "Shape".into(),
        params: vec![],
        error: false,
        alias: None,
        constructors: vec![
            tagged_ctor("Dot", vec![]),
            tagged_ctor("Circle", vec![float_ty.clone()]),
            tagged_ctor("Rect", vec![float_ty.clone(), float_ty]),
        ],
    };
    let chain_decl = TypeDecl {
        name: "Chain".into(),
        params: vec![],
        error: false,
        alias: None,
        constructors: vec![
            tagged_ctor("End", vec![]),
            tagged_ctor(
                "Link",
                vec![int_ty.clone(), named_ir("App", "Chain", vec![])],
            ),
        ],
    };
    let huge_decl = TypeDecl {
        name: "Huge".into(),
        params: vec![],
        error: false,
        alias: None,
        constructors: vec![
            tagged_ctor("Empty", vec![]),
            tagged_ctor(
                "Big",
                vec![
                    str_ty.clone(),
                    str_ty.clone(),
                    str_ty.clone(),
                    str_ty,
                    int_ty,
                ],
            ),
        ],
    };

    let mut decls = TypeDecls::new();
    decls.insert(("App".into(), "Shape".into()), own_decl(&shape_decl));
    decls.insert(("App".into(), "Chain".into()), own_decl(&chain_decl));
    decls.insert(("App".into(), "Huge".into()), own_decl(&huge_decl));

    let shape = AdtShape::of(&lang, &decls, Some(&named_ir("App", "Shape", vec![])))
        .expect("Shape eligible");
    assert_eq!(shape.go_type(), "A_App__Shape");
    assert_eq!(
        shape.slots,
        vec![
            AdtSlot::Scalar(Native::Float),
            AdtSlot::Scalar(Native::Float)
        ]
    );
    assert!(!shape.is_go_comparable());
    assert_eq!(
        AdtShape::of(&lang, &decls, Some(&named_ir("App", "Chain", vec![]))),
        None
    );
    assert_eq!(
        AdtShape::of(&lang, &decls, Some(&named_ir("App", "Huge", vec![]))),
        None
    );
}

#[test]
fn concrete_result_and_record_substitution_invariance() {
    let items = adt_lang_items();
    let lang = Known::of(&items);
    let int_ty = named_ir("Prelude", "Int", vec![]);
    let str_ty = named_ir("Prelude", "String", vec![]);
    let result_decl = TypeDecl {
        name: "Result".into(),
        params: vec!["e".into(), "a".into()],
        error: false,
        alias: None,
        constructors: vec![
            tagged_ctor("Err", vec![IrType::Var("e".into())]),
            tagged_ctor("Ok", vec![IrType::Var("a".into())]),
        ],
    };
    let mut decls = TypeDecls::new();
    decls.insert(
        ("Prelude".into(), "Result".into()),
        TypeDeclInfo {
            decl: &result_decl,
            own: false,
            package: Some("core"),
        },
    );

    let concrete_result = named_ir("Prelude", "Result", vec![str_ty, int_ty.clone()]);
    let adt = AdtShape::of(&lang, &decls, Some(&concrete_result)).expect("Result eligible");
    assert!(adt.is_result);
    assert!(adt.go_type().starts_with("AI_Prelude__Result__"));
    assert_eq!(
        adt.slots,
        vec![
            AdtSlot::Scalar(Native::String),
            AdtSlot::Scalar(Native::Int)
        ]
    );
    assert!(adt.is_go_comparable());

    let poly_result = named_ir("Prelude", "Result", vec![IrType::Var("e".into()), int_ty]);
    assert_eq!(AdtShape::of(&lang, &decls, Some(&poly_result)), None);
    let poly_record = IrType::Record {
        fields: vec![("r".into(), TypeRef::new(poly_result))],
        tail: None,
    };
    assert_eq!(RecordShape::of(&lang, &decls, Some(&poly_record)), None);
    let concrete_record = IrType::Record {
        fields: vec![("r".into(), TypeRef::new(concrete_result))],
        tail: None,
    };
    let record_shape =
        RecordShape::of(&lang, &decls, Some(&concrete_record)).expect("record with Result");
    assert_eq!(
        record_shape.fields,
        vec![("r".into(), RecordSlot::Adt(adt))]
    );
}

#[test]
fn adt_show_strips_flattened_submodule_prefix() {
    let items = [lang_type(Lang::Int, "Int")];
    let lang = Known::of(&items);
    let int_ty = named_ir("Prelude", "Int", vec![]);
    let pair_decl = TypeDecl {
        name: "Points$Pair".into(),
        params: vec![],
        error: false,
        alias: None,
        constructors: vec![tagged_ctor("Points$Pair", vec![int_ty.clone(), int_ty])],
    };
    let mut decls = TypeDecls::new();
    decls.insert(("App".into(), "Points$Pair".into()), own_decl(&pair_decl));
    let shape = AdtShape::of(&lang, &decls, Some(&named_ir("App", "Points$Pair", vec![])))
        .expect("Points$Pair eligible");
    let mut shapes = BTreeMap::new();
    shapes.insert(shape, super::adt::AdtOps::SHOW);
    let emitted = super::adt_emit::file(
        &shapes,
        &super::own::Records::new(),
        &super::own::CtorFields::new(),
    );
    assert!(
        emitted.contains("func Show_A_App__Points_x24_Pair("),
        "{emitted}"
    );
    assert!(emitted.contains("return \"Pair\" + \" \" +"), "{emitted}");
    assert!(!emitted.contains("return \"Points$Pair\""), "{emitted}");
}
