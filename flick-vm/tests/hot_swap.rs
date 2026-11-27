//! Integration tests for live VM hot swap (#990):
//! - two-version execution (suspended frame inside old `render` completes on old
//!   code; next call enters swapped `render`; pre-swap closures keep old code)
//!   on both Tier 0 and Tier 1;
//! - Tier 1 fused bytecode cache invalidation + `RegionInvalidator` hook;
//! - incompatible swap atomic refusal listing EVERY transitively dependent
//!   binding across multi-module chains;
//! - bytecode verification before committing any swap.

#[path = "hot_swap_support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::types::{Atom, Binder, Elem, Elided, Kind, Row, Type};

use flick_vm::session::{
    EvalOutcome, ExecutionTier, HandlerKind, HotSwapError, RegionInvalidator, StepEvalOutcome,
    SwapBinding, SwapDefinitionsRequest, VmSession,
};
use flick_vm::tier0::HostMode;
use support::{
    add_const_program, int_task_phrase, int_ty, named_row, pure_fun, render_handler_program,
    render_two_version_program, rt_fun, unit_ty,
};

struct RecordingInvalidator {
    calls: Arc<Mutex<Vec<Vec<Name>>>>,
}

impl RegionInvalidator for RecordingInvalidator {
    fn invalidate(&mut self, bindings: &[Name]) {
        self.calls
            .lock()
            .expect("invalidator log lock")
            .push(bindings.to_vec());
    }
}

fn run_two_version_render_scenario(tier: ExecutionTier) {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    session.set_tier(tier);

    let prog = render_two_version_program(5);
    assert_eq!(
        session.eval_declarations(&prog, "Ui", &[Name::new("Ui", "render")], None, None),
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );
    session.register_binding_scheme(Name::new("Ui", "render"), pure_fun(int_ty(), int_ty()));

    let task = session
        .start_step_existing(&Name::new("App", "driver"), tier)
        .expect("start driver task");
    let pause_steps = match tier {
        ExecutionTier::Tier0 => 7,
        ExecutionTier::Tier1 => 6,
    };
    assert!(matches!(
        session
            .step_eval(task, pause_steps)
            .expect("step into render"),
        StepEvalOutcome::Running { .. }
    ));

    let report = session
        .swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("Ui", "render", 100),
            vec![SwapBinding {
                name: Name::new("Ui", "render"),
                new_scheme: Some(pure_fun(int_ty(), int_ty())),
            }],
        ))
        .expect("compatible swap succeeds");
    assert!(report.bytecode_verified);
    assert_eq!(report.swapped_bindings, vec![Name::new("Ui", "render")]);

    assert_eq!(
        session.finish_step_eval(task).expect("finish driver"),
        EvalOutcome::Value {
            rendered: "(15, 110, 15)".to_owned(),
            stdout: String::new(),
        },
        "two-version execution failed on {tier:?}"
    );
    assert_eq!(session.shutdown(), Ok(()), "zero leaks on {tier:?}");
}

#[test]
fn suspended_frame_and_captured_closure_preserve_two_version_semantics_on_tier0_and_tier1() {
    run_two_version_render_scenario(ExecutionTier::Tier0);
    run_two_version_render_scenario(ExecutionTier::Tier1);
}

