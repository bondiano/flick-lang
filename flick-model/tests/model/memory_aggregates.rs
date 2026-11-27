//! Real source-lowering and counted-model tests for abstract unboxed aggregates (#744).
//!
//! Exercises:
//! 1. Real Flick source lowered through `perceus::lower_roots_with_aggregates`:
//!    an inline aggregate containing a nested inline aggregate with a boxed
//!    descendant, projecting the nested aggregate, dropping the parent, reading
//!    the boxed descendant through the projection, and dropping the projection.
//! 2. Immediate unboxed aggregates executing with zero heap allocations/dups/drops.
//! 3. Fuel-prefix zero-leak sweeps with external aliases over aggregate source programs.
//! 4. Typed-cell `CellLayout`-checked `Reuse` fallback and `BoxAggregate` /
//!    `UnboxBorrow` / `UnboxConsume` through `execute_program_outcome`.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::build;
use flick_core::ir::memory::{
    self, Block, CellLayout, Exit, Function, Global, Instruction, Name, Operation, Program,
    ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::opt::perceus::{self, AggregatePolicy};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

fn lower_source_with_policy(
    source: &str,
    release: bool,
    build_policy: impl FnOnce(&str) -> AggregatePolicy,
) -> (Program, Name) {
    let modules = run::compile_source(source).expect("source checks");
    let main_path = run::main_of(&modules).expect("main binding");
    let policy = build_policy(&main_path.module);
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    let entry = Name::new(&main_path.module, &main_path.name);
    let program = perceus::lower_roots_with_aggregates(
        ir.iter().chain(build::builtin_modules()),
        modules.iter().map(|m| m.declarations.as_ref()),
        &[&entry],
        policy,
    )
    .expect("source aggregate memory lowering");
    memory::validate_program(&program).expect("lowered program validates");
    (program, entry)
}

fn count_aggregate_ops(block: &Block) -> (usize, usize) {
    let mut agg_constructs = 0;
    let mut agg_reuses = 0;
    for inst in &block.instructions {
        if let Instruction::Let { value, .. } = inst {
            match value {
                Operation::Construct {
                    rep: ValueRep::Aggregate,
                    ..
                }
                | Operation::ConstructWithDicts {
                    rep: ValueRep::Aggregate,
                    ..
                } => agg_constructs += 1,
                Operation::Reuse {
                    rep: ValueRep::Aggregate,
                    ..
                }
                | Operation::ReuseWithDicts {
                    rep: ValueRep::Aggregate,
                    ..
                } => agg_reuses += 1,
                _ => {}
            }
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        let (tc, tr) = count_aggregate_ops(then_);
        let (ec, er) = count_aggregate_ops(else_);
        agg_constructs += tc + ec;
        agg_reuses += tr + er;
    }
    (agg_constructs, agg_reuses)
}

fn program_aggregate_counts(program: &Program) -> (usize, usize) {
    program.functions.values().fold((0, 0), |(c, r), f| {
        let (fc, fr) = count_aggregate_ops(&f.body);
        (c + fc, r + fr)
    })
}

#[test]
fn real_source_nested_aggregate_with_boxed_descendant_projects_drops_parent_and_reads_cleanly() {
    let source = r#"
module Main exposing (main)

type Inner = Inner String Int
type Outer = Outer Inner Int | Empty

make_outer : Bool -> String -> Outer
let make_outer flag text =
  if flag then
    let inner = Inner (text ++ "lo") 42
    Outer inner 7
  else
    Empty

extract_inner : Outer -> Inner
let extract_inner outer =
  match outer with
  | Outer inner _ -> inner
  | Empty -> Inner "empty" 0

read_payload : Inner -> String
let read_payload inner =
  match inner with
  | Inner text _ -> text

main : String -> String
let main prefix =
  let outer = make_outer true prefix
  // `extract_inner` projects the nested inline aggregate and drops `outer`
  // before `read_payload` reads the boxed `String` descendant and drops `inner`.
  let projected = extract_inner outer
  read_payload projected
"#;
    for release in [false, true] {
        let (program, entry) = lower_source_with_policy(source, release, |m| AggregatePolicy {
            types: BTreeSet::from([format!("{m}.Inner"), format!("{m}.Outer")]),
            ..AggregatePolicy::default()
        });
        let (agg_constructs, agg_reuses) = program_aggregate_counts(&program);
        assert!(
            agg_constructs >= 2,
            "release={release}: expected aggregate constructs for Inner and Outer, got {agg_constructs}"
        );
        assert_eq!(
            agg_reuses, 0,
            "release={release}: aggregates must never be lowered to Reuse"
        );

        let mut heap = Heap::new();
        let arg = heap.literal(&Lit::String("hel".into())).expect("prefix");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![arg], 50_000);
        let ProgramOutcome::Returned(result) = report.outcome else {
            panic!(
                "release={release}: expected Returned, got {:?}",
                report.outcome
            );
        };
        assert_eq!(heap.string(&result), Ok("hello"));
        heap.drop_value(result).expect("drop returned string");
        let stats = heap
            .finish()
            .expect("zero live objects after explicit drop");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn real_source_nested_aggregate_fuel_prefix_sweep_preserves_external_alias_with_zero_leaks() {
    let source = r#"
module Main exposing (main)

type Leaf = Leaf String Int
type Node = Node Leaf Leaf

build : String -> Node
let build s =
  let left = Leaf (s ++ "-L") 1
  let right = Leaf (s ++ "-R") 2
  Node left right

main : Unit -> String
let main () =
  let node = build "root"
  match node with
  | Node (Leaf a _) (Leaf b _) -> a ++ ":" ++ b
"#;
    let (program, entry) = lower_source_with_policy(source, false, |m| AggregatePolicy {
        types: BTreeSet::from([format!("{m}.Leaf"), format!("{m}.Node")]),
        ..AggregatePolicy::default()
    });

    for fuel in 0..=400 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external-alias".into()))
            .expect("external root");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
        let finished = match report.outcome {
            ProgramOutcome::Returned(value) => {
                assert_eq!(heap.string(&value), Ok("root-L:root-R"));
                heap.drop_value(value).expect("drop returned value");
                true
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => false,
            other => panic!("unexpected outcome at fuel={fuel}: {other:?}"),
        };
        assert_eq!(heap.string(&external), Ok("external-alias"));
        assert_eq!(heap.reference_count(&external), Ok(1));
        heap.drop_value(external).expect("drop external alias");
        let stats = heap.finish().expect("zero leaks at fuel prefix");
        assert_eq!(stats.live_objects, 0);
        if finished {
            return;
        }
    }
    panic!("aggregate fixture did not finish within 400 fuel steps");
}

#[test]
fn real_source_immediate_aggregates_avoid_heap_allocations_for_data_constructors() {
    let source = r"
module Main exposing (main)

type Pair = Pair Int Int
type Wrap = Wrap Pair Int

sum_wrap : Wrap -> Int
let sum_wrap w =
  match w with
  | Wrap (Pair a b) c -> a + b + c

main : Unit -> Int
let main () =
  sum_wrap (Wrap (Pair 10 20) 12)
";
    let (program, entry) = lower_source_with_policy(source, false, |m| AggregatePolicy {
        types: BTreeSet::from([format!("{m}.Pair"), format!("{m}.Wrap")]),
        ..AggregatePolicy::default()
    });
    let (agg_constructs, agg_reuses) = program_aggregate_counts(&program);
    assert!(agg_constructs >= 2);
    assert_eq!(agg_reuses, 0);

    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 50_000);
    assert!(matches!(
        report.outcome,
        ProgramOutcome::Returned(Value::Int(42))
    ));
    report.outcome.release(&mut heap).expect("release");
    let stats = heap.finish().expect("zero live objects");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn real_source_typed_cells_with_incompatible_cell_layouts_fall_back_on_reuse() {
    let source = r#"
module Main exposing (main)

type CellA = CellA String Int
type CellB = CellB String String

swap : CellA -> CellB
let swap a =
  match a with
  | CellA s _ -> CellB s "second"

main : Unit -> String
let main () =
  match swap (CellA ("fi" ++ "rst") 1) with
  | CellB x y -> x ++ "+" ++ y
"#;
    let (program, entry) = lower_source_with_policy(source, false, |m| {
        let mut policy = AggregatePolicy::default();
        policy.cell_layouts.insert(
            format!("{m}.CellA.CellA"),
            CellLayout {
                capacity: 2,
                align: 8,
                ownership: "boxed,imm".into(),
            },
        );
        policy.cell_layouts.insert(
            format!("{m}.CellB.CellB"),
            CellLayout {
                capacity: 2,
                align: 16,
                ownership: "boxed,boxed".into(),
            },
        );
        policy
    });
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 50_000);
    let ProgramOutcome::Returned(result) = report.outcome else {
        panic!("expected Returned, got {:?}", report.outcome);
    };
    assert_eq!(heap.string(&result), Ok("first+second"));
    heap.drop_value(result).expect("drop result");
    let stats = heap.finish().expect("zero live objects");
    assert_eq!(stats.live_objects, 0);
    // `CellA` and `CellB` have equal field count (2) but incompatible layouts:
    // the emitted reuse site must fall back instead of reusing in place.
    assert_eq!(stats.reuses, 0);
    assert!(
        stats.reuse_fallbacks >= 1,
        "expected incompatible-layout reuse fallback, got {stats:?}"
    );
}

#[test]
fn program_box_unbox_shared_and_unique_paths_preserve_aliases_with_zero_leaks() {
    let entry = Name::new("Main", "main");
    let lay = CellLayout {
        capacity: 2,
        align: 8,
        ownership: "boxed,imm".into(),
    };
    let bind = |binding: u32, value: Operation| Instruction::Let { binding, value };
    let program = Program {
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        functions: BTreeMap::from([(
            0,
            Function {
                params: vec![],
                body: Block {
                    instructions: vec![
                        bind(0, Operation::Literal(Lit::String("shared-agg".into()))),
                        bind(1, Operation::Literal(Lit::Int(5))),
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
                                layout: Some(lay),
                            },
                        ),
                        // Share the boxed fallback cell across two locals.
                        Instruction::Dup(3),
                        // Shared UnboxConsume retains boxed descendant and keeps alias alive.
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
                        // Unique UnboxConsume now frees the fallback cell without extra descendant dup.
                        bind(6, Operation::UnboxConsume { object: 3 }),
                        bind(
                            7,
                            Operation::Project {
                                object: 6,
                                field: 0,
                                rep: ValueRep::Aggregate,
                            },
                        ),
                        Instruction::Drop(6),
                        bind(
                            8,
                            Operation::Binary {
                                op: BinOp::Concat,
                                left: 5,
                                right: 7,
                            },
                        ),
                    ],
                    exit: Exit::Return(8),
                },
            },
        )]),
        op_spans: BTreeMap::new(),
    };
    memory::validate_program(&program).expect("valid program");
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &program, &entry, vec![], 10_000);
    let ProgramOutcome::Returned(result) = report.outcome else {
        panic!("expected Returned, got {:?}", report.outcome);
    };
    assert_eq!(heap.string(&result), Ok("shared-aggshared-agg"));
    heap.drop_value(result).expect("drop result");
    let stats = heap.finish().expect("zero leaks");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}
