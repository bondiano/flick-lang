//! Tests of `Program` serialization, wire interning, and version checks.

use std::collections::BTreeMap;
use std::{mem, thread};

use super::{Error, Program, VERSION, compatible};
use crate::{
    Expr, ExprKind, FileId, IrType, Item, ItemKind, LangItem, Lit, Module, Namespace, Span,
    TypeInterner,
};

fn program(x: f64) -> Program {
    let span = Span::new(FileId::new(7), 1, 4);
    let body = Expr::new(ExprKind::Lit(Lit::Float(x)), span);
    let item = Item {
        kind: ItemKind::Value {
            name: "x".to_owned(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body,
        },
        span,
    };
    let module = Module {
        name: "App".to_owned(),
        items: vec![item],
        types: Vec::new(),
        preambles: Vec::new(),
        labels: BTreeMap::default(),
        initialization: crate::Initialization::default(),
        records: BTreeMap::default(),
        package: None,
        exports: crate::Exports::default(),
    };
    let item = |lang: &str, namespace, name: &str| LangItem {
        lang: lang.to_owned(),
        module: "Prelude".to_owned(),
        namespace,
        name: name.to_owned(),
    };
    let lang = vec![
        item("some", Namespace::Constructor, "Just"),
        item("option", Namespace::Type, "Maybe"),
    ];
    Program::new("flick test", vec![module], lang, &|f| {
        format!("file{}.fl", f.index())
    })
}

#[test]
fn the_lang_items_are_in_the_order_of_their_roles() {
    let p = program(1.0);
    let lang: Vec<&str> = p.lang.iter().map(|i| i.lang.as_str()).collect();
    assert_eq!(lang, ["option", "some"]);
}

#[test]
fn spans_point_into_the_list_of_files() {
    let p = program(1.5);
    assert_eq!(p.files, ["file7.fl"]);
    assert_eq!(p.modules[0].items[0].span.file, FileId::new(0));
}

#[cfg(feature = "cbor")]
#[test]
fn the_slice_reader_accepts_the_writer_float_widths() {
    for value in [0.0, -0.0, 1.5, f64::INFINITY, f64::NAN, 0.1] {
        let expected = program(value);
        let decoded: Program = super::from_slice(&expected.to_cbor()).expect("writer CBOR");
        assert_eq!(decoded, expected);
    }
}

#[cfg(feature = "cbor")]
#[test]
fn a_deeply_nested_expression_survives_cbor() {
    let deep = thread::Builder::new().stack_size(64 << 20);
    deep.spawn(deep_round_trip)
        .expect("a thread")
        .join()
        .expect("deep IR round-trips");
}

#[cfg(feature = "cbor")]
fn deep_round_trip() {
    let mut expected = program(1.0);
    let ItemKind::Value { body, .. } = &mut expected.modules[0].items[0].kind else {
        unreachable!("the value of the program");
    };
    for _ in 0..1_000 {
        let span = body.span;
        let inner = mem::replace(body, Expr::new(ExprKind::Lit(Lit::Unit), span));
        *body = Expr::new(ExprKind::Yield(Box::new(inner)), span);
    }
    let decoded = Program::from_cbor(&expected.to_cbor()).expect("deep IR decodes");
    assert_eq!(decoded, expected);
}

#[test]
fn floats_survive_both_forms() {
    for x in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.1 + 0.2,
        1e300,
    ] {
        let p = program(x);
        assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()), "{x}");
        #[cfg(feature = "cbor")]
        assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p), "{x}");
    }
}

#[cfg(feature = "cbor")]
#[test]
fn cbor_keeps_accepting_tagged_programs_and_trailing_input() {
    let expected = program(1.5);
    let bytes = expected.to_cbor();
    let mut tagged = vec![0xd8, 42];
    tagged.extend_from_slice(&bytes);
    assert_eq!(Program::from_cbor(&tagged), Ok(expected.clone()));
    let mut trailing = bytes;
    trailing.push(0xff);
    assert_eq!(Program::from_cbor(&trailing), Ok(expected));
}

