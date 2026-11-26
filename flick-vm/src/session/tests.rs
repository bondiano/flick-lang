//! Unit tests for [`VmSession`]: persistent heap/globals, monotonic `CodeId`
//! remapping, external interruption with scope finalization, and background jobs.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};

use super::{EvalOutcome, JobStatus, VmSession};
use crate::tier0::HostMode;

fn int_program(module: &str, name: &str, value: i32) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Int(value)),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, name),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

fn finalizer_fn(message: &str) -> Function {
    Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Drop(1),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Literal(Lit::String(message.to_owned())),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::PrintLine,
                        args: vec![2],
                    },
                },
            ],
            exit: Exit::Return(3),
        },
    }
}

fn spin_loop_fn() -> Function {
    Function {
        params: vec![],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 0,
                value: Operation::Literal(Lit::Unit),
            }],
            exit: Exit::Return(0),
        },
    }
}

fn looping_task_with_finalizer_program(module: &str, name: &str, message: &str) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Task {
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
                    Instruction::Drop(1),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::ScopeRun,
                            args: vec![2],
                        },
                    },
                ],
                exit: Exit::Return(3),
            },
        },
    );
    functions.insert(
        2,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Drop(1),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Task {
                            code: 3,
                            captures: vec![],
                        },
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::ScopeAddFinalizer,
                            args: vec![2],
                        },
                    },
                    Instruction::Drop(3),
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Loop {
                            id: "spin".to_owned(),
                            code: 4,
                            args: vec![],
                        },
                    },
                ],
                exit: Exit::Return(4),
            },
        },
    );
    functions.insert(3, finalizer_fn(message));
    functions.insert(4, spin_loop_fn());
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, name),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

#[test]
fn persistent_globals_are_not_reevaluated_and_shutdown_is_leak_free() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let p1 = int_program("Repl.P1", "n", 41);
    let decl = session.eval_declarations(&p1, "Repl.P1", &[Name::new("Repl.P1", "n")], None, None);
    assert_eq!(
        decl,
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );

    // Second phrase references Repl.P1.n with its own CodeId 0 (remapped cleanly).
    let mut funcs = BTreeMap::new();
    funcs.insert(
        0 as CodeId,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new("Repl.P1", "n")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(1)),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 0,
                            right: 1,
                        },
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    let p2 = Program {
        functions: funcs,
        globals: vec![Global {
            name: Name::new("Repl.P2", "it"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let out = session.eval_phrase(&p2, &Name::new("Repl.P2", "it"), None, false, None);
    assert_eq!(
        out,
        EvalOutcome::Value {
            rendered: "42".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
fn external_interrupt_runs_scope_finalizer_once_and_preserves_earlier_bindings() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let p1 = int_program("Repl.P1", "n", 41);
    let _ = session.eval_declarations(&p1, "Repl.P1", &[Name::new("Repl.P1", "n")], None, None);

    let loop_prog = looping_task_with_finalizer_program("Repl.P2", "it", "finalizer ran");
    let flag = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&flag);
    let waiter = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        trigger.store(true, Ordering::Release);
    });
    let interrupted = session.eval_phrase(
        &loop_prog,
        &Name::new("Repl.P2", "it"),
        None,
        true,
        Some(flag),
    );
    waiter.join().expect("trigger thread");
    assert_eq!(
        interrupted,
        EvalOutcome::Interrupted {
            stdout: "finalizer ran\n".to_owned(),
        }
    );

    // Earlier binding Repl.P1.n is still alive and usable after interruption.
    let p3 = int_program("Repl.P3", "it", 99);
    let after = session.eval_phrase(&p3, &Name::new("Repl.P3", "it"), None, false, None);
    assert_eq!(
        after,
        EvalOutcome::Value {
            rendered: "99".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
fn background_job_lists_as_running_and_kill_runs_finalizer_without_leaks() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let bg_prog = looping_task_with_finalizer_program("Repl.P1", "__repl_bg", "bg cleaned");
    let id = session
        .spawn_bg(
            &bg_prog,
            &Name::new("Repl.P1", "__repl_bg"),
            None,
            "spin".to_owned(),
        )
        .expect("spawn bg job");
    assert_eq!(id, 1);

    let jobs = session.list_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, 1);
    assert_eq!(jobs[0].status, JobStatus::Running);

    thread::sleep(Duration::from_millis(25));
    let killed = session.kill_job(1).expect("kill job 1");
    assert_eq!(killed.status, JobStatus::Killed);
    assert_eq!(killed.stdout, "bg cleaned\n");
    assert_eq!(killed.leak_error, None);
    assert_eq!(session.shutdown(), Ok(()));
}