#[test]
#[allow(clippy::too_many_lines)]
fn tier1_fused_bytecode_invalidates_and_notifies_region_invalidator() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    session.set_tier(ExecutionTier::Tier1);

    let log = Arc::new(Mutex::new(Vec::new()));
    session.register_region_invalidator(Box::new(RecordingInvalidator {
        calls: Arc::clone(&log),
    }));

    let mut prog = add_const_program("A", "helper", 10);
    prog.functions.insert(
        2,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new("A", "helper")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(7)),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Apply {
                            function: 0,
                            argument: 1,
                        },
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Literal(Lit::Int(1)),
                    },
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 2,
                            right: 3,
                        },
                    },
                ],
                exit: Exit::Return(4),
            },
        },
    );
    prog.globals.push(Global {
        name: Name::new("B", "caller"),
        initializer: 2,
    });
    let caller = Name::new("B", "caller");
    assert_eq!(
        session.eval_phrase_on_tier(ExecutionTier::Tier1, &prog, &caller),
        EvalOutcome::Value {
            rendered: "18".to_owned(),
            stdout: String::new(),
        }
    );
    let stats = session
        .cached_tier1_stats(&caller)
        .expect("caller cached on tier 1");
    assert!(stats.fused_compute > 0);
    assert_eq!(session.tier1_cache_len(), 1);

    let report = session
        .swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("A", "helper", 50),
            vec![SwapBinding {
                name: Name::new("A", "helper"),
                new_scheme: None,
            }],
        ))
        .expect("swap A.helper");
    assert!(report.bytecode_verified);
    assert_eq!(session.tier1_cache_len(), 0);
    assert_eq!(
        report.invalidated_bindings,
        vec![Name::new("A", "helper"), Name::new("B", "caller")]
    );
    assert_eq!(
        *log.lock().expect("log lock"),
        vec![vec![Name::new("A", "helper"), Name::new("B", "caller")]]
    );

    let mut call_funcs = BTreeMap::new();
    call_funcs.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new("A", "helper")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(7)),
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
    let probe = Program {
        functions: call_funcs,
        globals: vec![Global {
            name: Name::new("Probe", "run"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let t1_res =
        session.eval_phrase_on_tier(ExecutionTier::Tier1, &probe, &Name::new("Probe", "run"));
    let t0_res = session.eval_existing_on_tier(ExecutionTier::Tier0, &Name::new("Probe", "run"));
    assert_eq!(t1_res, t0_res);
    assert_eq!(
        t1_res,
        EvalOutcome::Value {
            rendered: "57".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
#[allow(clippy::too_many_lines)]
fn incompatible_swap_is_refused_atomically_and_lists_all_transitive_dependents() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
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
                instructions: vec![Instruction::Drop(0)],
                exit: Exit::Return(1),
            },
        },
    );
    for (code, dep) in [(2, ("A", "leaf")), (3, ("B", "mid")), (4, ("A", "leaf"))] {
        functions.insert(
            code,
            Function {
                params: vec![],
                body: Block {
                    instructions: vec![Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new(dep.0, dep.1)),
                    }],
                    exit: Exit::Return(0),
                },
            },
        );
    }
    let prog = Program {
        functions,
        globals: vec![
            Global {
                name: Name::new("A", "leaf"),
                initializer: 0,
            },
            Global {
                name: Name::new("B", "mid"),
                initializer: 2,
            },
            Global {
                name: Name::new("C", "top"),
                initializer: 3,
            },
            Global {
                name: Name::new("D", "side"),
                initializer: 4,
            },
        ],
        op_spans: BTreeMap::new(),
    };
    let _ = session.eval_declarations(&prog, "A", &[Name::new("A", "leaf")], None, None);

    let poly_id = Type::Forall {
        vars: vec![Binder {
            name: "a".to_owned(),
            kind: Kind::Type,
        }],
        constraints: Vec::new(),
        body: Box::new(pure_fun(
            Type::Var("a".to_owned()),
            Type::Var("a".to_owned()),
        )),
    };
    session.register_binding_scheme(Name::new("A", "leaf"), poly_id);
    let bytes_before = session.heap_live_bytes();

    // Narrowing polymorphic -> monomorphic rejected with full transitive dependents list.
    let err = session
        .swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("A", "leaf", 99),
            vec![SwapBinding {
                name: Name::new("A", "leaf"),
                new_scheme: Some(pure_fun(int_ty(), int_ty())),
            }],
        ))
        .expect_err("narrowing scheme must fail");
    match err {
        HotSwapError::Incompatible {
            target, dependents, ..
        } => {
            assert_eq!(target, Name::new("A", "leaf"));
            assert_eq!(
                dependents,
                vec![
                    Name::new("B", "mid"),
                    Name::new("C", "top"),
                    Name::new("D", "side"),
                ]
            );
        }
        other => panic!("expected HotSwapError::Incompatible, got {other:?}"),
    }

    // Widening effect row rejected.
    session.register_binding_scheme(Name::new("A", "leaf"), pure_fun(int_ty(), int_ty()));
    assert!(matches!(
        session.swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("A", "leaf", 5),
            vec![SwapBinding {
                name: Name::new("A", "leaf"),
                new_scheme: Some(rt_fun(
                    int_ty(),
                    named_row("Demo", "Log"),
                    Type::empty_row(),
                    int_ty(),
                )),
            }],
        )),
        Err(HotSwapError::Incompatible { .. })
    ));

    // Narrowing contravariant parameter capture qualifier rejected.
    let thunk = |caps: Option<Type>| {
        let arrow = rt_fun(unit_ty(), Type::empty_row(), Type::empty_row(), unit_ty());
        match caps {
            Some(c) => Type::Captured {
                ty: Box::new(arrow),
                captures: Box::new(c),
                value: false,
                elided: Elided::default(),
            },
            None => arrow,
        }
    };
    let region_s = Type::Row(Row {
        entries: vec![(
            Elem::Capture(Atom::Var("s".to_owned())),
            Type::Var("s".to_owned()),
        )],
        tail: None,
        ..Row::default()
    });
    let param_s = Type::Forall {
        vars: vec![Binder {
            name: "s".to_owned(),
            kind: Kind::Region,
        }],
        constraints: Vec::new(),
        body: Box::new(rt_fun(
            thunk(Some(region_s)),
            Type::empty_row(),
            Type::empty_row(),
            unit_ty(),
        )),
    };
    session.register_binding_scheme(Name::new("A", "leaf"), param_s);
    assert!(matches!(
        session.swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("A", "leaf", 5),
            vec![SwapBinding {
                name: Name::new("A", "leaf"),
                new_scheme: Some(rt_fun(
                    thunk(None),
                    Type::empty_row(),
                    Type::empty_row(),
                    unit_ty(),
                )),
            }],
        )),
        Err(HotSwapError::Incompatible { .. })
    ));

    assert_eq!(session.heap_live_bytes(), bytes_before);
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
fn invalid_replacement_ir_is_rejected_before_commit() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let base = add_const_program("M", "f", 1);
    let _ = session.eval_declarations(&base, "M", &[Name::new("M", "f")], None, None);

    // Invalid IR: references an undefined CodeId 99.
    let mut bad_funcs = BTreeMap::new();
    bad_funcs.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 99,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    let bad = Program {
        functions: bad_funcs,
        globals: vec![Global {
            name: Name::new("M", "f"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let err = session
        .swap_definitions(SwapDefinitionsRequest::new(
            bad,
            vec![SwapBinding {
                name: Name::new("M", "f"),
                new_scheme: None,
            }],
        ))
        .expect_err("invalid IR must be refused");
    assert!(matches!(err, HotSwapError::InvalidIr(_)));
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
fn installed_handler_calling_render_picks_up_swapped_render_on_next_call() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let render_v1 = add_const_program("Ui", "render", 10);
    assert_eq!(
        session.eval_declarations(&render_v1, "Ui", &[Name::new("Ui", "render")], None, None),
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );

    let handler_prog = render_handler_program();
    assert_eq!(
        session.install_handler(
            &handler_prog,
            &Name::new("Repl.H1", "h"),
            None,
            HandlerKind::Thunk {
                returns_task: false
            },
            vec!["Render".to_owned()],
            None,
        ),
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );

    let p1 = int_task_phrase("Repl.P1", 7);
    assert_eq!(
        session.eval_phrase(&p1, &Name::new("Repl.P1", "it"), None, true, None),
        EvalOutcome::Value {
            rendered: "17".to_owned(),
            stdout: String::new(),
        }
    );

    session
        .swap_definitions(SwapDefinitionsRequest::new(
            add_const_program("Ui", "render", 200),
            vec![SwapBinding {
                name: Name::new("Ui", "render"),
                new_scheme: None,
            }],
        ))
        .expect("swap render");

    let p2 = int_task_phrase("Repl.P2", 7);
    assert_eq!(
        session.eval_phrase(&p2, &Name::new("Repl.P2", "it"), None, true, None),
        EvalOutcome::Value {
            rendered: "207".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(session.shutdown(), Ok(()));
}
