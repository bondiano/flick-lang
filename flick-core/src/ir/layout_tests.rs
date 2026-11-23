//! End-to-end tests computing shared data layouts from real compiler-lowered
//! typed IR (`flick-core` -> `flick_backend_support::layout`), including
//! `tests/check/pass/types/recursion.fl` (`Nested Float`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use flick_backend_support::layout::{
    Discriminant, FieldStorage, LayoutEngine, LayoutError, PrimitiveTable, TypeRepr, compute_layout,
};
use flick_ir::{IrType, Program, TypeRef};

use super::lower_text;
use crate::build::builtin_modules;
use crate::ir::lang_items;

#[test]
fn recursion_fixture_nested_float_layout_boxes_only_nest() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/types/recursion.fl");
    let source = fs::read_to_string(&path).expect("recursion.fl exists");
    let module = lower_text("App.Recursion", &source).expect("recursion.fl lowers");
    assert_eq!(module.name, "App.Recursion");

    let program = Program {
        ir_version: flick_ir::VERSION.to_owned(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![module],
        lang: lang_items(),
    };
    let nested_float = TypeRef::new(IrType::Named {
        module: "App.Recursion".into(),
        name: "Nested".into(),
        args: vec![TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Float".into(),
            args: Vec::new(),
        })],
    });

    for (table, ptr_size, nest_offset) in [
        (PrimitiveTable::target_32(), 4u64, 4u64),
        (PrimitiveTable::target_64(), 8u64, 8u64),
    ] {
        let layout = compute_layout(&program, &table, &nested_float).expect("Nested Float layout");
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 8);
        let TypeRepr::Data(data) = &layout.repr else {
            panic!("expected Data layout, got {:?}", layout.repr);
        };
        assert_eq!(
            data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1,
            }
        );
        assert_eq!(data.pointer_fields, BTreeSet::from([(1, 0)]));
        // Flat Float stays by value
        assert_eq!(data.variants[0].name, "Flat");
        assert_eq!(data.variants[0].fields[0].offset, 8);
        assert_eq!(data.variants[0].fields[0].size, 8);
        assert_eq!(data.variants[0].fields[0].storage, FieldStorage::ByValue);
        // Nest (Nested (List Float)) goes behind a pointer
        assert_eq!(data.variants[1].name, "Nest");
        assert_eq!(data.variants[1].fields[0].offset, nest_offset);
        assert_eq!(data.variants[1].fields[0].size, ptr_size);
        assert_eq!(data.variants[1].fields[0].storage, FieldStorage::Pointer);
    }
}

#[test]
fn recursive_opaque_fixture_with_prelude_builds_and_lays_out_without_overflow() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/types/recursive_opaque.fl");
    let source = fs::read_to_string(&path).expect("recursive_opaque.fl exists");
    let module = lower_text("App.RecursiveOpaque", &source).expect("recursive_opaque.fl lowers");
    let prelude = builtin_modules()
        .iter()
        .find(|m| m.name == "Prelude")
        .expect("Prelude builtin module")
        .clone();
    let program = Program {
        ir_version: flick_ir::VERSION.to_owned(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![prelude, module],
        lang: lang_items(),
    };
    let bare = |name: &str| {
        TypeRef::new(IrType::Named {
            module: "App.RecursiveOpaque".into(),
            name: name.into(),
            args: Vec::new(),
        })
    };
    let chain_float = TypeRef::new(IrType::Named {
        module: "App.RecursiveOpaque".into(),
        name: "Chain".into(),
        args: vec![TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Float".into(),
            args: Vec::new(),
        })],
    });

    for (table, ptr_size, closure_size, node_size) in [
        (PrimitiveTable::target_32(), 4u64, 8u64, 8u64),
        (PrimitiveTable::target_64(), 8u64, 16u64, 16u64),
    ] {
        let mut engine = LayoutEngine::new(&program, table).expect("LayoutEngine::new succeeds");

        let stream = engine.layout(&bare("Stream")).expect("Stream");
        assert_eq!(stream.size, ptr_size);
        assert_eq!(stream.align, ptr_size);

        let chain = engine.layout(&chain_float).expect("Chain Float");
        assert_eq!(chain.size, ptr_size);
        assert_eq!(chain.align, ptr_size);

        let boxed = engine.layout(&bare("Box")).expect("Box");
        assert_eq!(boxed.size, ptr_size);

        let lost = engine.layout(&bare("Lost")).expect("Lost");
        assert_eq!(lost.size, ptr_size);

        let lp = engine.layout(&bare("Loop")).expect("Loop");
        assert_eq!(lp.size, closure_size);

        let node = engine.layout(&bare("Node")).expect("Node");
        assert_eq!(node.size, node_size);

        let many = engine.layout(&bare("Many")).expect("Many");
        assert_eq!(many.size, ptr_size);

        assert!(matches!(
            engine.layout(&bare("Pair")),
            Err(LayoutError::CyclicAlias { .. })
        ));
    }
}
