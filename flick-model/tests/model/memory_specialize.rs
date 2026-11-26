//! End-to-end specialization (#743) + layout engine (#742) + Perceus (#744) +
//! counted tier-0 model tests over real Flick source programs.

use flick_backend_support::layout::{Discriminant, PrimitiveTable, TypeRepr, value_rep_of};
use flick_backend_support::specialize::{
    DefMode, FallbackReason, SpecExpr, SpecExprKind, SpecStmt, SpecializeConfig,
    SpecializedProgram, specialize,
};
use flick_core::build;
use flick_core::ir::memory::{self, Block, Exit, Instruction, Name, Operation, Program, ValueRep};
use flick_core::ir::{Lit, lang_items};
use flick_core::opt::perceus;
use flick_ir::{IrType, Program as IrProgram, TypeRef};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

#[derive(Default, Debug)]
struct MemoryOpCounts {
    agg_constructs: usize,
    agg_reuses: usize,
    cell_with_layout: usize,
    box_aggregate: usize,
    unbox_borrow: usize,
    unbox_consume: usize,
}

fn count_ops_in_block(block: &Block, counts: &mut MemoryOpCounts) {
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
                } => counts.agg_constructs += 1,
                Operation::Reuse {
                    rep: ValueRep::Aggregate,
                    ..
                }
                | Operation::ReuseWithDicts {
                    rep: ValueRep::Aggregate,
                    ..
                } => counts.agg_reuses += 1,
                Operation::Construct {
                    rep: ValueRep::Cell,
                    layout: Some(_),
                    ..
                }
                | Operation::ConstructWithDicts {
                    rep: ValueRep::Cell,
                    layout: Some(_),
                    ..
                }
                | Operation::Reuse {
                    layout: Some(_), ..
                }
                | Operation::ReuseWithDicts {
                    layout: Some(_), ..
                } => counts.cell_with_layout += 1,
                Operation::BoxAggregate { .. } => counts.box_aggregate += 1,
                Operation::UnboxBorrow { .. } => counts.unbox_borrow += 1,
                Operation::UnboxConsume { .. } => counts.unbox_consume += 1,
                _ => {}
            }
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        count_ops_in_block(then_, counts);
        count_ops_in_block(else_, counts);
    }
}

fn count_spec_boxed_cells(expr: &SpecExpr) -> usize {
    match &expr.kind {
        SpecExprKind::Box { expr: inner, .. } | SpecExprKind::Unbox { expr: inner, .. } => {
            1 + count_spec_boxed_cells(inner)
        }
        SpecExprKind::App { func, arg, .. }
        | SpecExprKind::Binary(_, func, arg)
        | SpecExprKind::WithOwnerEvidence(func, arg) => {
            count_spec_boxed_cells(func) + count_spec_boxed_cells(arg)
        }
        SpecExprKind::Lambda { body, .. }
        | SpecExprKind::Abstract(_, body)
        | SpecExprKind::Dicts(body, _)
        | SpecExprKind::FieldOffset { record: body, .. }
        | SpecExprKind::FieldByLabel { record: body, .. }
        | SpecExprKind::Not(body)
        | SpecExprKind::Unwrap(body)
        | SpecExprKind::Neg(_, body)
        | SpecExprKind::Ignore(body)
        | SpecExprKind::Await(body)
        | SpecExprKind::Task(body)
        | SpecExprKind::Within(_, body)
        | SpecExprKind::Lift(body, _)
        | SpecExprKind::Yield(body) => count_spec_boxed_cells(body),
        SpecExprKind::Block(stmts, tail) => {
            stmts
                .iter()
                .map(|s| match s {
                    SpecStmt::Let { value, .. }
                    | SpecStmt::Var { value, .. }
                    | SpecStmt::Assign(_, value)
                    | SpecStmt::Fun { value, .. }
                    | SpecStmt::Expr(value) => count_spec_boxed_cells(value),
                })
                .sum::<usize>()
                + count_spec_boxed_cells(tail)
        }
        SpecExprKind::If(c, t, e) => {
            count_spec_boxed_cells(c) + count_spec_boxed_cells(t) + count_spec_boxed_cells(e)
        }
        SpecExprKind::Match(s, arms) => {
            count_spec_boxed_cells(s)
                + arms
                    .iter()
                    .map(|a| {
                        a.guard.as_ref().map_or(0, count_spec_boxed_cells)
                            + count_spec_boxed_cells(&a.body)
                    })
                    .sum::<usize>()
        }
        SpecExprKind::Tuple(items) | SpecExprKind::List(items) => {
            items.iter().map(count_spec_boxed_cells).sum()
        }
        SpecExprKind::Record { fields, .. } | SpecExprKind::Dictionary(fields) => {
            fields.iter().map(|(_, v)| count_spec_boxed_cells(v)).sum()
        }
        _ => 0,
    }
}

