//! Differential checks comparing `flick_vm::tier1` against `flick_vm::tier0`
//! on generated programs, generated effect-handler programs, and CLI fuel-prefix sweeps.

use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_model::types::Type;
use flick_vm::stdlib;
use flick_vm::tier0::{
    Error as MemoryError, Heap, HostMode, ProgramOutcome, Value,
    execute_program_outcome as tier0_exec_program,
};
use flick_vm::tier1;

use crate::memory_tier1_diff::is_tier1_subset;

fn values_structurally_equal(h0: &Heap, v0: &Value, h1: &Heap, v1: &Value) -> Result<bool, String> {
    match (v0, v1) {
        (Value::Unit, Value::Unit) => Ok(true),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Int8(a), Value::Int8(b)) => Ok(a == b),
        (Value::Int16(a), Value::Int16(b)) => Ok(a == b),
        (Value::Int64(a), Value::Int64(b)) => Ok(a == b),
        (Value::UInt8(a), Value::UInt8(b)) => Ok(a == b),
        (Value::UInt16(a), Value::UInt16(b)) => Ok(a == b),
        (Value::UInt32(a), Value::UInt32(b)) => Ok(a == b),
        (Value::UInt64(a), Value::UInt64(b)) => Ok(a == b),
        (Value::Float(a), Value::Float(b)) => {
            Ok(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()))
        }
        (Value::Float32(a), Value::Float32(b)) => {
            Ok(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()))
        }
        (
            Value::Aggregate {
                tag: t0,
                ordinary_fields: o0,
                fields: f0,
            },
            Value::Aggregate {
                tag: t1,
                ordinary_fields: o1,
                fields: f1,
            },
        ) => {
            if t0 != t1 || o0 != o1 {
                return Ok(false);
            }
            for (a, b) in f0[..*o0].iter().zip(&f1[..*o1]) {
                if !values_structurally_equal(h0, a, h1, b)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Object(_), Value::Object(_)) => {
            if let (Ok(s0), Ok(s1)) = (h0.string(v0), h1.string(v1)) {
                return Ok(s0 == s1);
            }
            if let (Ok(b0), Ok(b1)) = (h0.bigint(v0), h1.bigint(v1)) {
                return Ok(b0 == b1);
            }
            if let (Ok(t0), Ok(t1)) = (h0.tag(v0), h1.tag(v1)) {
                if t0 != t1 {
                    return Ok(false);
                }
                for i in 0.. {
                    match (h0.field(v0, i), h1.field(v1, i)) {
                        (Ok(a), Ok(b)) => {
                            if !values_structurally_equal(h0, a, h1, b)? {
                                return Ok(false);
                            }
                        }
                        (Err(_), Err(_)) => return Ok(true),
                        _ => return Ok(false),
                    }
                }
            }
            match (stdlib::show_value(h0, v0), stdlib::show_value(h1, v1)) {
                (Ok(s0), Ok(s1)) => Ok(s0 == s1),
                (Err(_), Err(_)) => Ok(true),
                (r0, r1) => Err(format!("show mismatch: {r0:?} vs {r1:?}")),
            }
        }
        _ => Ok(false),
    }
}

