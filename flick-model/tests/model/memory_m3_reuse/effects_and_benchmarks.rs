//! Early failure/handler/cancellation FBIP safety and reuse-token escape isolation tests.

use flick_core::ir::{
    Lit, Pat,
    memory::{Block, Exit, Function, Global, Instruction, Name, Operation, Program},
};
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute, execute_program_outcome};

use super::{compare_fbip_vs_baseline, sweep_fuel_prefixes};

const EARLY_FAILURE_HANDLERS_CANCEL_SOURCE: &str = r#"
module App.Main exposing (main)

type IntList = Nil | Cons Int IntList

type error BadItem = BadItem Int

effect Abort =
  abort bail : forall a. String ->{} a

effect Step =
  control step : Int ->{} Int

build : Int -> IntList ->{} IntList
let rec build n acc =
  if n <= 0 then acc else build (n - 1) (Cons n acc)

map_fallible : IntList ->{; BadItem} IntList
let rec map_fallible xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h == 3 then Fail.fail (BadItem h)
      else Cons (h * 10) (map_fallible t)

map_defect : IntList ->{} IntList
let rec map_defect xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h == 2 then Cons (Int.unsafe_div 1 0) (map_defect t)
      else Cons (h + 1) (map_defect t)

map_abort : IntList ->{Abort} IntList
let rec map_abort xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h == 3 then Abort.bail "stopped-at-3"
      else Cons (h + 100) (map_abort t)

map_control : IntList ->{Step} IntList
let rec map_control xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      let v = Step.step h
      Cons v (map_control t)

sum : IntList -> Int ->{} Int
let rec sum xs acc =
  match xs with
  | Nil -> acc
  | Cons h t -> sum t (acc + h)

nap : Float ->{Async} Unit
let nap ms =
  match Duration.millis ms with
  | Ok d -> Effect.sleep d
  | Err _ -> ()

main : Unit ->{Host.Console, Async} Unit
let main () =
  let r_err =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-err-1" })
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-err-2" })
        let _ = map_fallible (build 5 Nil)
        "unreachable"
      }
    with
    | BadItem n -> "err:" ++ show n
  Host.raw_stdout_line r_err

  let ex_die : Exit none IntList =
    Effect.exit
      (task {
        scope {
          Scope.add_finalizer (task { Host.raw_stdout_line "fin-die-1" })
          map_defect (build 4 Nil)
        }
      })
  Host.raw_stdout_line ("die:" ++ show (Exit.has_dies ex_die))

  let r_abort =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-abort-1" })
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-abort-2" })
        let _ = map_abort (build 5 Nil)
        "unreachable"
      }
    with
    | Abort.bail msg -> "aborted:" ++ msg
  Host.raw_stdout_line r_abort

  let r_ctrl_ok =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-ctrl-ok" })
        let ys = map_control (build 4 Nil)
        "ctrl-sum:" ++ show (sum ys 0)
      }
    with
    | Step.step x -> resume (x * 2)
  Host.raw_stdout_line r_ctrl_ok

  let r_ctrl_disc =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-ctrl-disc-1" })
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-ctrl-disc-2" })
        let _ = map_control (build 4 Nil)
        "unreachable"
      }
    with
    | Step.step x -> if x == 2 then "disc:" ++ show x else resume x
  Host.raw_stdout_line r_ctrl_disc

  let ex_twice : Exit none IntList =
    Effect.exit
      (task {
        try
          scope {
            Scope.add_finalizer (task { Host.raw_stdout_line "fin-twice" })
            map_control (build 3 Nil)
          }
        with
        | Step.step x ->
            let _ = resume x
            resume x
      })
  Host.raw_stdout_line ("twice:" ++ show (Exit.has_dies ex_twice))

  let r_cancel =
    try
      let _ =
        timeout
          5
          (task {
            scope {
              Scope.add_finalizer (task { Host.raw_stdout_line "fin-cancel-1" })
              Scope.add_finalizer (task { Host.raw_stdout_line "fin-cancel-2" })
              let xs = build 3 Nil
              match xs with
              | Cons h t ->
                  nap 5000
                  Cons (h + 1) t
              | Nil -> Nil
            }
          })
      "unreachable"
    with
    | TimeoutError -> "cancelled"
  Host.raw_stdout_line r_cancel
"#;

