use super::*;

// ============================================================================
// B. Deep #744 unboxed aggregate rules
// ============================================================================

#[test]
fn m4_aggregate_three_level_nested_inline_projection_survives_parent_and_mid_drops() {
    let source = r#"
module Main exposing (main)

type Inner = Inner String Int
type Mid = Mid Inner Int
type Outer = Outer Mid Int

project_deep : Outer -> String
let project_deep outer =
  let mid =
    match outer with
    | Outer m _ -> m
  let inner =
    match mid with
    | Mid i _ -> i
  match inner with
  | Inner s _ -> s

main : String -> String
let main prefix =
  let outer = Outer (Mid (Inner (prefix ++ "-deep") 3) 2) 1
  project_deep outer
"#;
    for release in [false, true] {
        let (program, entry) = lower_source_with_policy(source, release, |m| AggregatePolicy {
            types: BTreeSet::from([
                format!("{m}.Inner"),
                format!("{m}.Mid"),
                format!("{m}.Outer"),
            ]),
            ..AggregatePolicy::default()
        });
        let mut heap = Heap::new();
        let arg = heap.literal(&Lit::String("m4".into())).expect("arg");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![arg], 50_000);
        let ProgramOutcome::Returned(result) = report.outcome else {
            panic!(
                "release={release}: expected Returned, got {:?}",
                report.outcome
            );
        };
        assert_eq!(heap.string(&result), Ok("m4-deep"));
        heap.drop_value(result).expect("drop result");
        let stats = heap.finish().expect("zero leaks");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn m4_aggregate_box_then_drop_without_unbox_releases_descendants() {
    let layout = CellLayout {
        capacity: 2,
        align: 8,
        ownership: "boxed,imm".into(),
    };
    let (program, entry) = single_fn_program(
        vec![
            bind(
                0,
                Operation::Literal(Lit::String("inside-boxed-agg".into())),
            ),
            bind(1, Operation::Literal(Lit::Int(7))),
            bind(
                2,
                Operation::Construct {
                    tag: "Main.P.P".into(),
                    fields: vec![0, 1],
                    rep: ValueRep::Aggregate,
                    layout: None,
                },
            ),
            bind(
                3,
                Operation::BoxAggregate {
                    value: 2,
                    layout: Some(layout),
                },
            ),
            Instruction::Drop(3),
            bind(4, Operation::Literal(Lit::Int(99))),
        ],
        Exit::Return(4),
    );
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &program, &entry, vec![], 10_000);
    assert!(matches!(
        report.outcome,
        ProgramOutcome::Returned(Value::Int(99))
    ));
    let stats = heap.finish().expect("zero leaks");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, 2);
    assert_eq!(stats.frees, 2);
}

fn shared_unbox_alias_program(layout: CellLayout) -> (Program, Name) {
    single_fn_program(
        vec![
            bind(0, Operation::Literal(Lit::String("shared-leaf".into()))),
            bind(1, Operation::Literal(Lit::Int(12))),
            bind(
                2,
                Operation::Construct {
                    tag: "Main.P.P".into(),
                    fields: vec![0, 1],
                    rep: ValueRep::Aggregate,
                    layout: Some(layout),
                },
            ),
            bind(
                3,
                Operation::BoxAggregate {
                    value: 2,
                    layout: None,
                },
            ),
            Instruction::Dup(3),
            bind(4, Operation::UnboxConsume { object: 3 }),
            bind(5, Operation::UnboxBorrow { object: 3 }),
            Instruction::Drop(3),
            bind(
                6,
                Operation::Project {
                    object: 4,
                    field: 0,
                    rep: ValueRep::Aggregate,
                },
            ),
            Instruction::Drop(4),
            bind(
                7,
                Operation::Project {
                    object: 5,
                    field: 0,
                    rep: ValueRep::Aggregate,
                },
            ),
            Instruction::Drop(5),
            bind(
                8,
                Operation::Binary {
                    op: BinOp::Concat,
                    left: 6,
                    right: 7,
                },
            ),
        ],
        Exit::Return(8),
    )
}

