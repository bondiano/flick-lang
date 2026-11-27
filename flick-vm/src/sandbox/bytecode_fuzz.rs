//! Deterministic corpus builders and mutation harnesses for fuzzing
//! [`super::verify_bytecode`] from integration tests.

use std::collections::BTreeMap;

use flick_core::ir::BinOp;
use flick_core::ir::memory::{CliIntrinsic, HostServiceIntrinsic, Intrinsic, Name, ValueRep};
use flick_core::numeric::{Arith, Num};
use flick_core::types::OpClass;

use super::verify_bytecode::{BytecodeVerifyError, verify_bytecode};
use crate::tier1::opcode::{
    CompiledFunction, CompiledHandlerOp, Immediate, Instr, MAX_TAIL_ARGS, Prepared, PreparedOp,
    ValueSource,
};

fn minimal_valid_prepared() -> Prepared {
    Prepared {
        functions: vec![CompiledFunction {
            params: vec![],
            slots: 2,
            instructions: vec![
                Instr::Literal {
                    dst: 0,
                    value: Immediate::Int(42),
                },
                Instr::Return { value: 0 },
            ],
            spans: vec![None, None],
        }],
        ir_codes: vec![0],
        code_indices: BTreeMap::from([(0, 0)]),
        globals: vec![0],
        global_names: vec![Name::new("Test", "main")],
        entry: 0,
        root: None,
        tags: vec!["Box".into()],
        dictionary_fields: vec!["show".into()],
        loops: vec!["L0".into()],
        operations: vec![PreparedOp {
            effect: "Eff".into(),
            op: "op".into(),
            console_fallback: None,
        }],
    }
}

fn with_first_instr(instr: Instr) -> Prepared {
    let mut p = minimal_valid_prepared();
    p.functions[0].instructions[0] = instr;
    p
}

/// Returns a deterministic corpus of malformed [`Prepared`] bytecode images,
/// each violating a specific verifier invariant.
#[must_use]
pub fn malformed_bytecode_corpus() -> Vec<(&'static str, Prepared)> {
    let mut cases = Vec::new();
    push_structural_corpus(&mut cases);
    push_table_corpus(&mut cases);
    push_handler_and_record_corpus(&mut cases);
    cases
}