fn compare_generated_profile(
    counted: &run::CountedMain,
    is_unit_fn: bool,
    seed: usize,
    release: bool,
    source: &str,
    fuel: u64,
) {
    let mut heap0 = Heap::new();
    let r0 = if is_unit_fn {
        counted.execute(&mut heap0, fuel)
    } else {
        tier0_exec_program(&mut heap0, &counted.program, &counted.entry, vec![], fuel)
    };
    let mut heap1 = Heap::new();
    let r1 = if is_unit_fn {
        tier1::execute_main_program_outcome_with_host(
            &mut heap1,
            &counted.program,
            &counted.entry,
            fuel,
            HostMode::Deterministic,
        )
    } else {
        tier1::execute_program_outcome(&mut heap1, &counted.program, &counted.entry, vec![], fuel)
    }
    .unwrap_or_else(|err| {
        panic!("seed {seed} (release={release}) tier1 prepare failed: {err}\n{source}")
    });

    assert_eq!(
        r0.stdout, r1.stdout,
        "seed {seed} (release={release}) stdout mismatch\n{source}"
    );
    match (&r0.outcome, &r1.outcome) {
        (ProgramOutcome::Returned(v0), ProgramOutcome::Returned(v1))
        | (ProgramOutcome::Raised(v0), ProgramOutcome::Raised(v1)) => {
            let eq = values_structurally_equal(&heap0, v0, &heap1, v1)
                .unwrap_or_else(|e| panic!("seed {seed} value compare error: {e}\n{source}"));
            assert!(
                eq,
                "seed {seed} (release={release}) value mismatch:\ntier0={v0:?}\ntier1={v1:?}\n{source}"
            );
        }
        (ProgramOutcome::Failed(_), ProgramOutcome::Failed(_))
        | (
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
        ) => {}
        (o0, o1) => panic!(
            "seed {seed} (release={release}) outcome mismatch:\ntier0={o0:?}\ntier1={o1:?}\n{source}"
        ),
    }
    r0.outcome.release(&mut heap0).expect("release tier0");
    heap0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut heap1).expect("release tier1");
    let stats = heap1.finish().unwrap_or_else(|e| {
        panic!("seed {seed} (release={release}) zero leaks tier1: {e}\n{source}")
    });
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn generated_programs_match_tier0_outcome_and_value_with_zero_leaks() {
    let mut compared = 0usize;
    let mut rng_state: u64 = 0x9740_cafe_1234_5678;
    let mut next_u32 = || {
        rng_state = rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from(rng_state >> 32).expect("shifted u64 fits u32")
    };
    let entry_path = ModelPath::new("Fixture", "program");
    for seed in 0..400 {
        let len = (next_u32() as usize % 40) + 6;
        let choices: Vec<u32> = (0..len).map(|_| next_u32()).collect();
        let p = generate::program(&choices);
        if !generate::runs(&p) || matches!(&p.scheme.body, Type::Eff(..)) {
            continue;
        }
        let is_unit_fn =
            matches!(&p.scheme.body, Type::Fun(u, _, _) if **u == Type::con("Prelude", "Unit"));
        let source = generate::source(&p);
        let Ok(modules) = run::compile_source(&source) else {
            continue;
        };
        for release in [false, true] {
            let Ok(counted) = run::counted_main(&modules, &entry_path, release) else {
                continue;
            };
            if !is_tier1_subset(&counted.program, &counted.entry, counted.root.as_ref()) {
                continue;
            }
            compare_generated_profile(&counted, is_unit_fn, seed, release, &source, 100_000);
            compared += 1;
        }
    }
    assert!(
        compared >= 40,
        "expected >= 40 generated differential runs, got {compared}"
    );
}

#[test]
fn generated_handler_programs_match_tier0_outcome_and_value_with_zero_leaks() {
    let mut rng: u64 = 0x9750_efec_cafe_babe;
    let mut next = || -> usize {
        rng = rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(rng >> 33).expect("fits usize")
    };
    let entry_path = ModelPath::new("App.GenHandlers", "main");
    let mut compared = 0usize;
    for seed in 0..40 {
        let (ask_out, ask_in, ab_val, bias, ret_add) = (
            next() % 11 + 1,
            next() % 11 + 20,
            next() % 17 + 50,
            next() % 7 + 1,
            next() % 5 + 1,
        );
        let inner = match next() % 3 {
            0 => format!("Stop.stop {ab_val}"),
            1 => format!("let _ = Fail.fail (Boom {ask_in})\n            0"),
            _ => "let u = Ask.ask ()\n            u + Emit.emit u + Ask.ask ()".to_owned(),
        };
        let arm = if next() % 2 == 0 {
            format!("Platform.stdout_line (\"drop:\" ++ show x)\n        x + {bias}")
        } else {
            format!("resume (x + {bias}) + 1")
        };
        let source = format!(
            "module App.GenHandlers exposing (main)\ntype error Boom = Boom Int\neffect Ask =\n  ask : Unit ->{{}} Int\neffect Stop =\n  abort stop : forall a. Int ->{{}} a\neffect Emit =\n  control emit : Int ->{{}} Int\nmain : Unit ->{{Host.Console}} Unit\nlet main () =\n  let res =\n    try\n      try\n        let a =\n          try\n            {inner}\n          with\n          | Ask.ask () -> {ask_in}\n          | Stop.stop n -> n + 3\n          | return v -> v + {ret_add}\n        a + Ask.ask ()\n      with\n      | Emit.emit x ->\n        {arm}\n      | Boom n -> n + 100\n    with\n    | Ask.ask () -> {ask_out}\n  Platform.stdout_line (\"res=\" ++ show res)\n"
        );
        let modules = run::compile_source(&source)
            .unwrap_or_else(|e| panic!("seed {seed} compile failed: {e:?}\n{source}"));
        for release in [false, true] {
            let counted = run::counted_main(&modules, &entry_path, release)
                .unwrap_or_else(|e| panic!("seed {seed} lower failed: {e:?}\n{source}"));
            assert!(is_tier1_subset(
                &counted.program,
                &counted.entry,
                counted.root.as_ref()
            ));
            compare_generated_profile(&counted, true, seed, release, &source, 200_000);
            compared += 1;
        }
    }
    assert_eq!(compared, 80);
}

