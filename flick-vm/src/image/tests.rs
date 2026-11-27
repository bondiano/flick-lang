use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, Instruction, Name, Operation, Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};

use super::heap_types::{HeapSnapshot, SnapNode, SnapObject, SnapValue};
use super::{IMAGE_MAGIC, ImageError, VmImage, compiler_hash, format_hash, memory_ir_hash};
use crate::session::{EvalOutcome, VmSession};
use crate::tier0::{Data, Heap, HostMode};

/// Builds module `M` with `make_adder` (`fun x -> fun y -> x + y`) and `tag` (`"secret-payload"`).
pub(super) fn adder_module_program(module: &str) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Closure {
                            code: 2,
                            captures: vec![1],
                        },
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    functions.insert(
        2,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Project {
                            object: 0,
                            field: 0,
                            rep: ValueRep::Cell,
                        },
                    },
                    Instruction::Drop(0),
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 2,
                            right: 1,
                        },
                    },
                ],
                exit: Exit::Return(3),
            },
        },
    );
    functions.insert(
        3,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::String("secret-payload".into())),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    Program {
        functions,
        globals: vec![
            Global {
                name: Name::new(module, "make_adder"),
                initializer: 0,
            },
            Global {
                name: Name::new(module, "tag"),
                initializer: 3,
            },
        ],
        op_spans: BTreeMap::new(),
    }
}

/// Builds module `C` importing `dep` with `C.add10 = dep.make_adder 10`.
pub(super) fn partial_consumer_program(module: &str, dep: &str) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new(dep, "make_adder")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(10)),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Apply {
                            function: 0,
                            argument: 1,
                        },
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, "add10"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

/// Builds phrase `P.it = target_mod.target_fn arg`.
pub(super) fn apply_global_program(
    phrase_mod: &str,
    target_mod: &str,
    target_fn: &str,
    arg: i32,
) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new(target_mod, target_fn)),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(arg)),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Apply {
                            function: 0,
                            argument: 1,
                        },
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(phrase_mod, "it"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

#[test]
fn version_hash_mismatch_names_both_hashes_and_truncation_never_panics() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let prog = adder_module_program("Math");
    let decl = session.eval_declarations(
        &prog,
        "Math",
        &[Name::new("Math", "make_adder"), Name::new("Math", "tag")],
        None,
        None,
    );
    assert!(matches!(decl, EvalOutcome::Declaration { .. }));

    let image = session.save_image(None).expect("save image");
    let bytes = image.encode().expect("encode image");
    assert_eq!(&bytes[0..4], IMAGE_MAGIC);

    // 1. Compiler hash mismatch names BOTH expected and found hashes.
    let mut bad_compiler = bytes.clone();
    bad_compiler[8] ^= 0xff;
    let err = VmImage::decode(&bad_compiler).expect_err("compiler mismatch");
    let msg = err.to_string();
    let expected_hex = format_hash(&compiler_hash());
    let mut found_arr = compiler_hash();
    found_arr[0] ^= 0xff;
    let found_hex = format_hash(&found_arr);
    assert!(
        msg.contains(&expected_hex) && msg.contains(&found_hex),
        "expected both compiler hashes in error, got: {msg}"
    );

    // 2. Memory-IR hash mismatch names BOTH expected and found hashes.
    let mut bad_ir = bytes.clone();
    bad_ir[40] ^= 0xaa;
    let err = VmImage::decode(&bad_ir).expect_err("ir mismatch");
    let msg = err.to_string();
    let expected_ir_hex = format_hash(&memory_ir_hash());
    let mut found_ir = memory_ir_hash();
    found_ir[0] ^= 0xaa;
    let found_ir_hex = format_hash(&found_ir);
    assert!(
        msg.contains(&expected_ir_hex) && msg.contains(&found_ir_hex),
        "expected both memory-IR hashes in error, got: {msg}"
    );

    // 3. Truncation at every byte boundary fails cleanly without panicking.
    for len in 0..bytes.len() {
        assert!(
            VmImage::decode(&bytes[..len]).is_err(),
            "truncated length {len} must fail"
        );
    }

    // 4. Corrupt magic, format version, and reserved flags.
    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    assert!(matches!(
        VmImage::decode(&bad_magic),
        Err(ImageError::Corrupt(_))
    ));

    let mut bad_ver = bytes.clone();
    bad_ver[4..8].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        VmImage::decode(&bad_ver),
        Err(ImageError::Corrupt(_))
    ));

    let mut bad_reserved = bytes;
    bad_reserved[76..80].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        VmImage::decode(&bad_reserved),
        Err(ImageError::Corrupt(_))
    ));

    // 5. Non-cell cyclic snapshot graph fails validation cleanly.
    let cyclic_snap = HeapSnapshot {
        objects: vec![SnapNode {
            owner_module: None,
            data: SnapObject::Tagged {
                tag: "Loop".into(),
                ordinary_fields: 1,
                fields: vec![SnapValue::Object(0)],
                layout: None,
            },
        }],
    };
    assert!(matches!(
        cyclic_snap.validate_structure(&BTreeMap::new()),
        Err(ImageError::Corrupt(_))
    ));

    assert_eq!(session.shutdown(), Ok(()));
}

