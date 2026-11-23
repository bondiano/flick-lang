//! Property-based tests encoding and decoding arbitrary values of generated
//! types through their computed layouts under arbitrary primitive tables.

use std::collections::BTreeSet;

use flick_ir::{EnumLit, IrType, JsClass, Program, Rep, Stage, TypeDecl, TypeInterner, TypeRef};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

use super::super::{LayoutEngine, Niche, Primitive, PrimitiveLayout, PrimitiveTable, TypeRepr};
use super::codec::{
    Heap, Value, assert_layout_invariants, decode, decode_variant_index, encode, valid_handle_bytes,
};
use super::{as_data, bare, ctor, data_decl, make_module, make_program, named};

/// Generates an arbitrary valid [`PrimitiveTable`] (varying pointer width,
/// niches, fat vs thin handles, closure size, and alignments).
fn arb_primitive_table() -> impl Strategy<Value = PrimitiveTable> {
    (
        prop_oneof![Just(4u64), Just(8u64)], // pointer size
        any::<bool>(),                       // non-null pointer niche?
        any::<bool>(),                       // Bool has niche 2..=255?
        prop_oneof![Just(4u64), Just(8u64)], // Float alignment
        any::<bool>(),                       // String is fat (2*ptr) vs thin (ptr)
        any::<bool>(),                       // Closure is fat (2*ptr) vs thin (ptr)
        0u8..3u8,                            // Bytes niche: 0=none, 1=null, 2=u64::MAX sentinel
    )
        .prop_map(
            |(ptr_sz, ptr_niche, bool_niche, float_align, fat_str, fat_closure, bytes_niche)| {
                let ptr_n = ptr_niche.then(|| Niche::null_pointer(ptr_sz));
                let ptr = PrimitiveLayout::new(ptr_sz, ptr_sz, ptr_n);
                let string = if fat_str {
                    PrimitiveLayout::new(ptr_sz * 2, ptr_sz, ptr_n)
                } else {
                    ptr
                };
                let closure = if fat_closure {
                    PrimitiveLayout::new(ptr_sz * 2, ptr_sz, ptr_n)
                } else {
                    ptr
                };
                let bytes_n = match bytes_niche {
                    1 => Some(Niche::null_pointer(ptr_sz)),
                    2 if ptr_sz == 8 => Some(Niche {
                        offset: 0,
                        width: 8,
                        start: u64::MAX,
                        end: u64::MAX,
                    }),
                    2 => Some(Niche {
                        offset: 0,
                        width: 4,
                        start: u64::from(u32::MAX),
                        end: u64::from(u32::MAX),
                    }),
                    _ => None,
                };
                let bytes = PrimitiveLayout::new(ptr_sz, ptr_sz, bytes_n);
                let mut table = if ptr_sz == 4 {
                    PrimitiveTable::target_32()
                } else {
                    PrimitiveTable::target_64()
                };
                table.bool = PrimitiveLayout::new(1, 1, bool_niche.then(Niche::bool));
                table.float = PrimitiveLayout::new(8, float_align, None);
                table.pointer = ptr;
                table.closure = closure;
                table.string = string;
                table.bigint = ptr;
                table.bytes = bytes;
                table
            },
        )
}