#[test]
fn early_failure_defects_handlers_cancellation_and_lifo_finalizer_order_under_fbip() {
    let expected = "\
fin-err-2
fin-err-1
err:3
fin-die-1
die:true
fin-abort-2
fin-abort-1
aborted:stopped-at-3
fin-ctrl-ok
ctrl-sum:20
fin-ctrl-disc-2
fin-ctrl-disc-1
disc:2
fin-twice
twice:true
fin-cancel-2
fin-cancel-1
cancelled
";
    let (dbg, rel) = compare_fbip_vs_baseline(
        "early_failure_handlers_cancel",
        EARLY_FAILURE_HANDLERS_CANCEL_SOURCE,
        expected,
    );
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 4,
            "{profile}: expected >= 4 reuses on successful control-resume path, got {}",
            cmp.fbip.reuses
        );
    }
    sweep_fuel_prefixes(
        "early_failure_handlers_cancel",
        EARLY_FAILURE_HANDLERS_CANCEL_SOURCE,
        expected,
        2000,
    );
}

fn escape_cases() -> Vec<(&'static str, Function)> {
    let bind = |binding: u32, value: Operation| Instruction::Let { binding, value };
    vec![
        (
            "return_reuse_token",
            Function {
                params: vec![0, 1],
                body: Block {
                    instructions: vec![Instruction::Drop(0), bind(2, Operation::DropReuse(1))],
                    exit: Exit::Return(2),
                },
            },
        ),
        (
            "store_reuse_token_in_construct",
            Function {
                params: vec![0, 1],
                body: Block {
                    instructions: vec![
                        Instruction::Drop(0),
                        bind(2, Operation::DropReuse(1)),
                        bind(
                            3,
                            Operation::Construct {
                                tag: "M.Box.Box".into(),
                                fields: vec![2],
                            },
                        ),
                    ],
                    exit: Exit::Return(3),
                },
            },
        ),
        (
            "match_on_reuse_token",
            Function {
                params: vec![0, 1],
                body: Block {
                    instructions: vec![
                        Instruction::Drop(0),
                        bind(2, Operation::DropReuse(1)),
                        bind(
                            3,
                            Operation::Match {
                                object: 2,
                                pattern: Box::new(Pat::Wild),
                            },
                        ),
                        Instruction::Drop(2),
                    ],
                    exit: Exit::Return(3),
                },
            },
        ),
    ]
}

#[test]
fn reuse_token_escape_is_rejected_without_leaks_across_all_fuel_prefixes() {
    for (label, body) in escape_cases() {
        let init = Function {
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
        };
        let name = Name::new("Escape", "main");
        let program = Program {
            functions: [(0, init), (1, body)].into_iter().collect(),
            globals: vec![Global {
                name: name.clone(),
                initializer: 0,
            }],
        };
        for shared in [false, true] {
            let mut saw_escape = false;
            for fuel in 0..=200 {
                let mut heap = Heap::new();
                let child = heap.literal(&Lit::String("inner".into())).expect("child");
                let input = heap.construct("M.Box.Box", vec![child]).expect("box");
                let alias = shared.then(|| heap.dup(&input).expect("alias"));
                let report = execute_program_outcome(&mut heap, &program, &name, vec![input], fuel);
                match &report.outcome {
                    ProgramOutcome::Fault(Error::ReuseTokenEscape) => saw_escape = true,
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => panic!("{label} shared={shared} fuel={fuel}: unexpected {other:?}"),
                }
                report.outcome.release(&mut heap).expect("release");
                if let Some(a) = alias {
                    assert_eq!(heap.tag(&a), Ok("M.Box.Box"));
                    assert_eq!(heap.string(heap.field(&a, 0).expect("inner")), Ok("inner"));
                    heap.drop_value(a).expect("drop alias");
                }
                let stats = heap.finish().expect("zero leaks on ReuseTokenEscape");
                assert_eq!(stats.allocations, stats.frees);
                if saw_escape {
                    break;
                }
            }
            assert!(
                saw_escape,
                "{label} shared={shared}: never reached ReuseTokenEscape"
            );
        }
    }
}

