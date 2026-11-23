//! Recursion, GADT, transparent unboxed, cyclic alias, and higher-kinded
//! layout tests.

use std::collections::BTreeSet;

use flick_ir::{Constraint, ConstructorDecl, IrType, Rep, Stage, TypeDecl, TypeRef};

use super::super::{Discriminant, LayoutEngine, LayoutError, PrimitiveTable};
use super::{as_data, bare, ctor, data_decl, make_module, make_program, named, var};

#[test]
fn rose_tree_and_mutual_polymorphic_recursion() {
    let a = var("a");
    let list_a = named("Prelude", "List", vec![a.clone()]);
    let app = make_module(
        "App",
        vec![
            data_decl(
                "Rose",
                &["a"],
                vec![ctor(
                    "Rose",
                    &["a"],
                    vec![
                        a.clone(),
                        named(
                            "Prelude",
                            "List",
                            vec![named("App", "Rose", vec![a.clone()])],
                        ),
                    ],
                    Rep::Tagged,
                )],
            ),
            data_decl(
                "Even",
                &["a"],
                vec![
                    ctor("DoneE", &["a"], vec![a.clone()], Rep::Tagged),
                    ctor(
                        "StepE",
                        &["a"],
                        vec![named("App", "Odd", vec![list_a])],
                        Rep::Tagged,
                    ),
                ],
            ),
            data_decl(
                "Odd",
                &["a"],
                vec![
                    ctor("DoneO", &["a"], vec![a.clone()], Rep::Tagged),
                    ctor(
                        "StepO",
                        &["a"],
                        vec![named("App", "Even", vec![a])],
                        Rep::Tagged,
                    ),
                ],
            ),
        ],
    );
    let program = make_program(vec![app]);
    let mut engine = LayoutEngine::new(&program, PrimitiveTable::target_64()).expect("engine");

    let rose = engine
        .layout(&named("App", "Rose", vec![bare("Prelude", "Int")]))
        .expect("Rose Int");
    assert_eq!(rose.size, 16);
    assert_eq!(rose.align, 8);
    assert!(as_data(&rose).pointer_fields.is_empty());

    let even = engine
        .layout(&named("App", "Even", vec![bare("Prelude", "Int")]))
        .expect("Even Int");
    assert_eq!(even.size, 16);
    assert_eq!(as_data(&even).pointer_fields, BTreeSet::from([(1, 0)]));
}

#[test]
fn gadts_filter_unreachable_constructors_and_box_existentials() {
    let a = var("a");
    let b = var("b");
    let app = make_module(
        "App",
        vec![
            data_decl(
                "Expr",
                &["a"],
                vec![
                    ConstructorDecl {
                        name: "IntLit".into(),
                        rep: Rep::Tagged,
                        type_params: Vec::new(),
                        existentials: Vec::new(),
                        constraints: Vec::new(),
                        fields: vec![bare("Prelude", "Int")],
                        record: None,
                        result_args: Some(vec![bare("Prelude", "Int")]),
                    },
                    ConstructorDecl {
                        name: "Lift".into(),
                        rep: Rep::Tagged,
                        type_params: vec!["a".into()],
                        existentials: Vec::new(),
                        constraints: Vec::new(),
                        fields: vec![a.clone()],
                        record: None,
                        result_args: Some(vec![a.clone()]),
                    },
                    ConstructorDecl {
                        name: "Apply".into(),
                        rep: Rep::Tagged,
                        type_params: vec!["a".into(), "b".into()],
                        existentials: vec!["b".into()],
                        constraints: Vec::new(),
                        fields: vec![
                            named(
                                "App",
                                "Expr",
                                vec![TypeRef::new(IrType::Fun {
                                    param: b.clone(),
                                    stage: Stage::Pure,
                                    result: a.clone(),
                                })],
                            ),
                            named("App", "Expr", vec![b.clone()]),
                        ],
                        record: None,
                        result_args: Some(vec![a]),
                    },
                ],
            ),
            data_decl(
                "ShowBox",
                &[],
                vec![ConstructorDecl {
                    name: "ShowBox".into(),
                    rep: Rep::Tagged,
                    type_params: vec!["b".into()],
                    existentials: vec!["b".into()],
                    constraints: vec![Constraint {
                        module: "Prelude".into(),
                        class: "Show".into(),
                        args: vec![b.clone()],
                    }],
                    fields: vec![b],
                    record: None,
                    result_args: Some(Vec::new()),
                }],
            ),
        ],
    );
    let program = make_program(vec![app]);
    let mut engine = LayoutEngine::new(&program, PrimitiveTable::target_64()).expect("engine");

    let expr_str = engine
        .layout(&named("App", "Expr", vec![bare("Prelude", "String")]))
        .expect("Expr String");
    let expr_data = as_data(&expr_str);
    assert_eq!(expr_data.variants.len(), 2);
    assert_eq!(expr_data.variants[0].name, "Lift");
    assert_eq!(expr_data.variants[1].name, "Apply");
    assert_eq!(expr_data.pointer_fields, BTreeSet::from([(1, 0), (1, 1)]));

    let show_box = engine.layout(&bare("App", "ShowBox")).expect("ShowBox");
    assert_eq!(show_box.size, 16);
    assert_eq!(show_box.align, 8);
    let sb = as_data(&show_box);
    assert_eq!(sb.pointer_fields, BTreeSet::from([(0, 0)]));
    assert_eq!(sb.variants[0].dicts.len(), 1);
}

