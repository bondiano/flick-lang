//! Preparation, unsupported-operation span, intra-block fusion, tail-call
//! frame reuse, and zero-leak integration tests for `flick_vm::tier1`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::ir::memory::{
    Block, Exit, Function, Global, HostPrimitive, HostServiceIntrinsic, Instruction, Intrinsic,
    MemorySpan, Name, Operation, Program, ValueRep,
};
use flick_core::ir::{Lit, Pat};
use flick_core::opt::perceus;
use flick_core::source::{FileId, SourceMap};
use flick_core::{build, project};
use flick_vm::tier0::{self, Error, Heap, HostMode, ProgramOutcome, Value};
use flick_vm::tier1::{self, PrepareError};

fn main_name() -> Name {
    Name::new("Test", "main")
}

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn single_op_program(ops: Vec<Instruction>, ret: u32) -> Program {
    let raw = Function {
        params: vec![],
        body: Block {
            instructions: ops,
            exit: Exit::Return(ret),
        },
    };
    Program {
        functions: BTreeMap::from([(0, perceus::insert(&raw).expect("valid ownership"))]),
        globals: vec![Global {
            name: main_name(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

struct MemHost<'a>(&'a str);

impl project::Host for MemHost<'_> {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        if p == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn lower_source(src: &str, entry: &Name) -> (Program, SourceMap) {
    let mut sources = SourceMap::default();
    let (diags, comp) = project::compile(
        &MemHost(src),
        &mut sources,
        None,
        &[PathBuf::from("/Main.fl")],
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "compile failed: {diags:?}"
    );
    let modules: Vec<_> = comp
        .modules
        .iter()
        .map(|m| (*m.ir).clone())
        .chain(build::builtin_modules().iter().cloned())
        .collect();
    let decls: Vec<_> = comp
        .modules
        .iter()
        .map(|m| m.declarations.as_ref())
        .chain(build::builtin_declarations().iter().map(AsRef::as_ref))
        .collect();
    let program = perceus::lower_roots_with_declarations(modules.iter(), decls, &[entry])
        .expect("perceus lower");
    (program, sources)
}

#[test]
fn unsupported_reachable_operations_report_name_and_source_span() {
    let source = "\
module App.Main exposing (main)

main : Unit ->{} Int
let main () =
  await (task { 42 })
";
    let entry = Name::new("App.Main", "main");
    let (program, sources) = lower_source(source, &entry);
    let err = tier1::prepare(&program, &entry).expect_err("Task must fail tier1 prepare");
    let PrepareError::Unsupported {
        operation,
        span: Some(MemorySpan { file, start, end }),
        ..
    } = err
    else {
        panic!("expected Unsupported with source span, got {err:?}");
    };
    assert_eq!(operation, "Task");
    let text = &sources.get(FileId::new(file)).text;
    let snippet = &text[start as usize..end as usize];
    assert!(
        snippet.contains("task") && snippet.contains("42"),
        "expected span to point at task expression, got {snippet:?}"
    );
}

#[test]
fn unreachable_unsupported_global_does_not_block_pure_entry() {
    let mut program = single_op_program(vec![let_op(0, Operation::Literal(Lit::Int(7)))], 0);
    let task_body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![let_op(2, Operation::Literal(Lit::Int(1)))],
            exit: Exit::Return(2),
        },
    })
    .expect("valid task body");
    let bad = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![let_op(
                0,
                Operation::Task {
                    code: 2,
                    captures: vec![],
                },
            )],
            exit: Exit::Return(0),
        },
    })
    .expect("valid bad fn");
    program.functions.insert(1, bad);
    program.functions.insert(2, task_body);
    program.globals.push(Global {
        name: Name::new("Test", "unused_task"),
        initializer: 1,
    });

    assert!(tier1::prepare(&program, &main_name()).is_ok());
    let err = tier1::prepare(&program, &Name::new("Test", "unused_task"))
        .expect_err("reachable Task must fail prepare");
    assert_eq!(err.operation(), Some("Task"));
}