#[test]
fn m4_aggregate_box_then_unique_and_shared_unbox_consume_transfer_and_alias_symmetry() {
    let layout = CellLayout {
        capacity: 2,
        align: 8,
        ownership: "boxed,imm".into(),
    };

    let (direct_prog, direct_entry) = single_fn_program(
        vec![
            bind(0, Operation::Literal(Lit::String("unique-leaf".into()))),
            bind(1, Operation::Literal(Lit::Int(11))),
            bind(
                2,
                Operation::Construct {
                    tag: "Main.P.P".into(),
                    fields: vec![0, 1],
                    rep: ValueRep::Aggregate,
                    layout: None,
                },
            ),
            bind(
                3,
                Operation::Project {
                    object: 2,
                    field: 0,
                    rep: ValueRep::Aggregate,
                },
            ),
            Instruction::Drop(2),
        ],
        Exit::Return(3),
    );
    let direct_stats =
        run_program_expect_string(&direct_prog, &direct_entry, "unique-leaf", "direct");

    let (unique_prog, unique_entry) = single_fn_program(
        vec![
            bind(0, Operation::Literal(Lit::String("unique-leaf".into()))),
            bind(1, Operation::Literal(Lit::Int(11))),
            bind(
                2,
                Operation::Construct {
                    tag: "Main.P.P".into(),
                    fields: vec![0, 1],
                    rep: ValueRep::Aggregate,
                    layout: None,
                },
            ),
            bind(
                3,
                Operation::BoxAggregate {
                    value: 2,
                    layout: Some(layout.clone()),
                },
            ),
            bind(4, Operation::UnboxConsume { object: 3 }),
            bind(
                5,
                Operation::Project {
                    object: 4,
                    field: 0,
                    rep: ValueRep::Aggregate,
                },
            ),
            Instruction::Drop(4),
        ],
        Exit::Return(5),
    );
    let unique_stats =
        run_program_expect_string(&unique_prog, &unique_entry, "unique-leaf", "unique-unbox");
    assert_eq!(unique_stats.dups, direct_stats.dups);

    let (shared_prog, shared_entry) = shared_unbox_alias_program(layout);
    let _ = run_program_expect_string(
        &shared_prog,
        &shared_entry,
        "shared-leafshared-leaf",
        "shared-unbox",
    );
}

// ============================================================================
// C. Failure / abort / control discontinuation / fuel-cut + Arena unwind
// ============================================================================

const UNWIND_SCENARIO_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

type error Boom = Boom String
type Inner = Inner String Int
type Outer = Outer Inner Int
type Node = Empty | Node Int Node

effect Stop =
  abort stop : forall a. String ->{} a

effect Ctrl =
  control ask : String ->{} String

build_list : Int -> Node
let rec build_list n =
  if n <= 0 then Empty else Node n (build_list (n - 1))

fallible_map : Node ->{; Boom} Node
let rec fallible_map xs =
  match xs with
  | Empty -> Empty
  | Node h t ->
      if h == 2 then Fail.fail (Boom "hit-2")
      else Node (h + 1) (fallible_map t)

run_failure_case : Unit ->{Host.Console} Unit
let run_failure_case () =
  try
    scope {
      let arena = Arena.make ()
      let view = Arena.alloc arena (Outer (Inner ("ar" ++ "ena") 1) 2)
      Scope.add_finalizer (task {
        let late_v = Arena.alloc arena "late-ok"
        match Arena.get view with
        | Outer (Inner s _) _ -> Host.raw_stdout_line ("fin1:" ++ s ++ ":" ++ Arena.get late_v)
      })
      Scope.add_finalizer (task { Host.raw_stdout_line "fin2" })
      let _ = fallible_map (build_list 4)
      ()
    }
  with
  | Boom msg -> Host.raw_stdout_line ("caught:" ++ msg)

run_abort_case : Unit ->{Host.Console} Unit
let run_abort_case () =
  let out =
    try
      scope {
        let arena = Arena.make ()
        let v = Arena.alloc arena "abort-slot"
        Scope.add_finalizer (task { Host.raw_stdout_line ("abort-fin:" ++ Arena.get v) })
        let _ = Stop.stop "stopped"
        "unreachable"
      }
    with
    | Stop.stop msg -> "aborted:" ++ msg
  Host.raw_stdout_line out

run_control_discontinue_case : Unit ->{Host.Console} Unit
let run_control_discontinue_case () =
  let out =
    try
      scope {
        let arena = Arena.make ()
        let v = Arena.alloc arena "ctrl-slot"
        Scope.add_finalizer (task { Host.raw_stdout_line ("ctrl-fin:" ++ Arena.get v) })
        let _ = Ctrl.ask "drop-k"
        "unreachable"
      }
    with
    | Ctrl.ask msg -> "discontinued:" ++ msg
  Host.raw_stdout_line out

main : Unit ->{Host.Console} Unit
let main () =
  run_failure_case ()
  run_abort_case ()
  run_control_discontinue_case ()
"#;

