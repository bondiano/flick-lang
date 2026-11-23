//! Edge-case tests for single-constructor, unboxed, untagged/literal enums,
//! niche chaining, fixed-width primitives, handles, tuples, aliases, custom
//! primitive tables, and error diagnostics.

use flick_ir::{EnumLit, IrType, JsClass, Rep, Stage, TypeDecl, TypeRef};

use super::super::{
    Discriminant, LayoutEngine, LayoutError, Niche, Primitive, PrimitiveLayout, PrimitiveTable,
    TypeRepr, compute_layout,
};
use super::{as_data, bare, ctor, data_decl, make_module, make_program, named, var};

#[test]
fn single_constructor_and_unboxed_types_have_no_discriminant() {
    let string = bare("Prelude", "String");
    let float = bare("Prelude", "Float");
    let app = make_module(
        "App",
        vec![
            data_decl(
                "UserId",
                &[],
                vec![ctor("UserId", &[], vec![string], Rep::Unboxed)],
            ),
            data_decl(
                "Point",
                &[],
                vec![ctor("Point", &[], vec![float.clone(), float], Rep::Tagged)],
            ),
            data_decl(
                "Marker",
                &[],
                vec![ctor("Marker", &[], Vec::new(), Rep::Tagged)],
            ),
        ],
    );
    let program = make_program(vec![app]);

    for (table, ptr_size) in [
        (PrimitiveTable::target_32(), 4u64),
        (PrimitiveTable::target_64(), 8u64),
    ] {
        let uid = compute_layout(&program, &table, &bare("App", "UserId")).expect("UserId");
        assert_eq!(uid.size, ptr_size);
        assert_eq!(uid.align, ptr_size);
        assert_eq!(uid.niche, Some(Niche::null_pointer(ptr_size)));
        let uid_data = as_data(&uid);
        assert!(uid_data.unboxed);
        assert_eq!(uid_data.discriminant, Discriminant::None);
        assert_eq!(uid_data.variants[0].tag, None);
        assert_eq!(uid_data.variants[0].fields[0].offset, 0);

        let opt_uid = compute_layout(
            &program,
            &table,
            &named("Prelude", "Option", vec![bare("App", "UserId")]),
        )
        .expect("Option UserId");
        assert_eq!(opt_uid.size, ptr_size);
        assert_eq!(opt_uid.align, ptr_size);
        assert!(matches!(
            as_data(&opt_uid).discriminant,
            Discriminant::Niche {
                offset: 0,
                width,
                data_variant: 1,
                niche_start: 0,
                niche_end: 0,
            } if width == ptr_size
        ));

        let pt = compute_layout(&program, &table, &bare("App", "Point")).expect("Point");
        assert_eq!(pt.size, 16);
        assert_eq!(pt.align, 8);
        let pt_data = as_data(&pt);
        assert!(!pt_data.unboxed);
        assert_eq!(pt_data.discriminant, Discriminant::None);
        assert_eq!(pt_data.variants[0].fields[0].offset, 0);
        assert_eq!(pt_data.variants[0].fields[1].offset, 8);

        let mk = compute_layout(&program, &table, &bare("App", "Marker")).expect("Marker");
        assert_eq!(mk.size, 0);
        assert_eq!(mk.align, 1);
        assert_eq!(as_data(&mk).discriminant, Discriminant::None);
    }
}

