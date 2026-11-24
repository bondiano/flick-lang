//! Unit tests for `OptRepr::of` layout selection and substitution invariance.

use flick_backend_support::Known;
use flick_ir::{Constraint, IrType, Lang, LangItem, Namespace, TypeDecl, TypeRef};

use super::arith::Native;
use super::go_type::{GoType, OptPayload, OptRepr, TypeDecls};
use super::record::RecordShape;

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
    decls.insert(("App".into(), "UserId".into()), &alias_decl);

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
    decls.insert(("App".into(), "UserId".into()), &alias_decl);

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
            ("2".into(), None),
            ("10".into(), Some(Native::Float)),
            ("a".into(), Some(Native::Int)),
            ("b".into(), Some(Native::Int)),
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