#[test]
fn fixed_width_literals_survive_both_forms() {
    let span = Span::new(FileId::new(0), 0, 1);
    let lits = vec![
        Lit::Int8(i8::MIN),
        Lit::Int8(i8::MAX),
        Lit::Int16(i16::MIN),
        Lit::Int16(i16::MAX),
        Lit::Int64(i64::MIN),
        Lit::Int64(i64::MAX),
        Lit::UInt8(0),
        Lit::UInt8(u8::MAX),
        Lit::UInt16(u16::MAX),
        Lit::UInt32(u32::MAX),
        Lit::UInt64(u64::MAX),
        Lit::Float32(f32::NAN),
        Lit::Float32(f32::INFINITY),
        Lit::Float32(f32::NEG_INFINITY),
        Lit::Float32(-0.0),
        Lit::Float32(0.0),
        Lit::Float32(0.1_f32),
        Lit::Float32(f32::MIN_POSITIVE),
        Lit::Float32(f32::from_bits(1)),
        Lit::Float32(f32::MAX),
        Lit::Float32(16_777_216.0_f32),
    ];
    for lit in lits {
        let mut p = program(0.0);
        p.modules[0].items[0].kind = ItemKind::Value {
            name: "v".into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body: Expr::new(ExprKind::Lit(lit.clone()), span),
        };
        assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()), "{lit:?}");
        #[cfg(feature = "cbor")]
        assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p), "{lit:?}");
    }
}

#[test]
fn a_reader_takes_only_its_own_minor_version_before_1_0() {
    assert!(compatible(VERSION));
    assert!(!compatible("0.5.0"));
    assert!(!compatible("0.3.0"));
    assert!(!compatible("1.0.0"));
    assert!(!compatible("0.1"));
    assert!(!compatible("0.10.0"));
    assert!(!compatible("0.11.0"));
    assert!(!compatible("0.12.0"));
    assert!(!compatible("0.13.0"));
    assert!(!compatible("0.14.0"));
    assert!(!compatible("0.15.0"));
    assert!(!compatible("0.16.0"));
    let mut p = program(1.0);
    p.ir_version = "0.18.0".to_owned();
    let found = "0.18.0".to_owned();
    assert_eq!(
        Program::from_json(&p.to_json()),
        Err(Error::Version { found })
    );
}

#[test]
fn foreign_adapter_words_round_trip_and_remap_spans() {
    let decl_span = Span::new(FileId::new(9), 0, 20);
    let word_span = Span::new(FileId::new(9), 10, 19);
    let preamble_span = Span::new(FileId::new(9), 1, 8);
    let inline_span = Span::new(FileId::new(9), 25, 42);
    let foreign = crate::Foreign {
        name: "call".into(),
        source_name: None,
        mode: crate::ForeignMode::Pure,
        binding: crate::Binding::Module {
            from: "./host".into(),
            name: "call".into(),
        },
        stages: vec![false, false],
        dicts: 0,
        arity: 2,
        adapter: crate::Adapter {
            owned: vec![0],
            words: vec![crate::BackendAdapter {
                name: "callback".into(),
                args: vec![
                    crate::AdapterArg::Int(0),
                    crate::AdapterArg::Word("data".into()),
                    crate::AdapterArg::Int(1),
                ],
                span: word_span,
            }],
            ..crate::Adapter::default()
        },
        catch: None,
        type_params: Vec::new(),
        ty: None,
    };
    let inline_foreign = crate::Foreign {
        name: "cos".into(),
        source_name: None,
        mode: crate::ForeignMode::Pure,
        binding: crate::Binding::Inline(crate::InlineCode {
            code: "(d) => Math.cos(d)".into(),
            span: inline_span,
        }),
        stages: vec![false],
        dicts: 0,
        arity: 1,
        adapter: crate::Adapter::default(),
        catch: None,
        type_params: Vec::new(),
        ty: None,
    };
    let module = Module {
        name: "App".into(),
        items: vec![
            Item {
                kind: ItemKind::Foreign(foreign),
                span: decl_span,
            },
            Item {
                kind: ItemKind::Foreign(inline_foreign),
                span: inline_span,
            },
        ],
        types: Vec::new(),
        preambles: vec![crate::InlineCode {
            code: "const K = 2;".into(),
            span: preamble_span,
        }],
        labels: BTreeMap::default(),
        initialization: crate::Initialization::default(),
        records: BTreeMap::default(),
        package: None,
        exports: crate::Exports::default(),
    };
    let p = Program::new("flick test", vec![module], Vec::new(), &|f| {
        format!("file{}.fl", f.index())
    });
    assert_eq!(p.files, ["file9.fl"]);
    assert_eq!(
        p.modules[0].preambles[0].span,
        Span::new(FileId::new(0), 1, 8)
    );
    let ItemKind::Foreign(f) = &p.modules[0].items[0].kind else {
        unreachable!();
    };
    assert_eq!(f.adapter.words[0].span, Span::new(FileId::new(0), 10, 19));
    let ItemKind::Foreign(inline_f) = &p.modules[0].items[1].kind else {
        unreachable!();
    };
    assert_eq!(
        inline_f.binding.inline().expect("inline").span,
        Span::new(FileId::new(0), 25, 42)
    );
    assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));
    #[cfg(feature = "cbor")]
    assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p));
}