fn seed_shared_and_cyclic_graph(s: &mut VmSession) {
    let shared_str = s.heap.allocate(Data::String("shared-atom".into()));
    let dup_str = s.heap.dup(&shared_str).expect("dup shared");
    let pair = s
        .heap
        .construct("Pair", vec![shared_str, dup_str])
        .expect("pair");
    s.persistent
        .globals
        .insert(Name::new("Graph", "pair"), pair);

    // 2-node Ref cycle: cell_a -> Link(cell_b), cell_b -> Link(cell_a).
    let cell_a = s.heap.allocate(Data::Cell {
        execution: s.execution,
        contents: None,
    });
    let cell_b = s.heap.allocate(Data::Cell {
        execution: s.execution,
        contents: None,
    });
    let first_id = cell_a.object_id().expect("first cell id");
    let second_id = cell_b.object_id().expect("second cell id");
    let node_to_second = s
        .heap
        .construct("Link", vec![Heap::cell_handle(second_id, s.execution)])
        .expect("link second");
    let node_to_first = s
        .heap
        .construct("Link", vec![Heap::cell_handle(first_id, s.execution)])
        .expect("link first");
    if let Data::Cell { contents, .. } = &mut s.heap.object_mut(first_id).expect("first").data {
        *contents = Some(node_to_second);
    }
    if let Data::Cell { contents, .. } = &mut s.heap.object_mut(second_id).expect("second").data {
        *contents = Some(node_to_first);
    }
    s.persistent.cells.push(cell_a);
    s.persistent.cells.push(cell_b);
    s.persistent.globals.insert(
        Name::new("Graph", "cycle_a"),
        Heap::cell_handle(first_id, s.execution),
    );
    let mut dummy_funcs = BTreeMap::new();
    dummy_funcs.insert(
        0 as CodeId,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Unit),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    s.program.load(&Program {
        functions: dummy_funcs,
        globals: vec![
            Global {
                name: Name::new("Graph", "pair"),
                initializer: 0,
            },
            Global {
                name: Name::new("Graph", "cycle_a"),
                initializer: 0,
            },
        ],
        op_spans: BTreeMap::new(),
    });
}

#[test]
fn shared_dag_aliasing_ref_cycles_and_partial_applications_survive_reload() {
    let mut s1 = VmSession::with_host_mode(HostMode::Deterministic);
    seed_shared_and_cyclic_graph(&mut s1);

    let math_prog = adder_module_program("Math");
    let _ = s1.eval_declarations(
        &math_prog,
        "Math",
        &[Name::new("Math", "make_adder")],
        None,
        None,
    );
    let cons_prog = partial_consumer_program("Consumer", "Math");
    let _ = s1.eval_declarations(
        &cons_prog,
        "Consumer",
        &[Name::new("Consumer", "add10")],
        None,
        None,
    );

    let bytes = s1.save_image(None).expect("save").encode().expect("encode");
    assert_eq!(s1.shutdown(), Ok(()));

    let decoded = VmImage::decode(&bytes).expect("decode");
    let mut s2 = VmSession::with_host_mode(HostMode::Deterministic);
    s2.load_image(&decoded).expect("load into fresh session");

    let restored_pair = s2
        .persistent
        .globals
        .get(&Name::new("Graph", "pair"))
        .expect("restored pair");
    let f0 = s2.heap.field(restored_pair, 0).expect("f0");
    let f1 = s2.heap.field(restored_pair, 1).expect("f1");
    assert_eq!(
        f0.object_id(),
        f1.object_id(),
        "DAG sharing must preserve ObjectId identity across save/load"
    );
    assert_eq!(s2.heap.reference_count(f0).expect("rc"), 2);

    let restored_a = s2
        .persistent
        .globals
        .get(&Name::new("Graph", "cycle_a"))
        .expect("cycle_a");
    let reloaded_first = restored_a.cell_id().expect("reloaded first cell id");
    let reloaded_second = match &s2.heap.object(reloaded_first).expect("obj first").data {
        Data::Cell {
            contents: Some(link_b),
            ..
        } => s2
            .heap
            .field(link_b, 0)
            .expect("second handle")
            .cell_id()
            .expect("second id"),
        other => panic!("unexpected cell first: {other:?}"),
    };
    assert_ne!(reloaded_first, reloaded_second);
    let back_to_first = match &s2.heap.object(reloaded_second).expect("obj second").data {
        Data::Cell {
            contents: Some(link_a),
            ..
        } => s2
            .heap
            .field(link_a, 0)
            .expect("first handle")
            .cell_id()
            .expect("back id"),
        other => panic!("unexpected cell second: {other:?}"),
    };
    assert_eq!(
        back_to_first, reloaded_first,
        "Ref cycle must close back on exact cell"
    );

    let call_prog = apply_global_program("Call", "Consumer", "add10", 32);
    let out = s2.eval_phrase(&call_prog, &Name::new("Call", "it"), None, false, None);
    assert_eq!(
        out,
        EvalOutcome::Value {
            rendered: "42".into(),
            stdout: String::new(),
        }
    );
    assert_eq!(s2.shutdown(), Ok(()));
}
