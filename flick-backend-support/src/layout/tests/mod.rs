//! Unit and property tests for shared data layouts (#742).

mod codec;
mod edge_cases;
mod props;
mod recursion;

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{
    ConstructorDecl, Exports, Initialization, IrType, LangItem, Module, Namespace, Program, Rep,
    TypeDecl, TypeInterner, TypeRef,
};

use super::{
    DataLayout, Discriminant, FieldStorage, LayoutEngine, Niche, PrimitiveTable, TypeLayout,
    TypeRepr, compute_layout,
};

pub(super) fn named(module: &str, name: &str, args: Vec<TypeRef>) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: module.to_owned(),
        name: name.to_owned(),
        args,
    })
}

pub(super) fn bare(module: &str, name: &str) -> TypeRef {
    named(module, name, Vec::new())
}

pub(super) fn var(name: &str) -> TypeRef {
    TypeRef::new(IrType::Var(name.to_owned()))
}

pub(super) fn ctor(name: &str, params: &[&str], fields: Vec<TypeRef>, rep: Rep) -> ConstructorDecl {
    ConstructorDecl {
        name: name.to_owned(),
        rep,
        type_params: params.iter().map(|s| (*s).to_owned()).collect(),
        existentials: Vec::new(),
        constraints: Vec::new(),
        fields,
        record: None,
        result_args: None,
    }
}

pub(super) fn data_decl(
    name: &str,
    params: &[&str],
    constructors: Vec<ConstructorDecl>,
) -> TypeDecl {
    TypeDecl {
        name: name.to_owned(),
        params: params.iter().map(|s| (*s).to_owned()).collect(),
        error: false,
        constructors,
        alias: None,
        foreign_forms: Vec::new(),
    }
}

pub(super) fn make_module(name: &str, types: Vec<TypeDecl>) -> Module {
    Module {
        name: name.to_owned(),
        items: Vec::new(),
        types,
        preambles: Vec::new(),
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    }
}

pub(super) fn standard_prelude_module() -> Module {
    let a = var("a");
    make_module(
        "Prelude",
        vec![
            data_decl("Unit", &[], Vec::new()),
            data_decl("Never", &[], Vec::new()),
            data_decl("Bool", &[], Vec::new()),
            data_decl("Int", &[], Vec::new()),
            data_decl("Float", &[], Vec::new()),
            data_decl("BigInt", &[], Vec::new()),
            data_decl("String", &[], Vec::new()),
            data_decl("List", &["a"], Vec::new()),
            data_decl(
                "Option",
                &["a"],
                vec![
                    ctor("None", &["a"], Vec::new(), Rep::Tagged),
                    ctor("Some", &["a"], vec![a], Rep::Tagged),
                ],
            ),
            data_decl(
                "Ordering",
                &[],
                vec![
                    ctor("LT", &[], Vec::new(), Rep::Tagged),
                    ctor("EQ", &[], Vec::new(), Rep::Tagged),
                    ctor("GT", &[], Vec::new(), Rep::Tagged),
                ],
            ),
        ],
    )
}

pub(super) fn standard_lang() -> Vec<LangItem> {
    let ty = |role: &str, name: &str| LangItem {
        lang: role.to_owned(),
        module: "Prelude".to_owned(),
        namespace: Namespace::Type,
        name: name.to_owned(),
    };
    vec![
        ty("unit", "Unit"),
        ty("never", "Never"),
        ty("bool", "Bool"),
        ty("int", "Int"),
        ty("float", "Float"),
        ty("bigint", "BigInt"),
        ty("string", "String"),
        ty("list", "List"),
        ty("option", "Option"),
    ]
}

pub(super) fn make_program(mut modules: Vec<Module>) -> Program {
    if !modules.iter().any(|m| m.name == "Prelude") {
        modules.insert(0, standard_prelude_module());
    }
    Program {
        ir_version: flick_ir::VERSION.to_owned(),
        compiler: String::new(),
        files: Vec::new(),
        modules,
        lang: standard_lang(),
    }
}

pub(super) fn as_data(layout: &TypeLayout) -> &DataLayout {
    match &layout.repr {
        TypeRepr::Data(d) => d,
        other => panic!("expected Data layout, got {other:?}"),
    }
}