#[test]
fn wire_interning_deduplicates_and_shares_types() {
    let mut interner = TypeInterner::new();
    let float_ty = interner.named("Prelude", "Float", Vec::new());
    let pair_ty = interner.tuple(vec![float_ty.clone(), float_ty.clone()]);
    let mut p = program(1.0);
    let span = p.modules[0].items[0].span;
    let a = Expr::with_ty(ExprKind::Lit(Lit::Float(1.0)), span, Some(float_ty.clone()));
    let b = Expr::with_ty(ExprKind::Lit(Lit::Float(2.0)), span, Some(float_ty.clone()));
    p.modules[0].items[0].kind = ItemKind::Value {
        name: "pair".into(),
        type_params: Vec::new(),
        dicts: Vec::new(),
        dict_constraints: Vec::new(),
        ty: Some(pair_ty.clone()),
        body: Expr::with_ty(ExprKind::Tuple(vec![a, b]), span, Some(pair_ty)),
    };
    let json = p.to_json();
    let raw: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    assert_eq!(raw["types"].as_array().map(Vec::len), Some(2));
    let back = Program::from_json(&json).expect("round-trip JSON");
    assert_eq!(back, p);
    assert_eq!(back.to_json(), json);
    let ItemKind::Value { ty, body, .. } = &back.modules[0].items[0].kind else {
        unreachable!();
    };
    let item_ty = ty.as_ref().expect("item type");
    let body_ty = body.ty.as_ref().expect("body type");
    assert!(item_ty.ptr_eq(body_ty));
    let ExprKind::Tuple(items) = &body.kind else {
        unreachable!();
    };
    let first_ty = items[0].ty.as_ref().expect("first element type");
    let second_ty = items[1].ty.as_ref().expect("second element type");
    assert!(first_ty.ptr_eq(second_ty));
    assert_eq!(
        *first_ty.as_bare_con().expect("bare constructor").1,
        *"Float"
    );
    assert!(matches!(&**item_ty, IrType::Tuple(v) if v.len() == 2));
    #[cfg(feature = "cbor")]
    {
        let cbor = p.to_cbor();
        let back = Program::from_cbor(&cbor).expect("round-trip CBOR");
        assert_eq!(back, p);
        assert_eq!(back.to_cbor(), cbor);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn wire_deserialization_accepts_modules_before_types() {
    let mut interner = TypeInterner::new();
    let float_ty = interner.named("Prelude", "Float", Vec::new());
    let pair_ty = interner.tuple(vec![float_ty.clone(), float_ty.clone()]);
    let mut p = program(1.0);
    let span = p.modules[0].items[0].span;
    let a = Expr::with_ty(ExprKind::Lit(Lit::Float(1.0)), span, Some(float_ty.clone()));
    let b = Expr::with_ty(ExprKind::Lit(Lit::Float(2.0)), span, Some(float_ty));
    p.modules[0].items[0].kind = ItemKind::Value {
        name: "pair".into(),
        type_params: Vec::new(),
        dicts: Vec::new(),
        dict_constraints: Vec::new(),
        ty: Some(pair_ty.clone()),
        body: Expr::with_ty(ExprKind::Tuple(vec![a, b]), span, Some(pair_ty)),
    };

    let raw: serde_json::Value = serde_json::from_str(&p.to_json()).expect("JSON");
    let obj = raw.as_object().expect("program JSON object");
    // Place `modules` before `types` explicitly.
    let reordered_json = format!(
        "{{\"modules\":{},\"lang\":{},\"files\":{},\"compiler\":{},\"ir_version\":{},\"types\":{}}}",
        obj["modules"], obj["lang"], obj["files"], obj["compiler"], obj["ir_version"], obj["types"]
    );
    let back = Program::from_json(&reordered_json).expect("out-of-order JSON decodes");
    assert_eq!(back, p);
    let ItemKind::Value { ty, body, .. } = &back.modules[0].items[0].kind else {
        unreachable!();
    };
    assert!(
        ty.as_ref()
            .expect("item ty")
            .ptr_eq(body.ty.as_ref().expect("body ty"))
    );
    let ExprKind::Tuple(items) = &body.kind else {
        unreachable!();
    };
    assert!(
        items[0]
            .ty
            .as_ref()
            .expect("first elem ty")
            .ptr_eq(items[1].ty.as_ref().expect("second elem ty"))
    );

    // Out-of-range type index in `modules` before `types` must return an error.
    let bad_types_json = format!(
        "{{\"modules\":{},\"lang\":{},\"files\":{},\"compiler\":{},\"ir_version\":{},\"types\":[]}}",
        obj["modules"], obj["lang"], obj["files"], obj["compiler"], obj["ir_version"]
    );
    assert!(
        Program::from_json(&bad_types_json).is_err(),
        "out-of-range deferred type index must fail"
    );

    #[cfg(feature = "cbor")]
    {
        fn to_cbor_val(v: &serde_json::Value) -> ciborium::Value {
            match v {
                serde_json::Value::Null => ciborium::Value::Null,
                serde_json::Value::Bool(b) => ciborium::Value::Bool(*b),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        ciborium::Value::Integer(i.into())
                    } else if let Some(u) = n.as_u64() {
                        ciborium::Value::Integer(u.into())
                    } else {
                        ciborium::Value::Float(n.as_f64().expect("json float"))
                    }
                }
                serde_json::Value::String(s) => ciborium::Value::Text(s.clone()),
                serde_json::Value::Array(a) => {
                    ciborium::Value::Array(a.iter().map(to_cbor_val).collect())
                }
                serde_json::Value::Object(m) => ciborium::Value::Map(
                    m.iter()
                        .map(|(k, v)| (ciborium::Value::Text(k.clone()), to_cbor_val(v)))
                        .collect(),
                ),
            }
        }
        let map = ciborium::Value::Map(vec![
            (
                ciborium::Value::Text("modules".into()),
                to_cbor_val(&obj["modules"]),
            ),
            (
                ciborium::Value::Text("lang".into()),
                to_cbor_val(&obj["lang"]),
            ),
            (
                ciborium::Value::Text("files".into()),
                to_cbor_val(&obj["files"]),
            ),
            (
                ciborium::Value::Text("compiler".into()),
                to_cbor_val(&obj["compiler"]),
            ),
            (
                ciborium::Value::Text("ir_version".into()),
                to_cbor_val(&obj["ir_version"]),
            ),
            (
                ciborium::Value::Text("types".into()),
                to_cbor_val(&obj["types"]),
            ),
        ]);
        let mut bytes = Vec::new();
        ciborium::into_writer(&map, &mut bytes).expect("CBOR encode");
        let back = Program::from_cbor(&bytes).expect("out-of-order CBOR decodes");
        assert_eq!(back, p);
        let ItemKind::Value { ty, body, .. } = &back.modules[0].items[0].kind else {
            unreachable!();
        };
        assert!(
            ty.as_ref()
                .expect("item ty")
                .ptr_eq(body.ty.as_ref().expect("body ty"))
        );
    }
}

