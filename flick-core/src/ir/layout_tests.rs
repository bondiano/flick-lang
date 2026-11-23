//! End-to-end tests computing shared data layouts from real compiler-lowered
//! typed IR (`flick-core` -> `flick_backend_support::layout`), including
//! `tests/check/pass/types/recursion.fl` (`Nested Float`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use flick_backend_support::layout::{
    Discriminant, FieldStorage, PrimitiveTable, TypeRepr, compute_layout,
};
use flick_ir::{IrType, Program, TypeRef};

use super::lower_text;
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