#[test]
fn shape_layout_on_32_and_64_bit_tables() {
    let float = bare("Prelude", "Float");
    let app = make_module(
        "App",
        vec![data_decl(
            "Shape",
            &[],
            vec![
                ctor("Circle", &[], vec![float.clone()], Rep::Tagged),
                ctor("Rect", &[], vec![float.clone(), float], Rep::Tagged),
                ctor("Empty", &[], Vec::new(), Rep::Tagged),
            ],
        )],
    );
    let program = make_program(vec![app]);
    let shape_ty = bare("App", "Shape");

    for table in [PrimitiveTable::target_32(), PrimitiveTable::target_64()] {
        let layout = compute_layout(&program, &table, &shape_ty).expect("Shape layout");
        assert_eq!(layout.size, 24);
        assert_eq!(layout.align, 8);
        assert_eq!(
            layout.niche,
            Some(Niche {
                offset: 0,
                width: 1,
                start: 3,
                end: 255,
            })
        );
        let data = as_data(&layout);
        assert!(!data.unboxed);
        assert!(data.pointer_fields.is_empty());
        assert_eq!(
            data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        assert_eq!(data.variants.len(), 3);
        // Circle Float
        assert_eq!(data.variants[0].name, "Circle");
        assert_eq!(data.variants[0].tag, Some(0));
        assert_eq!(data.variants[0].fields.len(), 1);
        assert_eq!(data.variants[0].fields[0].offset, 8);
        assert_eq!(data.variants[0].fields[0].size, 8);
        assert_eq!(data.variants[0].fields[0].storage, FieldStorage::ByValue);
        // Rect Float Float
        assert_eq!(data.variants[1].name, "Rect");
        assert_eq!(data.variants[1].tag, Some(1));
        assert_eq!(data.variants[1].fields.len(), 2);
        assert_eq!(data.variants[1].fields[0].offset, 8);
        assert_eq!(data.variants[1].fields[1].offset, 16);
        assert_eq!(data.variants[1].fields[0].storage, FieldStorage::ByValue);
        assert_eq!(data.variants[1].fields[1].storage, FieldStorage::ByValue);
        // Empty (payload-free discriminant value only)
        assert_eq!(data.variants[2].name, "Empty");
        assert_eq!(data.variants[2].tag, Some(2));
        assert!(data.variants[2].fields.is_empty());
    }
}

#[test]
fn option_int_layout_on_32_and_64_bit_tables() {
    let program = make_program(Vec::new());
    let opt_int = named("Prelude", "Option", vec![bare("Prelude", "Int")]);

    for table in [PrimitiveTable::target_32(), PrimitiveTable::target_64()] {
        let layout = compute_layout(&program, &table, &opt_int).expect("Option Int layout");
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
        assert_eq!(
            layout.niche,
            Some(Niche {
                offset: 0,
                width: 1,
                start: 2,
                end: 255,
            })
        );
        let data = as_data(&layout);
        assert!(data.pointer_fields.is_empty());
        assert_eq!(
            data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        assert_eq!(data.variants[0].name, "None");
        assert_eq!(data.variants[0].tag, Some(0));
        assert!(data.variants[0].fields.is_empty());
        assert_eq!(data.variants[1].name, "Some");
        assert_eq!(data.variants[1].tag, Some(1));
        assert_eq!(data.variants[1].fields.len(), 1);
        assert_eq!(data.variants[1].fields[0].offset, 4);
        assert_eq!(data.variants[1].fields[0].size, 4);
        assert_eq!(data.variants[1].fields[0].align, 4);
        assert_eq!(data.variants[1].fields[0].storage, FieldStorage::ByValue);
    }
}

#[test]
fn option_list_int_uses_null_pointer_niche_on_32_and_64_bit_tables() {
    let program = make_program(Vec::new());
    let list_int = named("Prelude", "List", vec![bare("Prelude", "Int")]);
    let opt_list_int = named("Prelude", "Option", vec![list_int]);

    for (table, ptr_size) in [
        (PrimitiveTable::target_32(), 4u64),
        (PrimitiveTable::target_64(), 8u64),
    ] {
        let layout = compute_layout(&program, &table, &opt_list_int).expect("Option (List Int)");
        assert_eq!(layout.size, ptr_size);
        assert_eq!(layout.align, ptr_size);
        assert_eq!(layout.niche, None);
        let data = as_data(&layout);
        assert!(data.pointer_fields.is_empty());
        assert_eq!(
            data.discriminant,
            Discriminant::Niche {
                offset: 0,
                width: ptr_size,
                data_variant: 1,
                niche_start: 0,
                niche_end: 0,
            }
        );
        // None is encoded as null (0) with no payload fields
        assert_eq!(data.variants[0].name, "None");
        assert_eq!(data.variants[0].tag, Some(0));
        assert!(data.variants[0].fields.is_empty());
        // Some holds the non-null List pointer directly at offset 0
        assert_eq!(data.variants[1].name, "Some");
        assert_eq!(data.variants[1].tag, None);
        assert_eq!(data.variants[1].fields.len(), 1);
        assert_eq!(data.variants[1].fields[0].offset, 0);
        assert_eq!(data.variants[1].fields[0].size, ptr_size);
        assert_eq!(data.variants[1].fields[0].align, ptr_size);
        assert_eq!(data.variants[1].fields[0].storage, FieldStorage::ByValue);
    }
}

#[test]
fn ordering_layout_on_32_and_64_bit_tables() {
    let program = make_program(Vec::new());
    let ordering = bare("Prelude", "Ordering");

    for table in [PrimitiveTable::target_32(), PrimitiveTable::target_64()] {
        let layout = compute_layout(&program, &table, &ordering).expect("Ordering layout");
        assert_eq!(layout.size, 1);
        assert_eq!(layout.align, 1);
        assert_eq!(
            layout.niche,
            Some(Niche {
                offset: 0,
                width: 1,
                start: 3,
                end: 255,
            })
        );
        let data = as_data(&layout);
        assert!(data.pointer_fields.is_empty());
        assert_eq!(
            data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        let tags: Vec<(&str, Option<u64>, bool)> = data
            .variants
            .iter()
            .map(|v| (v.name.as_str(), v.tag, v.fields.is_empty()))
            .collect();
        assert_eq!(
            tags,
            vec![
                ("LT", Some(0), true),
                ("EQ", Some(1), true),
                ("GT", Some(2), true),
            ]
        );
    }
}

#[test]
fn mutually_recursive_types_across_two_modules_on_32_and_64_bit_tables() {
    let alpha = make_module(
        "Alpha",
        vec![data_decl(
            "Expr",
            &[],
            vec![
                ctor("Lit", &[], vec![bare("Prelude", "Int")], Rep::Tagged),
                ctor("Group", &[], vec![bare("Beta", "Term")], Rep::Tagged),
            ],
        )],
    );
    let beta = make_module(
        "Beta",
        vec![data_decl(
            "Term",
            &[],
            vec![
                ctor("Val", &[], vec![bare("Prelude", "Float")], Rep::Tagged),
                ctor("Wrap", &[], vec![bare("Alpha", "Expr")], Rep::Tagged),
            ],
        )],
    );
    let program = make_program(vec![alpha, beta]);
    let expr_ty = bare("Alpha", "Expr");
    let term_ty = bare("Beta", "Term");

    for (table, ptr_size, expr_size, expr_align, ptr_off) in [
        (PrimitiveTable::target_32(), 4u64, 8u64, 4u64, 4u64),
        (PrimitiveTable::target_64(), 8u64, 16u64, 8u64, 8u64),
    ] {
        let mut engine_a = LayoutEngine::new(&program, table.clone()).expect("engine A");
        let expr_l = engine_a.layout(&expr_ty).expect("Alpha.Expr");
        let term_l = engine_a.layout(&term_ty).expect("Beta.Term");

        // Query order independence: starting from Beta.Term gives the exact same layouts.
        let mut engine_b = LayoutEngine::new(&program, table).expect("engine B");
        let term_l2 = engine_b.layout(&term_ty).expect("Beta.Term second");
        let expr_l2 = engine_b.layout(&expr_ty).expect("Alpha.Expr second");
        assert_eq!(expr_l, expr_l2);
        assert_eq!(term_l, term_l2);

        // Alpha.Expr
        assert_eq!(expr_l.size, expr_size);
        assert_eq!(expr_l.align, expr_align);
        let expr_data = as_data(&expr_l);
        assert_eq!(expr_data.pointer_fields, BTreeSet::from([(1, 0)]));
        assert_eq!(expr_data.variants[0].fields[0].offset, 4);
        assert_eq!(
            expr_data.variants[0].fields[0].storage,
            FieldStorage::ByValue
        );
        assert_eq!(expr_data.variants[1].fields[0].offset, ptr_off);
        assert_eq!(expr_data.variants[1].fields[0].size, ptr_size);
        assert_eq!(
            expr_data.variants[1].fields[0].storage,
            FieldStorage::Pointer
        );

        // Beta.Term
        assert_eq!(term_l.size, 16);
        assert_eq!(term_l.align, 8);
        let term_data = as_data(&term_l);
        assert_eq!(term_data.pointer_fields, BTreeSet::from([(1, 0)]));
        assert_eq!(term_data.variants[0].fields[0].offset, 8);
        assert_eq!(
            term_data.variants[0].fields[0].storage,
            FieldStorage::ByValue
        );
        assert_eq!(term_data.variants[1].fields[0].offset, ptr_off);
        assert_eq!(term_data.variants[1].fields[0].size, ptr_size);
        assert_eq!(
            term_data.variants[1].fields[0].storage,
            FieldStorage::Pointer
        );
    }
}

#[test]
fn closed_record_reorders_fields_to_reduce_padding_and_preserves_canonical_labels() {
    let program = make_program(Vec::new());
    let mut interner = TypeInterner::new();
    // Deliberately supply labels unsorted to confirm canonical UTF-16 order.
    let rec_ty = interner.intern(IrType::Record {
        fields: vec![
            ("tag".into(), bare("Prelude", "Bool")),
            ("label".into(), bare("Prelude", "String")),
            ("active".into(), bare("Prelude", "Bool")),
            ("count".into(), bare("Prelude", "Int")),
        ],
        tail: None,
    });

    // 32-bit: count(4) @ 0, label(4) @ 4, active(1) @ 8, tag(1) @ 9 -> size 12, align 4
    let l32 = compute_layout(&program, &PrimitiveTable::target_32(), &rec_ty).expect("32-bit rec");
    assert_eq!(l32.size, 12);
    assert_eq!(l32.align, 4);
    assert_eq!(
        l32.niche,
        Some(Niche {
            offset: 8,
            width: 1,
            start: 2,
            end: 255,
        })
    );
    let TypeRepr::Record(r32) = &l32.repr else {
        panic!("expected Record");
    };
    let entries32: Vec<(&str, u64, u64)> = r32
        .fields
        .iter()
        .map(|f| (f.label.as_str(), f.field.offset, f.field.size))
        .collect();
    assert_eq!(
        entries32,
        vec![
            ("active", 8, 1),
            ("count", 0, 4),
            ("label", 4, 4),
            ("tag", 9, 1),
        ]
    );

    // 64-bit: label(8) @ 0, count(4) @ 8, active(1) @ 12, tag(1) @ 13 -> size 16, align 8
    // (Without reordering, canonical order active/count/label/tag would take 24 bytes.)
    let l64 = compute_layout(&program, &PrimitiveTable::target_64(), &rec_ty).expect("64-bit rec");
    assert_eq!(l64.size, 16);
    assert_eq!(l64.align, 8);
    assert_eq!(
        l64.niche,
        Some(Niche {
            offset: 12,
            width: 1,
            start: 2,
            end: 255,
        })
    );
    let TypeRepr::Record(r64) = &l64.repr else {
        panic!("expected Record");
    };
    let entries64: Vec<(&str, u64, u64)> = r64
        .fields
        .iter()
        .map(|f| (f.label.as_str(), f.field.offset, f.field.size))
        .collect();
    assert_eq!(
        entries64,
        vec![
            ("active", 12, 1),
            ("count", 8, 4),
            ("label", 0, 8),
            ("tag", 13, 1),
        ]
    );
}

#[test]
fn nested_float_polymorphic_recursion_terminates_and_boxes_only_nest() {
    // Mirrors `tests/check/pass/types/recursion.fl`:
    //   type Nested a = Flat a | Nest (Nested (List a))
    let a = var("a");
    let list_a = named("Prelude", "List", vec![a.clone()]);
    let nested_list_a = named("App.Recursion", "Nested", vec![list_a]);
    let rec_mod = make_module(
        "App.Recursion",
        vec![data_decl(
            "Nested",
            &["a"],
            vec![
                ctor("Flat", &["a"], vec![a], Rep::Tagged),
                ctor("Nest", &["a"], vec![nested_list_a], Rep::Tagged),
            ],
        )],
    );
    let program = make_program(vec![rec_mod]);
    let nested_float = named("App.Recursion", "Nested", vec![bare("Prelude", "Float")]);

    for (table, ptr_size, nest_off) in [
        (PrimitiveTable::target_32(), 4u64, 4u64),
        (PrimitiveTable::target_64(), 8u64, 8u64),
    ] {
        let layout = compute_layout(&program, &table, &nested_float).expect("Nested Float");
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 8);
        let data = as_data(&layout);
        assert_eq!(data.pointer_fields, BTreeSet::from([(1, 0)]));
        assert_eq!(
            data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        // Flat stores Float by value
        assert_eq!(data.variants[0].name, "Flat");
        assert_eq!(data.variants[0].fields[0].offset, 8);
        assert_eq!(data.variants[0].fields[0].size, 8);
        assert_eq!(data.variants[0].fields[0].storage, FieldStorage::ByValue);
        // Nest stores Nested (List Float) behind a pointer
        assert_eq!(data.variants[1].name, "Nest");
        assert_eq!(data.variants[1].fields[0].offset, nest_off);
        assert_eq!(data.variants[1].fields[0].size, ptr_size);
        assert_eq!(data.variants[1].fields[0].storage, FieldStorage::Pointer);
    }
}