#[test]
fn fusion_tail_call_frame_reuse_and_out_of_fuel_zero_leaks() {
    let source = "\
module App.Main exposing (main)

sum_loop : Int -> Int
let sum_loop n =
  let mut acc = 0
  let mut i = 0
  while i < n do
    acc = acc + i
    i = i + 1
  acc

tail_sum : Int -> Int -> Int
let rec tail_sum n acc =
  if n == 0 then acc else tail_sum (n - 1) (acc + n)

is_zero : Int -> Bool
let is_zero x =
  match x with
  | 0 -> true
  | _ -> false

main : Unit -> Int
let main () =
  let a = sum_loop 100
  let b = tail_sum 20000 0
  let c = if is_zero 0 then 1 else 0
  a + b + c + 1
";
    let entry = Name::new("App.Main", "main");
    let (program, _) = lower_source(source, &entry);
    let prepared = tier1::prepare(&program, &entry).expect("tier1 prepares");
    let stats = prepared.stats();
    assert!(
        stats.fused_compute > 0,
        "expected Compute fusion: {stats:?}"
    );
    assert!(
        stats.fused_compare_branch > 0,
        "expected CompareBranch fusion: {stats:?}"
    );
    assert!(
        stats.fused_update_cell >= 2,
        "expected UpdateCell fusion: {stats:?}"
    );
    assert!(
        stats.fused_match_flag > 0,
        "expected MatchFlag fusion: {stats:?}"
    );
    assert!(
        stats.fused_tail_calls > 0,
        "expected TailCall fusion: {stats:?}"
    );

    let mut heap = Heap::new();
    let exec = tier1::execute_main_program(&mut heap, &program, &entry, 2_000_000)
        .expect("prepare")
        .expect("execute");
    assert_eq!(exec.value, Value::Int(200_014_952));
    heap.drop_value(exec.value).expect("drop");
    heap.finish().expect("zero leaks");

    for fuel in [1, 5, 25, 100, 500] {
        let mut heap = Heap::new();
        let report =
            tier1::execute_main_program_outcome(&mut heap, &program, &entry, fuel).expect("prep");
        assert!(matches!(
            report.outcome,
            ProgramOutcome::Fault(Error::OutOfFuel)
        ));
        report.outcome.release(&mut heap).expect("release");
        heap.finish().expect("zero leaks on OutOfFuel");
    }
}

#[test]
fn flag_only_match_allocates_zero_heap_cells() {
    let func = perceus::insert(&Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                let_op(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(Pat::Lit(Lit::Int(42))),
                        rep: ValueRep::Cell,
                    },
                ),
                let_op(
                    2,
                    Operation::Project {
                        object: 1,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
            ],
            exit: Exit::Return(2),
        },
    })
    .expect("valid");
    let program = Program {
        functions: BTreeMap::from([
            (
                0,
                perceus::insert(&Function {
                    params: vec![],
                    body: Block {
                        instructions: vec![
                            let_op(0, Operation::Literal(Lit::Int(42))),
                            let_op(
                                1,
                                Operation::Call {
                                    code: 1,
                                    args: vec![0],
                                },
                            ),
                        ],
                        exit: Exit::Return(1),
                    },
                })
                .expect("valid entry"),
            ),
            (1, func),
        ]),
        globals: vec![Global {
            name: main_name(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let prepared = tier1::prepare(&program, &main_name()).expect("prepares");
    assert!(prepared.stats().fused_match_flag > 0);
    let mut heap = Heap::new();
    let exec = tier1::execute_program(&mut heap, &program, &main_name(), vec![], 1000)
        .expect("prep")
        .expect("exec");
    assert_eq!(exec.value, Value::Bool(true));
    assert_eq!(heap.stats().allocations, 0);
    heap.drop_value(exec.value).expect("drop");
    heap.finish().expect("zero leaks");
}

#[test]
fn tier1_executes_host_service_stderr_and_args_with_zero_leaks() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR").to_owned();
    let program = single_op_program(
        vec![
            let_op(0, Operation::Literal(Lit::String("t1-err".to_owned()))),
            let_op(
                1,
                Operation::Intrinsic {
                    kind: Intrinsic::Host(HostPrimitive::StderrLine),
                    args: vec![0],
                },
            ),
            let_op(2, Operation::Literal(Lit::Unit)),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::Host(HostPrimitive::Args),
                    args: vec![2],
                },
            ),
            let_op(
                4,
                Operation::Project {
                    object: 3,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![4],
                },
            ),
            let_op(6, Operation::Literal(Lit::String(manifest_dir))),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::HostService(HostServiceIntrinsic::FsExists),
                    args: vec![6],
                },
            ),
            let_op(
                8,
                Operation::Project {
                    object: 7,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                9,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![8],
                },
            ),
            let_op(
                10,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![9],
                },
            ),
        ],
        10,
    );
    let args = vec!["alpha".to_owned(), "beta".to_owned()];

    let mut heap0 = Heap::new();
    let r0 = tier0::execute_main_program_outcome_with_args(
        &mut heap0,
        &program,
        &main_name(),
        10_000,
        HostMode::LiveOs,
        args.clone(),
    );
    let mut heap1 = Heap::new();
    let r1 = tier1::execute_main_program_outcome_with_args(
        &mut heap1,
        &program,
        &main_name(),
        10_000,
        HostMode::LiveOs,
        args,
    )
    .expect("tier1 prepares host services");

    assert_eq!(r0.stdout, r1.stdout);
    assert_eq!(r0.stderr, r1.stderr);
    assert_eq!(r1.stderr, "t1-err\n");
    assert_eq!(r1.stdout, "alpha\ntrue\n");
    assert!(matches!(r0.outcome, ProgramOutcome::Returned(Value::Unit)));
    assert!(matches!(r1.outcome, ProgramOutcome::Returned(Value::Unit)));
    r0.outcome.release(&mut heap0).expect("release tier0");
    heap0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut heap1).expect("release tier1");
    heap1.finish().expect("zero leaks tier1");
}