/// Generates a random monomorphic IR type over `available_adts` and primitives.
fn arb_type(available_adts: Vec<TypeRef>) -> impl Strategy<Value = TypeRef> {
    let mut leaves: Vec<TypeRef> = vec![
        bare("Prelude", "Unit"),
        bare("Prelude", "Bool"),
        bare("Prelude", "Int"),
        bare("Prelude", "Float"),
        bare("Prelude", "Int8"),
        bare("Prelude", "Int16"),
        bare("Prelude", "Int64"),
        bare("Prelude", "UInt8"),
        bare("Prelude", "UInt16"),
        bare("Prelude", "UInt32"),
        bare("Prelude", "UInt64"),
        bare("Prelude", "Float32"),
        bare("Prelude", "String"),
        bare("Prelude", "BigInt"),
        bare("Bytes", "Bytes"),
        named("Prelude", "List", vec![bare("Prelude", "Int")]),
        bare("Prelude", "Ordering"),
        TypeRef::new(IrType::Fun {
            param: bare("Prelude", "Int"),
            stage: Stage::Pure,
            result: bare("Prelude", "Int"),
        }),
    ];
    leaves.extend(available_adts);

    let leaf = select(leaves);
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            3 => inner.clone().prop_map(|t| named("Prelude", "Option", vec![t])),
            2 => vec(inner.clone(), 0..=4).prop_map(|items| TypeRef::new(IrType::Tuple(items))),
            2 => vec(inner, 0..=4).prop_map(|tys| {
                let mut interner = TypeInterner::new();
                let labels = ["a", "b", "c", "d"];
                let fields: Vec<(String, TypeRef)> = labels
                    .into_iter()
                    .zip(tys)
                    .map(|(l, t)| (l.to_owned(), t))
                    .collect();
                interner.record(fields, None)
            }),
        ]
    })
}

fn build_js_form_decl(rep_mode: usize) -> TypeDecl {
    match rep_mode {
        1 => data_decl(
            "JsForm",
            &[],
            vec![
                ctor("K0", &[], Vec::new(), Rep::Literal(EnumLit::Int(10))),
                ctor("K1", &[], Vec::new(), Rep::Literal(EnumLit::Int(20))),
                ctor("K2", &[], Vec::new(), Rep::Literal(EnumLit::Int(30))),
            ],
        ),
        2 => data_decl(
            "JsForm",
            &[],
            vec![
                ctor(
                    "FNum",
                    &[],
                    vec![bare("Prelude", "Float")],
                    Rep::Typeof(JsClass::Number),
                ),
                ctor(
                    "FStr",
                    &[],
                    vec![bare("Prelude", "String")],
                    Rep::Typeof(JsClass::String),
                ),
            ],
        ),
        _ => data_decl(
            "JsForm",
            &[],
            vec![
                ctor("J0", &[], Vec::new(), Rep::Tagged),
                ctor("J1", &[], vec![bare("Prelude", "Bool")], Rep::Tagged),
            ],
        ),
    }
}

