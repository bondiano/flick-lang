//! Counted-execution conformance runner for `tests/language/test/Language/*.fl`.
//!
//! Runs every unlocked language test suite in both debug and optimized IR
//! profiles under `--seed 42`, asserting zero test failures and zero heap leaks
//! (`Heap::finish()` with `allocations == frees`).

use std::path::Path;
use std::slice;

use flick_core::ir::memory::Name as MemoryName;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_vm::tier0::{Heap, execute_tests_program};
use flick_vm::tier1;

const STAGE3_PURE_SUITES: &[&str] = &[
    "Adts",
    "Assertions",
    "BigInts",
    "Bytes",
    "Chunk",
    "ClassVariance",
    "Collections",
    "Effects",
    "Equality",
    "Floats",
    "Functions",
    "GadtRows",
    "Ints",
    "Json",
    "Loops",
    "Mutation",
    "NumericBoundaries",
    "Patterns",
    "Properties",
    "Records",
    "Schemas",
    "Show",
    "Strings",
    "Syntax",
    "Time",
];

#[test]
fn stage3_language_suites_pass_with_zero_leaks() {
    let lang_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/language");
    let mut files = vec![
        lang_root.join("src/Language/Shapes.fl"),
        lang_root.join("src/Language/ClassVarianceDefs.fl"),
    ];
    let suites: Vec<(&str, &[&str])> = STAGE3_PURE_SUITES.iter().map(|&s| (s, &[][..])).collect();

    for &(suite, _) in &suites {
        files.push(lang_root.join(format!("test/Language/{suite}.fl")));
    }
    let modules = run::compile_files(&files).expect("language suites compile");
    let entries: Vec<_> = suites
        .iter()
        .map(|&(s, _)| ModelPath::new(&format!("Language.{s}"), "tests"))
        .collect();

    let mut total = 0;
    for release in [false, true] {
        let (program, root) =
            run::counted_program(&modules, &entries, release).unwrap_or_else(|err| {
                panic!("language suites (release={release}) failed to lower: {err}")
            });
        let mut profile_total = 0;
        for &(suite, skip_groups) in &suites {
            let entry = MemoryName::new(&format!("Language.{suite}"), "tests");
            let mut heap = Heap::new();
            let results = execute_tests_program(
                &mut heap,
                &program,
                &entry,
                root.as_ref(),
                42,
                5_000_000,
                skip_groups,
            )
            .unwrap_or_else(|err| panic!("{suite} (release={release}) runner error: {err}"));
            let stats = heap
                .finish()
                .unwrap_or_else(|err| panic!("{suite} (release={release}) heap leak: {err}"));
            assert_eq!(
                stats.allocations, stats.frees,
                "{suite} (release={release}) allocation mismatch: {stats:?}"
            );

            let mut heap1 = Heap::new();
            let results1 = tier1::execute_tests_program(
                &mut heap1,
                &program,
                &entry,
                root.as_ref(),
                42,
                5_000_000,
                skip_groups,
            )
            .unwrap_or_else(|err| panic!("{suite} (release={release}) tier1 prepare error: {err}"))
            .unwrap_or_else(|err| panic!("{suite} (release={release}) tier1 runner error: {err}"));
            let stats1 = heap1
                .finish()
                .unwrap_or_else(|err| panic!("{suite} (release={release}) tier1 heap leak: {err}"));
            assert_eq!(stats1.live_objects, 0);
            assert_eq!(
                stats1.allocations, stats1.frees,
                "{suite} (release={release}) tier1 allocation mismatch: {stats1:?}"
            );
            assert_eq!(
                results, results1,
                "{suite} (release={release}) tier0 vs tier1 test results diverged"
            );
            let failures: Vec<_> = results
                .iter()
                .filter(|r| !r.passed)
                .map(|r| format!("{}: {}", r.name, r.message.as_deref().unwrap_or("")))
                .collect();
            assert!(
                failures.is_empty(),
                "{suite} (release={release}) had {} failing tests:\n{}",
                failures.len(),
                failures.join("\n")
            );
            let ran = results.iter().filter(|r| !r.skipped).count();
            assert!(ran > 0, "{suite} (release={release}) executed zero tests");
            profile_total += ran;
        }
        if !release {
            total = profile_total;
        }
    }
    assert!(
        total >= 300,
        "expected >= 300 counted language test cases, ran {total}"
    );
}

#[test]
#[ignore = "million-call tail-recursion stress suite; run with --release -- --ignored"]
fn stage3_language_recursion_stress_with_zero_leaks() {
    let lang_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/language");
    let files = vec![lang_root.join("test/Language/Recursion.fl")];
    let modules = run::compile_files(&files).expect("Recursion.fl compiles");
    let entry_path = ModelPath::new("Language.Recursion", "tests");
    let entry = MemoryName::new("Language.Recursion", "tests");
    for release in [false, true] {
        let (program, root) = run::counted_program(&modules, slice::from_ref(&entry_path), release)
            .unwrap_or_else(|err| panic!("Recursion (release={release}) failed to lower: {err}"));
        let mut heap = Heap::new();
        let results = execute_tests_program(
            &mut heap,
            &program,
            &entry,
            root.as_ref(),
            42,
            200_000_000,
            &[],
        )
        .unwrap_or_else(|err| panic!("Recursion (release={release}) runner error: {err}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|err| panic!("Recursion (release={release}) heap leak: {err}"));
        assert_eq!(stats.allocations, stats.frees);

        let mut heap1 = Heap::new();
        let results1 = tier1::execute_tests_program(
            &mut heap1,
            &program,
            &entry,
            root.as_ref(),
            42,
            200_000_000,
            &[],
        )
        .unwrap_or_else(|err| panic!("Recursion (release={release}) tier1 prepare error: {err}"))
        .unwrap_or_else(|err| panic!("Recursion (release={release}) tier1 runner error: {err}"));
        let stats1 = heap1
            .finish()
            .unwrap_or_else(|err| panic!("Recursion (release={release}) tier1 heap leak: {err}"));
        assert_eq!(stats1.live_objects, 0);
        assert_eq!(stats1.allocations, stats1.frees);
        assert_eq!(
            results, results1,
            "Recursion (release={release}) tier0 vs tier1 mismatch"
        );
        let failures: Vec<_> = results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| format!("{}: {}", r.name, r.message.as_deref().unwrap_or("")))
            .collect();
        assert!(
            failures.is_empty(),
            "Recursion (release={release}) failures:\n{}",
            failures.join("\n")
        );
        assert!(
            results.len() >= 8,
            "expected >= 8 Recursion cases, ran {}",
            results.len()
        );
    }
}