fn push_structural_corpus(cases: &mut Vec<(&'static str, Prepared)>) {
    let mut p = minimal_valid_prepared();
    p.functions.clear();
    cases.push(("empty_functions", p));

    let mut p = minimal_valid_prepared();
    p.entry = 99;
    cases.push(("oob_entry_global", p));

    let mut p = minimal_valid_prepared();
    p.globals[0] = 5;
    cases.push(("oob_global_init", p));

    let mut p = minimal_valid_prepared();
    p.functions[0].params = vec![0];
    cases.push(("global_init_with_params", p));

    cases.push((
        "oob_dst_slot",
        with_first_instr(Instr::Literal {
            dst: 99,
            value: Immediate::Int(1),
        }),
    ));
    cases.push((
        "oob_var_slot",
        with_first_instr(Instr::UpdateCell {
            var: 50,
            op: BinOp::Arith(Arith::Add, Num::Int),
            right: ValueSource::Immediate(Immediate::Int(1)),
        }),
    ));

    let mut p = minimal_valid_prepared();
    p.functions[0].instructions = vec![
        Instr::Literal {
            dst: 0,
            value: Immediate::Bool(true),
        },
        Instr::Branch {
            condition: 0,
            then_pc: 0,
            else_pc: 99,
        },
    ];
    cases.push(("oob_branch_pc", p));

    let mut p = minimal_valid_prepared();
    p.functions[0].instructions.pop();
    p.functions[0].spans.pop();
    cases.push(("missing_terminator", p));

    let mut p = minimal_valid_prepared();
    p.functions[0].spans.pop();
    cases.push(("mismatched_spans_len", p));

    cases.push((
        "oob_call_code",
        with_first_instr(Instr::Call {
            dst: 0,
            code: 7,
            args: vec![],
        }),
    ));
    cases.push((
        "call_arity_mismatch",
        with_first_instr(Instr::Call {
            dst: 0,
            code: 0,
            args: vec![0],
        }),
    ));

    let mut p = minimal_valid_prepared();
    p.functions.push(CompiledFunction {
        params: (0..=MAX_TAIL_ARGS).collect(),
        slots: MAX_TAIL_ARGS + 2,
        instructions: vec![Instr::ReturnUnit],
        spans: vec![None],
    });
    p.ir_codes.push(1);
    p.code_indices.insert(1, 1);
    p.functions[0].slots = MAX_TAIL_ARGS + 2;
    p.functions[0].instructions[0] = Instr::TailCall {
        dst: 0,
        code: 1,
        args: (0..=MAX_TAIL_ARGS).collect(),
    };
    cases.push(("tail_call_too_many_args", p));
}

fn push_table_corpus(cases: &mut Vec<(&'static str, Prepared)>) {
    cases.push((
        "oob_tag_index",
        with_first_instr(Instr::Construct {
            dst: 0,
            tag: 42,
            fields: vec![],
            dicts: vec![],
            rep: ValueRep::Cell,
            layout: None,
        }),
    ));
    cases.push((
        "oob_dict_field",
        with_first_instr(Instr::Method {
            dst: 0,
            dictionary: 0,
            field: 9,
        }),
    ));
    cases.push((
        "oob_loop_id",
        with_first_instr(Instr::Loop {
            dst: 0,
            id: 12,
            code: 0,
            args: vec![],
        }),
    ));
    cases.push((
        "oob_global_index",
        with_first_instr(Instr::Global { dst: 0, global: 4 }),
    ));
    cases.push((
        "oob_perform_op_id",
        with_first_instr(Instr::Perform {
            dst: 0,
            op_id: 99,
            dicts: vec![],
            args: vec![],
        }),
    ));
}

fn push_handler_and_record_corpus(cases: &mut Vec<(&'static str, Prepared)>) {
    cases.push((
        "duplicate_handler_op",
        with_first_instr(Instr::Handle {
            dst: 0,
            body: 0,
            operations: vec![
                CompiledHandlerOp {
                    op_id: 0,
                    class: OpClass::Tail,
                    dicts: 0,
                    arity: 1,
                    function: 0,
                },
                CompiledHandlerOp {
                    op_id: 0,
                    class: OpClass::Abort,
                    dicts: 0,
                    arity: 1,
                    function: 1,
                },
            ],
            ret: None,
            errors: None,
        }),
    ));
    cases.push((
        "oob_handler_return_slot",
        with_first_instr(Instr::Handle {
            dst: 0,
            body: 0,
            operations: vec![],
            ret: Some(55),
            errors: None,
        }),
    ));

    let mut p = minimal_valid_prepared();
    p.operations[0].console_fallback = Some(Intrinsic::Raise);
    cases.push(("non_console_fallback", p));

    cases.push((
        "host_service_arity_mismatch",
        with_first_instr(Instr::HostService {
            dst: 0,
            op: HostServiceIntrinsic::FsReadFile,
            args: vec![0, 1],
        }),
    ));
    cases.push((
        "cli_arity_mismatch",
        with_first_instr(Instr::Cli {
            dst: 0,
            op: CliIntrinsic::RunVersion,
            args: vec![0],
        }),
    ));
    cases.push((
        "closure_bad_param_count",
        with_first_instr(Instr::Closure {
            dst: 0,
            code: 0,
            captures: vec![],
        }),
    ));
    cases.push((
        "duplicate_record_field",
        with_first_instr(Instr::Record {
            dst: 0,
            fields: vec![("x".into(), 0), ("x".into(), 1)],
        }),
    ));
    cases.push((
        "update_labels_values_mismatch",
        with_first_instr(Instr::Update {
            dst: 0,
            object: 0,
            labels: vec!["a".into(), "b".into()],
            values: vec![0],
        }),
    ));

    let mut p = minimal_valid_prepared();
    p.code_indices.insert(999, 0);
    cases.push(("corrupt_code_indices", p));
}

/// Applies deterministic pseudo-random structural mutations derived from `seed`
/// to a clone of `base` and runs [`verify_bytecode`]. Used by `fuzz_vm.rs`.
pub fn fuzz_mutate_and_verify_bytecode(
    base: &Prepared,
    mut rng_state: u64,
) -> Result<(), BytecodeVerifyError> {
    let mut next = || -> u64 {
        rng_state ^= rng_state >> 12;
        rng_state ^= rng_state << 25;
        rng_state ^= rng_state >> 27;
        rng_state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    let mut mutated = clone_prepared(base);
    let mutations = 1 + usize::try_from(next() % 4).unwrap_or(0);
    for _ in 0..mutations {
        let pick = next() % 19;
        let wild = usize::try_from(next() % 512).unwrap_or(0);
        apply_bytecode_mutation(&mut mutated, pick, wild);
    }
    verify_bytecode(&mutated)
}

fn apply_bytecode_mutation(mutated: &mut Prepared, pick: u64, wild: usize) {
    let fn_len = mutated.functions.len();
    match pick {
        0 => mutated.entry = wild,
        1 if !mutated.globals.is_empty() => {
            let i = wild % mutated.globals.len();
            mutated.globals[i] = wild;
        }
        2 => {
            mutated.ir_codes.pop();
        }
        3 => {
            mutated
                .code_indices
                .insert(u32::try_from(wild).unwrap_or(0), wild);
        }
        4 if fn_len > 0 => mutated.functions[wild % fn_len].slots = wild % 4,
        5 if fn_len > 0 => mutated.functions[wild % fn_len].params.push(wild),
        6 if fn_len > 0 => {
            mutated.functions[wild % fn_len].spans.pop();
        }
        7 if fn_len > 0 => {
            let f = &mut mutated.functions[wild % fn_len];
            f.instructions.pop();
            f.spans.pop();
        }
        8..=17 if fn_len > 0 => {
            let f = &mut mutated.functions[wild % fn_len];
            let ilen = f.instructions.len();
            if ilen > 0 {
                f.instructions[wild % ilen] = mutated_instr(pick, wild);
            }
        }
        _ => {
            mutated.tags.clear();
            mutated.loops.clear();
            mutated.dictionary_fields.clear();
        }
    }
}

fn mutated_instr(pick: u64, wild: usize) -> Instr {
    match pick {
        8 => Instr::Branch {
            condition: wild,
            then_pc: wild,
            else_pc: wild.wrapping_add(1),
        },
        9 => Instr::Call {
            dst: wild % 8,
            code: wild % 16,
            args: vec![wild % 8; wild % 5],
        },
        10 => Instr::TailCall {
            dst: wild % 4,
            code: wild % 8,
            args: vec![0; (wild % 12) + 1],
        },
        11 => Instr::Construct {
            dst: wild % 4,
            tag: wild % 16,
            fields: vec![wild % 8],
            dicts: vec![],
            rep: ValueRep::Cell,
            layout: None,
        },
        12 => Instr::Handle {
            dst: wild % 4,
            body: wild % 4,
            operations: vec![CompiledHandlerOp {
                op_id: wild % 8,
                class: OpClass::Control,
                dicts: wild,
                arity: usize::MAX,
                function: wild % 8,
            }],
            ret: Some(wild % 8),
            errors: Some(wild % 8),
        },
        13 => Instr::Perform {
            dst: wild % 4,
            op_id: wild % 16,
            dicts: vec![wild % 8],
            args: vec![wild % 8],
        },
        14 => Instr::Global {
            dst: wild % 4,
            global: wild % 16,
        },
        15 => Instr::Loop {
            dst: wild % 4,
            id: wild % 16,
            code: wild % 8,
            args: vec![wild % 4],
        },
        16 => Instr::Cli {
            dst: wild % 4,
            op: CliIntrinsic::RunVersion,
            args: vec![wild % 8; wild % 5],
        },
        _ => Instr::UpdateCell {
            var: wild % 32,
            op: BinOp::Arith(Arith::Add, Num::Int),
            right: ValueSource::Cell(wild % 32),
        },
    }
}

fn clone_prepared(src: &Prepared) -> Prepared {
    Prepared {
        functions: src
            .functions
            .iter()
            .map(|f| CompiledFunction {
                params: f.params.clone(),
                slots: f.slots,
                instructions: f.instructions.clone(),
                spans: f.spans.clone(),
            })
            .collect(),
        ir_codes: src.ir_codes.clone(),
        code_indices: src.code_indices.clone(),
        globals: src.globals.clone(),
        global_names: src.global_names.clone(),
        entry: src.entry,
        root: src.root,
        tags: src.tags.clone(),
        dictionary_fields: src.dictionary_fields.clone(),
        loops: src.loops.clone(),
        operations: src.operations.clone(),
    }
}