/// Generates a small program with custom ADTs (including non-recursive,
/// unboxed, literal enum, direct-recursive, and mutually recursive across two
/// modules) together with a target monomorphic type to test.
fn arb_program_and_type() -> impl Strategy<Value = (Program, TypeRef)> {
    let field_leaf = prop_oneof![
        Just(bare("Prelude", "Unit")),
        Just(bare("Prelude", "Bool")),
        Just(bare("Prelude", "Int")),
        Just(bare("Prelude", "Float")),
        Just(bare("Prelude", "Int8")),
        Just(bare("Prelude", "Int16")),
        Just(bare("Prelude", "String")),
        Just(bare("Bytes", "Bytes")),
        Just(named("Prelude", "List", vec![bare("Prelude", "Int")])),
        Just(bare("Prelude", "Ordering")),
    ];

    let custom_variants = vec(vec(field_leaf.clone(), 0..=3), 1..=5);
    let rec_base = vec(field_leaf.clone(), 0..=2);
    let rec_step_extras = vec(field_leaf.clone(), 0..=2);
    let mutual_base_a = vec(field_leaf.clone(), 0..=2);
    let mutual_base_b = vec(field_leaf.clone(), 0..=2);
    let unboxed_inner = field_leaf;
    let rep_mode = 0usize..3;

    (
        custom_variants,
        rec_base,
        rec_step_extras,
        mutual_base_a,
        mutual_base_b,
        unboxed_inner,
        rep_mode,
    )
        .prop_flat_map(
            |(custom_vs, rec_base, rec_extras, mut_a, mut_b, unboxed_ty, rep_mode)| {
                let custom_ctors = custom_vs
                    .into_iter()
                    .enumerate()
                    .map(|(i, fs)| ctor(&format!("C{i}"), &[], fs, Rep::Tagged))
                    .collect();

                let mut step_fields = rec_extras;
                step_fields.push(bare("ModA", "Rec"));
                let rec_ctors = vec![
                    ctor("Base", &[], rec_base, Rep::Tagged),
                    ctor("Step", &[], step_fields, Rep::Tagged),
                ];

                let unboxed_decl = data_decl(
                    "Wrap",
                    &[],
                    vec![ctor("Wrap", &[], vec![unboxed_ty], Rep::Unboxed)],
                );
                let even_decl = data_decl(
                    "Even",
                    &[],
                    vec![
                        ctor("EvenEnd", &[], mut_a, Rep::Tagged),
                        ctor("EvenNext", &[], vec![bare("ModB", "Odd")], Rep::Tagged),
                    ],
                );
                let odd_decl = data_decl(
                    "Odd",
                    &[],
                    vec![
                        ctor("OddEnd", &[], mut_b, Rep::Tagged),
                        ctor("OddNext", &[], vec![bare("ModA", "Even")], Rep::Tagged),
                    ],
                );

                let mod_a = make_module(
                    "ModA",
                    vec![
                        data_decl("Custom", &[], custom_ctors),
                        data_decl("Rec", &[], rec_ctors),
                        unboxed_decl,
                        build_js_form_decl(rep_mode),
                        even_decl,
                    ],
                );
                let mod_b = make_module("ModB", vec![odd_decl]);
                let program = make_program(vec![mod_a, mod_b]);

                let adts = vec![
                    bare("ModA", "Custom"),
                    bare("ModA", "Rec"),
                    bare("ModA", "Wrap"),
                    bare("ModA", "JsForm"),
                    bare("ModA", "Even"),
                    bare("ModB", "Odd"),
                ];
                (Just(program), arb_type(adts))
            },
        )
}

/// Deterministically synthesizes a finite [`Value`] for `ty` using a simple
/// PRNG stream `rng`, with bounded recursion depth.
struct ValueRng {
    state: u64,
}

impl ValueRng {
    fn new(seed: u64) -> ValueRng {
        ValueRng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state ^ (self.state >> 32)
    }
}

fn gen_value(
    engine: &mut LayoutEngine<'_>,
    ty: &TypeRef,
    rng: &mut ValueRng,
    depth: usize,
) -> Value {
    let layout = engine.layout(ty).expect("layout succeeds");
    match layout.repr {
        TypeRepr::Primitive(prim) => gen_primitive(prim, engine.primitives().get(prim), rng),
        TypeRepr::Tuple(t) => {
            let items = t
                .fields
                .iter()
                .map(|f| gen_value(engine, &f.ty, rng, depth))
                .collect();
            Value::Tuple(items)
        }
        TypeRepr::Record(r) => {
            let items = r
                .fields
                .iter()
                .map(|rf| {
                    (
                        rf.label.clone(),
                        gen_value(engine, &rf.field.ty, rng, depth),
                    )
                })
                .collect();
            Value::Record(items)
        }
        TypeRepr::Data(d) => {
            // When at depth 0, pick a variant without pointer-indirected fields
            // (always exists because our generators include a base constructor).
            let variant = if depth == 0 {
                d.variants
                    .iter()
                    .position(|v| v.fields.iter().all(|f| !f.is_pointer()))
                    .unwrap_or(0)
            } else {
                usize::try_from(rng.next_u64()).expect("fits") % d.variants.len()
            };
            gen_data_variant(engine, &d.variants[variant], variant, rng, depth)
        }
    }
}