fn sweep_unwind_prefixes(counted: &CountedMain, expected: &str, max_fuel: u64) {
    for fuel in 0..=max_fuel {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("unwind-external-alias".into()))
            .expect("external");
        let report = counted.execute(&mut heap, fuel);
        match report.outcome {
            ProgramOutcome::Returned(v) => {
                assert_eq!(v, Value::Unit);
                assert_eq!(report.stdout, expected);
                heap.drop_value(v).expect("drop unit");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                assert!(
                    expected.starts_with(&report.stdout),
                    "fuel={fuel}: stdout {:?} not prefix of expected",
                    report.stdout
                );
            }
            other => {
                other.release(&mut heap).expect("release");
                panic!("fuel={fuel}: unexpected outcome");
            }
        }
        assert_eq!(heap.string(&external), Ok("unwind-external-alias"));
        assert_eq!(heap.stats().live_objects, 1, "fuel={fuel}: VM leaked");
        heap.drop_value(external).expect("drop external");
        let stats = heap.finish().expect("zero leaks after dropping external");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn m4_unwind_live_reuse_token_nested_aggregate_and_arena_lifo_on_failure_and_abort() {
    let expected = "fin2\nfin1:arena:late-ok\ncaught:hit-2\nabort-fin:abort-slot\naborted:stopped\nctrl-fin:ctrl-slot\ndiscontinued:drop-k\n";
    let modules = compile_source(UNWIND_SCENARIO_SOURCE).expect("source checks");
    let entry = main_of(&modules).expect("main binding");
    for release in [false, true] {
        let counted = counted_main(&modules, &entry, release).expect("lower");
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, 2_000_000);
        assert!(
            matches!(report.outcome, ProgramOutcome::Returned(Value::Unit)),
            "release={release}: unexpected outcome {:?}",
            report.outcome
        );
        assert_eq!(report.stdout, expected, "release={release}");
        report.outcome.release(&mut heap).expect("release");
        let stats = heap.finish().expect("zero leaks");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }

    let counted = counted_main(&modules, &entry, false).expect("lower debug");
    sweep_unwind_prefixes(&counted, expected, 400);
}

// ============================================================================
// D. Compiler-to-model / unproved gap pins
// ============================================================================

#[test]
fn m4_boundary_ref_cycle_root_arena_retention_vs_prompt_acyclic_release() {
    let acyclic_src = r#"
module App.Main exposing (main)

type Box = Box String

main : Unit ->{Host.Console} Unit
let main () =
  match Box ("a" ++ "b") with
  | Box s -> Host.raw_stdout_line s
"#;
    let cyclic_src = r#"
module App.Main exposing (main)

type Node = Node String (Ref (Option Node))

main : Unit ->{Host.Console} Unit
let main () =
  let cell = Ref.make None
  let node = Node ("cy" ++ "cle") cell
  Ref.set cell (Some node)
  match Ref.get cell with
  | Some (Node s _) -> Host.raw_stdout_line s
  | None -> Host.raw_stdout_line "none"
"#;

    let run = |src: &str, want: &str| {
        let modules = compile_source(src).expect("compile");
        let entry = main_of(&modules).expect("main");
        let counted = counted_main(&modules, &entry, false).expect("lower");
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, 1_000_000);
        assert_eq!(report.stdout, want);
        report.outcome.release(&mut heap).expect("release");
        heap.finish().expect("zero leaks")
    };

    let acyc = run(acyclic_src, "ab\n");
    assert_eq!(acyc.cells_detached_on_teardown, 0);
    assert_eq!(acyc.live_objects, 0);

    let cyc = run(cyclic_src, "cycle\n");
    assert!(
        cyc.cells_detached_on_teardown > 0,
        "Ref cycle must be reclaimed by root-arena detach-all teardown, not prompt RC"
    );
    assert_eq!(cyc.live_objects, 0);
    assert_eq!(cyc.cells_allocated, cyc.cells_freed);
}

#[test]
fn m4_boundary_non_reusable_storage_kinds_fall_back_cleanly() {
    let cases: [(&str, Operation); 3] = [
        (
            "String",
            Operation::Literal(Lit::String("non-reusable".into())),
        ),
        (
            "Closure",
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        (
            "Dictionary",
            Operation::Dictionary {
                entries: vec![("m".into(), DictionaryEntry::Value(0))],
            },
        ),
    ];

    for (label, make_non_reusable) in cases {
        let entry = Name::new("Main", "main");
        let mut instructions = Vec::new();
        if label == "Dictionary" {
            instructions.push(bind(0, Operation::Literal(Lit::Int(1))));
        }
        instructions.extend([
            bind(1, make_non_reusable),
            bind(2, Operation::DropReuse(1)),
            bind(3, Operation::Literal(Lit::String("fresh-cell".into()))),
            bind(
                4,
                Operation::Reuse {
                    object: 2,
                    tag: "Main.C.C".into(),
                    fields: vec![3],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            bind(
                5,
                Operation::Project {
                    object: 4,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            Instruction::Drop(4),
        ]);
        let program = Program {
            globals: vec![Global {
                name: entry.clone(),
                initializer: 0,
            }],
            functions: BTreeMap::from([
                (
                    0,
                    Function {
                        params: vec![],
                        body: Block {
                            instructions,
                            exit: Exit::Return(5),
                        },
                    },
                ),
                (
                    1,
                    Function {
                        params: vec![0, 1],
                        body: Block {
                            instructions: vec![Instruction::Drop(0)],
                            exit: Exit::Return(1),
                        },
                    },
                ),
            ]),
            op_spans: BTreeMap::new(),
        };
        memory::validate_program(&program).expect("valid IR");
        let stats = run_program_expect_string(&program, &entry, "fresh-cell", label);
        assert_eq!(stats.reuses, 0, "{label}: must never reuse {label} storage");
    }
}