fn count_program_ops(program: &Program) -> MemoryOpCounts {
    let mut counts = MemoryOpCounts::default();
    for func in program.functions.values() {
        count_ops_in_block(&func.body, &mut counts);
    }
    counts
}

fn specialize_and_lower_source(source: &str, release: bool) -> (SpecializedProgram, Program, Name) {
    let compiled = run::compile_source(source).expect("source checks");
    let main_path = run::main_of(&compiled).expect("main binding");
    let mut ir_modules: Vec<_> = compiled
        .iter()
        .map(|module| {
            if release {
                build::optimized(&compiled, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    for builtin in build::builtin_modules() {
        if !ir_modules.iter().any(|m| m.name == builtin.name) {
            ir_modules.push(builtin.clone());
        }
    }
    let ir_program = IrProgram {
        ir_version: flick_ir::VERSION.to_owned(),
        compiler: "test".to_owned(),
        files: Vec::new(),
        modules: ir_modules,
        lang: lang_items(),
    };
    let roots = vec![(main_path.module.to_string(), main_path.name.to_string())];
    let specialized = specialize(
        &ir_program,
        &roots,
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("specialize succeeds");
    let entry = Name::new(&main_path.module, &main_path.name);
    let memory_program = perceus::lower_roots_specialized(
        &ir_program.modules,
        &specialized,
        compiled.iter().map(|m| m.declarations.as_ref()),
        &[&entry],
    )
    .expect("specialized memory lowering succeeds");
    memory::validate_program(&memory_program).expect("lowered memory program validates");
    (specialized, memory_program, entry)
}

#[test]
#[allow(clippy::too_many_lines)]
fn end_to_end_specialize_layout_perceus_tier0_unboxed_aggregates_and_nested_boxed_descendants() {
    let source = r#"
module Main exposing (main)

type Shape =
  | Circle Float
  | Rect Float Float

type Inner = Inner String Int
type Outer = Outer Inner Int | Empty

type alias Metrics = { flag : Bool; value : Float; tag : Int }

opt_map : (a -> b) -> Option a -> Option b
let opt_map f opt =
  match opt with
  | None -> None
  | Some x -> Some (f x)

sum_list : Int -> List Int -> Int
let rec sum_list acc xs =
  match xs with
  | [] -> acc
  | [h, ...t] -> sum_list (acc + h) t

area : Shape -> Float
let area s =
  match s with
  | Circle r -> r * r
  | Rect w h -> w * h

read_outer : Outer -> String
let read_outer o =
  match o with
  | Outer (Inner text n) k -> text ++ ":" ++ show (n + k)
  | Empty -> "empty"

main : Float -> String
let main radius =
  let c = Circle radius
  let r = Rect 2.0 5.0
  let mapped = opt_map (fun x -> x + 1.5) (Some (area c + area r))
  let opt_val =
    match mapped with
    | Some v -> v
    | None -> 0.0
  let total = sum_list 0 [10, 20, 12]
  let rec_val : Metrics = { flag = true; value = opt_val; tag = total }
  let outer = Outer (Inner ("box" ++ "-ok") rec_val.tag) 8
  let summary = read_outer outer
  let flag_str = if rec_val.flag then "T" else "F"
  flag_str ++ "|" ++ show rec_val.value ++ "|" ++ summary
"#;
    for release in [false, true] {
        let (specialized, program, entry) = specialize_and_lower_source(source, release);

        // Verify `#742` layouts were automatically computed for Shape and Option Float.
        let shape_ty = TypeRef::new(IrType::Named {
            module: "Main".into(),
            name: "Shape".into(),
            args: vec![],
        });
        let shape_layout = specialized
            .layouts
            .get(&shape_ty)
            .expect("Shape layout computed");
        assert_eq!(shape_layout.size, 24);
        assert_eq!(shape_layout.align, 8);
        assert_eq!(value_rep_of(shape_layout), ValueRep::Aggregate);
        let TypeRepr::Data(shape_data) = &shape_layout.repr else {
            panic!("expected Data repr for Shape");
        };
        assert_eq!(
            shape_data.discriminant,
            Discriminant::Tag {
                offset: 0,
                width: 1
            }
        );

        let opt_float_ty = TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Option".into(),
            args: vec![TypeRef::new(IrType::Named {
                module: "Prelude".into(),
                name: "Float".into(),
                args: vec![],
            })],
        });
        let opt_layout = specialized
            .layouts
            .get(&opt_float_ty)
            .expect("Option Float layout computed");
        assert_eq!(opt_layout.size, 16);
        assert_eq!(opt_layout.align, 8);
        assert_eq!(value_rep_of(opt_layout), ValueRep::Aggregate);

        // Zero boxed cells in specialized `opt_map` (`Option Float`) and `sum_list` (`List Int`).
        for def in specialized.definitions.values() {
            if def.source.0 == "Main"
                && matches!(def.source.1.as_str(), "opt_map" | "sum_list" | "area")
            {
                assert_eq!(
                    count_spec_boxed_cells(&def.body),
                    0,
                    "release={release}: specialized {} must have 0 boxed cells",
                    def.symbol
                );
            }
        }
        assert_eq!(specialized.stats.fallback_definitions, 0);

        // Perceus automatically derived `ValueRep::Aggregate` with zero aggregate Reuse.
        let counts = count_program_ops(&program);
        let min_aggs = if release { 1 } else { 4 };
        assert!(
            counts.agg_constructs >= min_aggs,
            "release={release}: expected >= {min_aggs} automatic aggregate constructs, got {counts:?}"
        );
        assert_eq!(counts.agg_reuses, 0);

        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Float(3.0)], 50_000);
        let ProgramOutcome::Returned(result) = report.outcome else {
            panic!(
                "release={release}: expected Returned, got {:?}",
                report.outcome
            );
        };
        // area(Circle 3.0) = 9.0, area(Rect 2.0 5.0) = 10.0, + 1.5 = 20.5; total = 42, + 8 = 50
        assert_eq!(heap.string(&result), Ok("T|20.5|box-ok:50"));
        heap.drop_value(result).expect("drop returned string");
        let stats = heap.finish().expect("zero live objects");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn end_to_end_specialize_fallback_emits_box_unbox_borrow_and_consume_with_zero_leaks() {
    let source = r#"
module Main exposing (main, poly_bounce, apply_hr)

type Shape =
  | Circle Float
  | Rect Float Float

type Inner = Inner String Int
type Outer = Outer Inner Int

poly_bounce : forall a. Int -> a -> a
let rec poly_bounce n x =
  if n <= 0 then x
  else
    let pair = poly_bounce (n - 1) (x, x)
    match pair with
    | (left, _) -> left

apply_hr : (forall a. a -> a) -> Shape -> Shape
let apply_hr f s = f s

area : Shape -> Float
let area s =
  match s with
  | Circle r -> r * r
  | Rect w h -> w * h

main : Float -> String
let main radius =
  // 1. Polymorphic recursion with unboxed Shape in `let` position -> BoxAggregate + UnboxConsume.
  let bounced_shape = poly_bounce 2 (Circle radius)
  // 2. Higher-rank callback over unboxed Shape -> BoxAggregate + UnboxConsume.
  let hr_shape = apply_hr (fun x -> x) bounced_shape
  // 3. Polymorphic recursion with nested aggregate containing boxed String in `match` scrutinee
  //    -> BoxAggregate + UnboxBorrow (depends on `radius` so release cannot constant-fold it).
  let label =
    match poly_bounce 2 (Outer (Inner ("fall" ++ show radius) 3) 4) with
    | Outer (Inner s a) b -> s ++ ":" ++ show (a + b)
  show (area hr_shape) ++ "|" ++ label
"#;
    for release in [false, true] {
        let (specialized, program, entry) = specialize_and_lower_source(source, release);

        assert!(
            specialized.definitions.values().any(|d| {
                d.source == ("Main".into(), "poly_bounce".into())
                    && d.mode == DefMode::Fallback(FallbackReason::PolymorphicRecursion)
            }),
            "release={release}: expected PolymorphicRecursion fallback on poly_bounce"
        );
        if !release {
            assert!(
                specialized.definitions.values().any(|d| {
                    d.source == ("Main".into(), "apply_hr".into())
                        && d.mode == DefMode::Fallback(FallbackReason::HigherRankCallback)
                }),
                "release={release}: expected HigherRankCallback fallback on apply_hr"
            );
        }

        let counts = count_program_ops(&program);
        assert_eq!(counts.agg_reuses, 0);
        assert!(
            counts.box_aggregate >= 2,
            "release={release}: expected automatic BoxAggregate emission, got {counts:?}"
        );
        assert!(
            counts.unbox_consume >= 1,
            "release={release}: expected automatic UnboxConsume emission, got {counts:?}"
        );
        assert!(
            counts.unbox_borrow >= 1,
            "release={release}: expected automatic UnboxBorrow emission, got {counts:?}"
        );

        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Float(4.0)], 50_000);
        let ProgramOutcome::Returned(result) = report.outcome else {
            panic!(
                "release={release}: expected Returned, got {:?}",
                report.outcome
            );
        };
        assert_eq!(heap.string(&result), Ok("16|fall4:7"));
        heap.drop_value(result).expect("drop result");
        let stats = heap.finish().expect("zero live objects");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);

        // Fuel-prefix sweep with an independent external alias on debug profile.
        if !release {
            let mut finished = false;
            for fuel in 0..=800 {
                let mut sweep_heap = Heap::new();
                let external = sweep_heap
                    .literal(&Lit::String("external-fallback-alias".into()))
                    .expect("external");
                let sweep = execute_program_outcome(
                    &mut sweep_heap,
                    &program,
                    &entry,
                    vec![Value::Float(4.0)],
                    fuel,
                );
                match sweep.outcome {
                    ProgramOutcome::Returned(val) => {
                        assert_eq!(sweep_heap.string(&val), Ok("16|fall4:7"));
                        sweep_heap.drop_value(val).expect("drop val");
                        finished = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => panic!("unexpected outcome at fuel={fuel}: {other:?}"),
                }
                assert_eq!(sweep_heap.string(&external), Ok("external-fallback-alias"));
                assert_eq!(sweep_heap.reference_count(&external), Ok(1));
                sweep_heap.drop_value(external).expect("drop external");
                let sweep_stats = sweep_heap.finish().expect("zero leaks at fuel prefix");
                assert_eq!(sweep_stats.live_objects, 0);
                if finished {
                    break;
                }
            }
            assert!(
                finished,
                "fallback fixture did not finish within 800 fuel steps"
            );
        }
    }
}

#[test]
fn recursive_adt_gets_derived_cell_layout_and_typed_reuse_compatibility() {
    let source = r"
module Main exposing (main)

type IntList = Nil | Cons Int IntList

bump : IntList -> IntList
let rec bump xs =
  match xs with
  | Nil -> Nil
  | Cons h t -> Cons (h + 1) (bump t)

sum : IntList -> Int
let rec sum xs =
  match xs with
  | Nil -> 0
  | Cons h t -> h + sum t

main : Int -> Int
let main seed =
  let xs = Cons seed (Cons (seed + 1) (Cons (seed + 2) Nil))
  sum (bump xs)
";
    for release in [false, true] {
        let (specialized, program, entry) = specialize_and_lower_source(source, release);

        let list_ty = TypeRef::new(IrType::Named {
            module: "Main".into(),
            name: "IntList".into(),
            args: vec![],
        });
        let list_layout = specialized
            .layouts
            .get(&list_ty)
            .expect("IntList layout computed");
        assert_eq!(
            value_rep_of(list_layout),
            ValueRep::Cell,
            "recursive IntList must lower as ValueRep::Cell"
        );

        let counts = count_program_ops(&program);
        assert!(
            counts.cell_with_layout >= 1,
            "release={release}: expected derived CellLayout on recursive IntList cell, got {counts:?}"
        );
        assert_eq!(counts.agg_reuses, 0);

        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Int(10)], 50_000);
        let ProgramOutcome::Returned(result) = report.outcome else {
            panic!("expected Returned, got {:?}", report.outcome);
        };
        // (10+1) + (11+1) + (12+1) = 36
        assert_eq!(result, Value::Int(36));
        let stats = heap.finish().expect("zero live objects");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}