#[test]
fn lexical_initialization_survives_both_forms() {
    let mut p = program(1.0);
    p.modules[0].initialization = crate::Initialization {
        prefixes: vec![crate::Prefix {
            before: 100,
            bindings: vec!["x".into()],
        }],
    };
    assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));
    #[cfg(feature = "cbor")]
    assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p));
}

#[test]
fn the_exports_of_a_module_survive_both_forms_as_a_reader_is_told() {
    let mut p = program(1.0);
    let written: serde_json::Value = serde_json::from_str(&p.to_json()).expect("JSON");
    assert_eq!(written["modules"][0].get("exports"), None);
    assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));

    let value = |name: &str, sync| crate::Exported {
        name: name.to_owned(),
        sync,
    };
    p.modules[0].exports = crate::Exports {
        values: vec![value("parse", Some(2)), value("x", None)],
        reexports: vec![crate::Reexport {
            module: "Http.Url".to_owned(),
            namespace: Namespace::Type,
            name: "Url".to_owned(),
        }],
    };
    let written: serde_json::Value = serde_json::from_str(&p.to_json()).expect("JSON");
    assert_eq!(
        written["modules"][0]["exports"],
        serde_json::json!({
            "values": [{ "name": "parse", "sync": 2 }, { "name": "x", "sync": null }],
            "reexports": [{ "module": "Http.Url", "namespace": "type", "name": "Url" }],
        })
    );
    assert_eq!(Program::from_json(&p.to_json()), Ok(p.clone()));
    #[cfg(feature = "cbor")]
    assert_eq!(Program::from_cbor(&p.to_cbor()), Ok(p));
}