#[test]
fn cli_intrinsics_match_tier0_and_never_leak_across_fuel_prefixes() {
    let source = "\
module App.CliSweep exposing (main)

import Cli

plan_cmd : Cli.Command {Host.Console} none
let plan_cmd = cli \"plan\" {
  description \"Plan work\"
  flag budget \"b\" : Int
  flag tag \"t\" : Option String
  arg items : List String
  handle fun { budget; tag; items } ->
    Platform.stdout_line (show budget ++ \":\" ++ show tag ++ \":\" ++ show items)
}

root_cmd : Cli.Command {Host.Console} none
let root_cmd = cli \"tool\" {
  description \"Demo CLI\"
  subcommand plan_cmd
}

main : Unit ->{Async, Host.Console, Host.Environment} Unit
let main () =
  let run_one args =
    try Cli.run_version \"1.2.3\" root_cmd args with
    | Cli.CliError msg -> Platform.stdout_line (\"ERR:\" ++ msg)
  run_one [\"plan\", \"--budget\", \"42\", \"-t\", \"fast\", \"a\", \"b\"]
  run_one [\"plan\", \"--budget\", \"nope\", \"a\"]
  run_one [\"--version\"]
  run_one [\"--help\"]
  run_one [\"--completions\", \"fish\"]
";
    let entry_path = ModelPath::new("App.CliSweep", "main");
    let modules = run::compile_source(source).expect("cli sweep compiles");
    for release in [false, true] {
        let counted = run::counted_main(&modules, &entry_path, release).expect("cli sweep lowers");
        assert!(
            is_tier1_subset(&counted.program, &counted.entry, counted.root.as_ref()),
            "cli sweep must be in tier1 subset (release={release})"
        );
        // Full run: tier 0 and tier 1 match stdout and leave zero leaks.
        let mut h0 = Heap::new();
        let r0 = counted.execute(&mut h0, 2_000_000);
        let mut h1 = Heap::new();
        let r1 = tier1::execute_main_program_outcome_with_host(
            &mut h1,
            &counted.program,
            &counted.entry,
            2_000_000,
            HostMode::Deterministic,
        )
        .expect("tier1 cli sweep prepares");
        assert_eq!(
            r0.stdout, r1.stdout,
            "cli stdout mismatch (release={release})"
        );
        assert!(
            r1.stdout.contains("42:Some \"fast\":[\"a\", \"b\"]")
                && r1.stdout.contains("ERR:Invalid value for flag --budget")
                && r1.stdout.contains("tool v1.2.3")
                && r1.stdout.contains("complete -c tool"),
            "unexpected cli sweep stdout:\n{}",
            r1.stdout
        );
        r0.outcome.release(&mut h0).expect("release t0");
        h0.finish().expect("zero leaks t0");
        r1.outcome.release(&mut h1).expect("release t1");
        let stats = h1.finish().expect("zero leaks t1");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);

        // Fuel-prefix sweep: OutOfFuel unwinds CliRunState with zero leaks on both tiers.
        for fuel in [50_u64, 200, 600, 1_500, 4_000, 12_000] {
            let mut ph0 = Heap::new();
            let pr0 = counted.execute(&mut ph0, fuel);
            pr0.outcome.release(&mut ph0).expect("prefix release t0");
            ph0.finish()
                .unwrap_or_else(|e| panic!("tier0 leak at fuel={fuel} (release={release}): {e}"));

            let mut ph1 = Heap::new();
            let pr1 = tier1::execute_main_program_outcome_with_host(
                &mut ph1,
                &counted.program,
                &counted.entry,
                fuel,
                HostMode::Deterministic,
            )
            .expect("prefix prepare t1");
            pr1.outcome.release(&mut ph1).expect("prefix release t1");
            ph1.finish()
                .unwrap_or_else(|e| panic!("tier1 leak at fuel={fuel} (release={release}): {e}"));
        }
    }
}