fn gen_data_variant(
    engine: &mut LayoutEngine<'_>,
    v: &super::super::VariantLayout,
    variant: usize,
    rng: &mut ValueRng,
    depth: usize,
) -> Value {
    let fields = v
        .fields
        .iter()
        .map(|f| {
            let next_depth = if f.is_pointer() {
                depth.saturating_sub(1)
            } else {
                depth
            };
            gen_value(engine, &f.ty, rng, next_depth)
        })
        .collect();
    Value::Data { variant, fields }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn gen_primitive(prim: Primitive, layout: PrimitiveLayout, rng: &mut ValueRng) -> Value {
    let x = rng.next_u64();
    match prim {
        Primitive::Unit => Value::Unit,
        Primitive::Bool => Value::Bool((x & 1) == 1),
        Primitive::Int => Value::Int(x as i32),
        Primitive::Float => Value::FloatBits(x),
        Primitive::Int8 => Value::Int8(x as i8),
        Primitive::Int16 => Value::Int16(x as i16),
        Primitive::Int64 => Value::Int64(x as i64),
        Primitive::UInt8 => Value::UInt8(x as u8),
        Primitive::UInt16 => Value::UInt16(x as u16),
        Primitive::UInt32 => Value::UInt32(x as u32),
        Primitive::UInt64 => Value::UInt64(x),
        Primitive::Float32 => Value::Float32Bits(x as u32),
        Primitive::Pointer
        | Primitive::Closure
        | Primitive::String
        | Primitive::BigInt
        | Primitive::Bytes => Value::Handle(valid_handle_bytes(layout, x)),
        Primitive::Never => panic!("no values of Never"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_types_encode_decode_round_trip_and_distinguish_constructors(
        table in arb_primitive_table(),
        (program, ty) in arb_program_and_type(),
        seed in any::<u64>(),
    ) {
        let mut engine = LayoutEngine::new(&program, table.clone()).expect("valid table");
        let layout = engine.layout(&ty).expect("monomorphic layout succeeds");
        assert_layout_invariants(&mut engine, &layout);

        // Determinism under reversed module order.
        let mut rev_program = program.clone();
        rev_program.modules.reverse();
        let mut rev_engine = LayoutEngine::new(&rev_program, table).expect("valid table");
        let rev_layout = rev_engine.layout(&ty).expect("layout succeeds");
        prop_assert_eq!(&layout, &rev_layout);

        // Round-trip arbitrary values with two distinct padding poison bytes
        // to prove decoding never depends on padding bytes.
        let mut rng = ValueRng::new(seed);
        for _ in 0..8 {
            let value = gen_value(&mut engine, &ty, &mut rng, 3);
            for poison in [0x00u8, 0xA5u8, 0xFFu8] {
                let mut heap = Heap::default();
                let bytes = encode(&mut engine, &ty, &value, &mut heap, poison);
                prop_assert_eq!(bytes.len() as u64, layout.size);
                let decoded = decode(&mut engine, &ty, &bytes, &heap);
                prop_assert_eq!(&decoded, &value);
            }
        }

        // Distinct constructors always produce distinct byte encodings and
        // decode to their own constructor index.
        if let TypeRepr::Data(data) = &layout.repr
            && data.variants.len() >= 2
        {
            let mut seen_encodings: BTreeSet<Vec<u8>> = BTreeSet::new();
            for (v_idx, v) in data.variants.iter().enumerate() {
                let value = gen_data_variant(&mut engine, v, v_idx, &mut rng, 2);
                let mut heap = Heap::default();
                let bytes = encode(&mut engine, &ty, &value, &mut heap, 0x00);
                let decoded_idx = decode_variant_index(&data.discriminant, &data.variants, &bytes);
                prop_assert_eq!(decoded_idx, v_idx);
                prop_assert!(
                    seen_encodings.insert(bytes),
                    "distinct constructors of {ty} produced identical byte encodings"
                );
                let mut h = Heap::default();
                let encoded = encode(&mut engine, &ty, &value, &mut h, 0x5A);
                let decoded = decode(&mut engine, &ty, &encoded, &h);
                prop_assert_eq!(decoded, value);
            }
        }

        // Non-recursive custom ADT `ModA.Custom` never receives unnecessary pointers.
        let custom_layout = engine.layout(&bare("ModA", "Custom")).expect("Custom");
        prop_assert!(as_data(&custom_layout).pointer_fields.is_empty());
    }
}