fn standalone_escape_cases() -> Vec<(&'static str, Function)> {
    let bind = |binding: u32, value: Operation| Instruction::Let { binding, value };
    vec![
        (
            "standalone_return_reuse_token",
            Function {
                params: vec![0],
                body: Block {
                    instructions: vec![bind(1, Operation::DropReuse(0))],
                    exit: Exit::Return(1),
                },
            },
        ),
        (
            "standalone_dup_reuse_token",
            Function {
                params: vec![0],
                body: Block {
                    instructions: vec![
                        bind(1, Operation::DropReuse(0)),
                        Instruction::Dup(1),
                        Instruction::Drop(1),
                        Instruction::Drop(1),
                        bind(2, Operation::Literal(Lit::Unit)),
                    ],
                    exit: Exit::Return(2),
                },
            },
        ),
        (
            "standalone_branch_on_reuse_token",
            Function {
                params: vec![0],
                body: Block {
                    instructions: vec![bind(1, Operation::DropReuse(0))],
                    exit: Exit::Branch {
                        condition: 1,
                        then_: Box::new(Block {
                            instructions: vec![bind(2, Operation::Literal(Lit::Unit))],
                            exit: Exit::Return(2),
                        }),
                        else_: Box::new(Block {
                            instructions: vec![bind(2, Operation::Literal(Lit::Unit))],
                            exit: Exit::Return(2),
                        }),
                    },
                },
            },
        ),
        (
            "standalone_construct_with_reuse_token",
            Function {
                params: vec![0],
                body: Block {
                    instructions: vec![
                        bind(1, Operation::DropReuse(0)),
                        bind(
                            2,
                            Operation::Construct {
                                tag: "M.Box.Box".into(),
                                fields: vec![1],
                            },
                        ),
                    ],
                    exit: Exit::Return(2),
                },
            },
        ),
    ]
}

#[test]
fn standalone_execute_and_heap_dup_reject_reuse_token_escape() {
    // 1. Direct `Heap::dup` rejects both live `Data::ReuseToken` and `Value::ReuseNull`.
    for shared in [false, true] {
        let mut heap = Heap::new();
        let child = heap.literal(&Lit::String("inner".into())).expect("child");
        let input = heap.construct("M.Box.Box", vec![child]).expect("box");
        let alias = shared.then(|| heap.dup(&input).expect("alias"));
        let token = heap.drop_reuse(input).expect("drop_reuse");
        let dups_before = heap.stats().dups;
        assert_eq!(heap.dup(&token), Err(Error::ReuseTokenEscape));
        assert_eq!(heap.stats().dups, dups_before);
        // Standalone entry arguments also reject both `Data::ReuseToken` and `Value::ReuseNull`.
        let identity = Function {
            params: vec![0],
            body: Block {
                instructions: vec![],
                exit: Exit::Return(0),
            },
        };
        assert_eq!(
            execute(&mut heap, &identity, vec![Value::ReuseNull]),
            Err(Error::ReuseTokenEscape)
        );
        let token_id = token.object_id();
        assert_eq!(
            execute(&mut heap, &identity, vec![token]),
            Err(Error::ReuseTokenEscape)
        );
        if let Some(a) = alias {
            assert_eq!(heap.string(heap.field(&a, 0).expect("inner")), Ok("inner"));
            heap.drop_value(a).expect("drop alias");
        }
        // Standalone `execute` deliberately does not sweep rejected heap arguments;
        // verify the unique token stayed untouched with rc == 1 and zero dups.
        assert_eq!(heap.stats().live_objects, usize::from(token_id.is_some()));
    }

    // 2. Standalone `Return`, `Dup`, `Branch`, and `Construct` reject reuse tokens.
    for (label, func) in standalone_escape_cases() {
        for shared in [false, true] {
            let mut heap = Heap::new();
            let child = heap.literal(&Lit::String("inner".into())).expect("child");
            let input = heap.construct("M.Box.Box", vec![child]).expect("box");
            let alias = shared.then(|| heap.dup(&input).expect("alias"));
            assert_eq!(
                execute(&mut heap, &func, vec![input]),
                Err(Error::ReuseTokenEscape),
                "{label} shared={shared}"
            );
            if let Some(a) = alias {
                assert_eq!(heap.string(heap.field(&a, 0).expect("inner")), Ok("inner"));
                heap.drop_value(a).expect("drop alias");
            }
        }
    }

    // 3. Valid `DropReuse` -> `ReuseRecord` succeeds in place under standalone `execute`.
    let bind = |binding: u32, value: Operation| Instruction::Let { binding, value };
    let valid = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(1, Operation::DropReuse(0)),
                bind(2, Operation::Literal(Lit::Int(42))),
                bind(
                    3,
                    Operation::ReuseRecord {
                        object: 1,
                        fields: vec![("x".into(), 2)],
                    },
                ),
            ],
            exit: Exit::Return(3),
        },
    };
    let mut heap = Heap::new();
    let rec = heap.record(vec![("x".into(), Value::Int(1))]).expect("rec");
    let rec_id = rec.object_id();
    let out = execute(&mut heap, &valid, vec![rec]).expect("valid standalone reuse");
    assert_eq!(out.object_id(), rec_id);
    assert_eq!(heap.named_field(&out, "x"), Ok(&Value::Int(42)));
    assert_eq!(heap.stats().reuses, 1);
    heap.drop_value(out).expect("drop out");
    heap.finish().expect("zero leaks");
}