#[test]
fn untagged_types_and_literal_enums_receive_discriminants() {
    let app = make_module(
        "App",
        vec![
            data_decl(
                "Key",
                &[],
                vec![
                    ctor(
                        "ByFloat",
                        &[],
                        vec![bare("Prelude", "Float")],
                        Rep::Typeof(JsClass::Number),
                    ),
                    ctor(
                        "ByStr",
                        &[],
                        vec![bare("Prelude", "String")],
                        Rep::Typeof(JsClass::String),
                    ),
                ],
            ),
            data_decl(
                "Method",
                &[],
                vec![
                    ctor(
                        "Get",
                        &[],
                        Vec::new(),
                        Rep::Literal(EnumLit::String("GET".into())),
                    ),
                    ctor(
                        "Post",
                        &[],
                        Vec::new(),
                        Rep::Literal(EnumLit::String("POST".into())),
                    ),
                    ctor(
                        "Delete",
                        &[],
                        Vec::new(),
                        Rep::Literal(EnumLit::String("DELETE".into())),
                    ),
                ],
            ),
        ],
    );
    let program = make_program(vec![app]);

    for (table, str_off) in [
        (PrimitiveTable::target_32(), 4u64),
        (PrimitiveTable::target_64(), 8u64),
    ] {
        let key = compute_layout(&program, &table, &bare("App", "Key")).expect("Key");
        assert_eq!(key.size, 16);
        assert_eq!(key.align, 8);
        let key_data = as_data(&key);
        assert_eq!(
            key_data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        assert_eq!(key_data.variants[0].tag, Some(0));
        assert_eq!(key_data.variants[0].fields[0].offset, 8);
        assert_eq!(key_data.variants[1].tag, Some(1));
        assert_eq!(key_data.variants[1].fields[0].offset, str_off);

        let method = compute_layout(&program, &table, &bare("App", "Method")).expect("Method");
        assert_eq!(method.size, 1);
        assert_eq!(method.align, 1);
        let m_data = as_data(&method);
        assert_eq!(
            m_data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        assert_eq!(m_data.variants[0].tag, Some(0));
        assert_eq!(m_data.variants[1].tag, Some(1));
        assert_eq!(m_data.variants[2].tag, Some(2));
        assert!(m_data.variants.iter().all(|v| v.fields.is_empty()));
    }
}

#[test]
fn nested_niches_chain_until_exhausted() {
    let program = make_program(Vec::new());
    let opt = |inner: TypeRef| named("Prelude", "Option", vec![inner]);
    let table = PrimitiveTable::target_64();

    let ob = compute_layout(&program, &table, &opt(bare("Prelude", "Bool"))).expect("Option Bool");
    assert_eq!(ob.size, 1);
    assert_eq!(
        ob.niche,
        Some(Niche {
            offset: 0,
            width: 1,
            start: 3,
            end: 255,
        })
    );

    let oob = compute_layout(&program, &table, &opt(opt(bare("Prelude", "Bool"))))
        .expect("Option (Option Bool)");
    assert_eq!(oob.size, 1);
    assert_eq!(
        as_data(&oob).discriminant,
        Discriminant::Niche {
            offset: 0,
            width: 1,
            data_variant: 1,
            niche_start: 3,
            niche_end: 3,
        }
    );

    let oo = compute_layout(&program, &table, &opt(bare("Prelude", "Ordering")))
        .expect("Option Ordering");
    assert_eq!(oo.size, 1);
    assert_eq!(as_data(&oo).variants[0].tag, Some(3));

    let list_int = named("Prelude", "List", vec![bare("Prelude", "Int")]);
    let ool = compute_layout(&program, &table, &opt(opt(list_int))).expect("Option Option List");
    assert_eq!(ool.size, 16);
    assert_eq!(ool.align, 8);
    assert_eq!(
        as_data(&ool).discriminant,
        Discriminant::Tag {
            offset: 0,
            width: 1,
        }
    );
}

#[test]
fn fixed_width_primitives_handles_closures_tuples_and_opaque_aliases() {
    let app = make_module(
        "App",
        vec![TypeDecl {
            name: "Pair".into(),
            params: vec!["a".into()],
            error: false,
            constructors: Vec::new(),
            alias: Some(TypeRef::new(IrType::Tuple(vec![var("a"), var("a")]))),
        }],
    );
    let program = make_program(vec![app]);
    let t64 = PrimitiveTable::target_64();
    let mut engine = LayoutEngine::new(&program, t64).expect("engine");

    for (name, prim, size, align) in [
        ("Int8", Primitive::Int8, 1, 1),
        ("Int16", Primitive::Int16, 2, 2),
        ("Int64", Primitive::Int64, 8, 8),
        ("UInt8", Primitive::UInt8, 1, 1),
        ("UInt16", Primitive::UInt16, 2, 2),
        ("UInt32", Primitive::UInt32, 4, 4),
        ("UInt64", Primitive::UInt64, 8, 8),
        ("Float32", Primitive::Float32, 4, 4),
    ] {
        let l = engine.layout(&bare("Prelude", name)).expect(name);
        assert_eq!(l.size, size);
        assert_eq!(l.align, align);
        assert_eq!(l.repr, TypeRepr::Primitive(prim));
    }

    let bytes = engine.layout(&bare("Bytes", "Bytes")).expect("Bytes");
    assert_eq!(bytes.repr, TypeRepr::Primitive(Primitive::Bytes));
    assert_eq!(bytes.size, 8);

    let bigint = engine.layout(&bare("Prelude", "BigInt")).expect("BigInt");
    assert_eq!(bigint.repr, TypeRepr::Primitive(Primitive::BigInt));

    let fun_ty = TypeRef::new(IrType::Fun {
        param: bare("Prelude", "Int"),
        stage: Stage::Pure,
        result: bare("Prelude", "Bool"),
    });
    let closure = engine.layout(&fun_ty).expect("Closure");
    assert_eq!(closure.repr, TypeRepr::Primitive(Primitive::Closure));
    assert_eq!(closure.size, 16);
    assert_eq!(closure.align, 8);

    let tup = TypeRef::new(IrType::Tuple(vec![
        bare("Prelude", "Bool"),
        bare("Prelude", "Float"),
        bare("Prelude", "Bool"),
    ]));
    let tl = engine.layout(&tup).expect("Tuple");
    assert_eq!(tl.size, 16);
    assert_eq!(tl.align, 8);
    let TypeRepr::Tuple(t) = &tl.repr else {
        panic!("expected Tuple");
    };
    assert_eq!(t.fields[0].offset, 8);
    assert_eq!(t.fields[1].offset, 0);
    assert_eq!(t.fields[2].offset, 9);

    let pair_i16 = engine
        .layout(&named("App", "Pair", vec![bare("Prelude", "Int16")]))
        .expect("Pair Int16");
    assert_eq!(pair_i16.size, 4);
    assert_eq!(pair_i16.align, 2);
}

#[test]
fn custom_primitive_table_and_error_cases() {
    let program = make_program(Vec::new());

    let mut custom = PrimitiveTable::target_64();
    custom.pointer = PrimitiveLayout::new(8, 8, None);
    custom.string = PrimitiveLayout::new(16, 8, Some(Niche::null_pointer(8)));
    let opt_str = compute_layout(
        &program,
        &custom,
        &named("Prelude", "Option", vec![bare("Prelude", "String")]),
    )
    .expect("Option String fat handle");
    assert_eq!(opt_str.size, 16);
    assert_eq!(opt_str.align, 8);
    assert!(matches!(
        as_data(&opt_str).discriminant,
        Discriminant::Niche {
            offset: 0,
            width: 8,
            data_variant: 1,
            niche_start: 0,
            niche_end: 0,
        }
    ));

    let opt_list = compute_layout(
        &program,
        &custom,
        &named(
            "Prelude",
            "Option",
            vec![named("Prelude", "List", vec![bare("Prelude", "Int")])],
        ),
    )
    .expect("Option List without pointer niche");
    assert_eq!(opt_list.size, 16);
    assert_eq!(
        as_data(&opt_list).discriminant,
        Discriminant::Tag {
            offset: 0,
            width: 1,
        }
    );

    let mut bad_table = PrimitiveTable::target_64();
    bad_table.int.align = 3;
    assert!(matches!(
        LayoutEngine::new(&program, bad_table),
        Err(LayoutError::InvalidPrimitiveTable(_))
    ));

    let mut engine = LayoutEngine::new(&program, PrimitiveTable::target_64()).expect("engine");
    assert!(matches!(
        engine.layout(&var("a")),
        Err(LayoutError::PolymorphicType(_))
    ));
    let open_rec = TypeRef::new(IrType::Record {
        fields: vec![("x".into(), bare("Prelude", "Int"))],
        tail: Some("r".into()),
    });
    assert!(matches!(
        engine.layout(&open_rec),
        Err(LayoutError::OpenRecord(_))
    ));
    assert!(matches!(
        engine.layout(&bare("Missing", "Thing")),
        Err(LayoutError::UnknownType { .. })
    ));
    assert!(matches!(
        engine.layout(&named("Prelude", "Option", Vec::new())),
        Err(LayoutError::ArityMismatch { .. })
    ));
}
