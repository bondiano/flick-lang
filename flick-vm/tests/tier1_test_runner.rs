//! Tier-1 bytecode test-runner integration tests: all supported `Test.Test.$*`
//! combinators, virtual `TestClock`, seeded 100-draw properties, fuel limits,
//! and zero heap-slot / host-handle leaks.

use std::io;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::ir::memory::{Name, Program};
use flick_core::opt::perceus;
use flick_core::source::SourceMap;
use flick_core::{build, project};
use flick_vm::tier0::{self, Error, Heap, HostMode};
use flick_vm::tier1;

struct MemHost<'a>(&'a str);

impl project::Host for MemHost<'_> {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        if p == Path::new("/Suite.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn lower_suite(src: &str, entry: &Name) -> (Program, Option<Name>) {
    let mut sources = SourceMap::default();
    let (diags, comp) = project::compile(
        &MemHost(src),
        &mut sources,
        None,
        &[PathBuf::from("/Suite.fl")],
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
    let root = Name::new("Host", "live");
    let program = perceus::lower_roots_with_declarations(modules.iter(), decls, &[entry, &root])
        .expect("perceus lower suite");
    (program, Some(root))
}

#[test]
fn tier1_test_runner_supports_all_combinators_clock_and_properties_matching_tier0() {
    let source = r#"
module App.RunnerTest exposing (tests)

import Clock
import Duration
import Gen
import Test

ms : Float -> Duration.Duration
let ms n = Result.with_default Duration.zero (Duration.millis n)

tests : List Test.Test
let tests = [
  Test.group "combinators" [
    Test.test "pure pass" (fun () -> Test.equal 4 (2 + 2)),
    Test.test "pure fail" (fun () -> Test.equal 4 5),
    Test.skip (Test.test "skipped fail" (fun () -> Test.equal 1 2)),
    Test.only (Test.test "only pass" (fun () -> Test.equal true true)),
    Test.serial (Test.test "serial pass" (fun () -> Test.equal "a" "a")),
    Test.timeout 5000 (Test.test "timeout pass" (fun () -> Test.equal 9 (3 * 3))),
    Test.effect_test "effect pass" (task {
      let n = await (task { 42 })
      Test.expect n |> Test.to.equal 42
    }),
    Test.clock_test "clock adjust and set_time" (task {
      let t0 = Clock.current_time_millis ()
      Test.adjust (ms 250)
      let t1 = Clock.current_time_millis ()
      Test.set_time 1000
      let t2 = Clock.current_time_millis ()
      Test.expect (t0, t1, t2) |> Test.to.equal (0, 250, 1000)
    }),
    Test.property "seeded 100-draw property" (Gen.int (-100) 100) (fun n ->
      Test.is_true "bounded" (n >= -100 && n <= 100)
    ),
    Test.property "failing seeded property" (Gen.int 0 1000) (fun n ->
      Test.is_true "must be tiny" (n < 10)
    ),
  ],
]
"#;
    let entry = Name::new("App.RunnerTest", "tests");
    let (program, root) = lower_suite(source, &entry);

    let mut heap0 = Heap::new();
    let r0 = tier0::execute_tests_program(
        &mut heap0,
        &program,
        &entry,
        root.as_ref(),
        42,
        2_000_000,
        &[],
    )
    .expect("tier0 runner");
    let stats0 = heap0.finish().expect("tier0 zero leaks");
    assert_eq!(stats0.live_objects, 0);

    let mut heap1 = Heap::new();
    let r1 = tier1::execute_tests_program(
        &mut heap1,
        &program,
        &entry,
        root.as_ref(),
        42,
        2_000_000,
        &[],
    )
    .expect("tier1 prepare")
    .expect("tier1 runner");
    let stats1 = heap1.finish().expect("tier1 zero leaks");
    assert_eq!(stats1.live_objects, 0);
    assert_eq!(stats1.allocations, stats1.frees);

    assert_eq!(r0, r1, "tier0 and tier1 test results must match exactly");
    assert_eq!(r1.len(), 10);
    assert!(r1[0].passed);
    assert!(!r1[1].passed);
    assert!(r1[2].skipped);
    assert!(r1[3].passed);
    assert!(r1[4].passed);
    assert!(r1[5].passed);
    assert!(r1[6].passed);
    assert!(r1[7].passed);
    assert!(r1[8].passed);
    assert!(!r1[9].passed);
    assert!(
        r1[9]
            .message
            .as_deref()
            .is_some_and(|m| m.contains("(case "))
    );
}

#[test]
fn tier1_test_runner_enforces_fuel_limits_with_zero_leaks() {
    let source = r#"
module App.FuelTest exposing (tests)

import Test

spin : Int -> Int
let rec spin n = if n == 0 then 0 else spin (n - 1)

tests : List Test.Test
let tests = [
  Test.test "exhausts fuel" (fun () -> Test.equal 0 (spin 50000)),
]
"#;
    let entry = Name::new("App.FuelTest", "tests");
    let (program, root) = lower_suite(source, &entry);

    for fuel in [50_u64, 500, 2_000] {
        let mut heap = Heap::new();
        let res = tier1::execute_tests_program_with_host(
            &mut heap,
            &program,
            &entry,
            root.as_ref(),
            42,
            fuel,
            &[],
            HostMode::Deterministic,
        )
        .expect("tier1 prepare");
        assert!(
            matches!(res, Err(Error::OutOfFuel)),
            "expected OutOfFuel at fuel={fuel}, got {res:?}"
        );
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("tier1 leaked on OutOfFuel at fuel={fuel}: {e}"));
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}