#[test]
fn cyclic_alias_and_unboxed_cycle_handling() {
    let app = make_module(
        "App",
        vec![
            TypeDecl {
                name: "BadAlias".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: Some(TypeRef::new(IrType::Tuple(vec![
                    bare("Prelude", "Int"),
                    bare("App", "BadAlias"),
                ]))),
            },
            data_decl(
                "SelfUnbox",
                &[],
                vec![ctor(
                    "SelfUnbox",
                    &[],
                    vec![bare("App", "SelfUnbox")],
                    Rep::Unboxed,
                )],
            ),
            data_decl(
                "NodeId",
                &[],
                vec![ctor("NodeId", &[], vec![bare("App", "Node")], Rep::Unboxed)],
            ),
            data_decl(
                "Node",
                &[],
                vec![
                    ctor("End", &[], Vec::new(), Rep::Tagged),
                    ctor("Next", &[], vec![bare("App", "NodeId")], Rep::Tagged),
                ],
            ),
        ],
    );
    let program = make_program(vec![app]);
    let mut engine = LayoutEngine::new(&program, PrimitiveTable::target_64()).expect("engine");

    assert!(matches!(
        engine.layout(&bare("App", "BadAlias")),
        Err(LayoutError::CyclicAlias { .. })
    ));

    let self_unbox = engine.layout(&bare("App", "SelfUnbox")).expect("SelfUnbox");
    assert_eq!(self_unbox.size, 8);
    assert_eq!(self_unbox.align, 8);
    assert_eq!(
        as_data(&self_unbox).pointer_fields,
        BTreeSet::from([(0, 0)])
    );

    let node = engine.layout(&bare("App", "Node")).expect("Node");
    let node_id = engine.layout(&bare("App", "NodeId")).expect("NodeId");
    assert_eq!(node.size, 8);
    assert_eq!(node.align, 8);
    assert_eq!(as_data(&node).pointer_fields, BTreeSet::from([(1, 0)]));
    assert!(matches!(
        as_data(&node).discriminant,
        Discriminant::Niche {
            offset: 0,
            width: 8,
            data_variant: 1,
            niche_start: 0,
            niche_end: 0,
        }
    ));
    assert_eq!(node_id.size, node.size);
    assert_eq!(node_id.align, node.align);
    assert!(as_data(&node_id).unboxed);
    assert!(as_data(&node_id).pointer_fields.is_empty());
}

#[test]
fn higher_kinded_non_recursive_and_growing_recursive_types() {
    let a = var("a");
    let f = var("f");
    let g = var("g");
    let app = make_module(
        "App",
        vec![
            data_decl(
                "Compose",
                &["f", "g", "a"],
                vec![ctor(
                    "Compose",
                    &["f", "g", "a"],
                    vec![TypeRef::new(IrType::App {
                        head: f.clone(),
                        args: vec![TypeRef::new(IrType::App {
                            head: g,
                            args: vec![a.clone()],
                        })],
                    })],
                    Rep::Tagged,
                )],
            ),
            data_decl(
                "Apply",
                &["f", "a"],
                vec![ctor(
                    "Apply",
                    &["f", "a"],
                    vec![TypeRef::new(IrType::App {
                        head: f,
                        args: vec![a.clone()],
                    })],
                    Rep::Tagged,
                )],
            ),
            data_decl(
                "HktNested",
                &["a"],
                vec![
                    ctor("HFlat", &["a"], vec![a.clone()], Rep::Tagged),
                    ctor(
                        "HNest",
                        &["a"],
                        vec![named(
                            "App",
                            "Apply",
                            vec![
                                named("App", "HktNested", Vec::new()),
                                named("Prelude", "List", vec![a]),
                            ],
                        )],
                        Rep::Tagged,
                    ),
                ],
            ),
        ],
    );
    let program = make_program(vec![app]);
    let mut engine = LayoutEngine::new(&program, PrimitiveTable::target_64()).expect("engine");

    let compose = engine
        .layout(&named(
            "App",
            "Compose",
            vec![
                named("Prelude", "Option", Vec::new()),
                named("Prelude", "Option", Vec::new()),
                bare("Prelude", "Bool"),
            ],
        ))
        .expect("Compose Option Option Bool");
    assert_eq!(compose.size, 1);
    assert_eq!(compose.align, 1);
    assert!(as_data(&compose).pointer_fields.is_empty());

    let hkt = engine
        .layout(&named("App", "HktNested", vec![bare("Prelude", "Float")]))
        .expect("HktNested Float");
    assert_eq!(hkt.size, 16);
    assert_eq!(hkt.align, 8);
    assert_eq!(as_data(&hkt).pointer_fields, BTreeSet::from([(1, 0)]));
}
