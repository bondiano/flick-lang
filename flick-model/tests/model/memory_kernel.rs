//! Primitive kernel conformance suite executed against `flick-vm`'s tier-neutral
//! shared primitive library (`flick_vm::stdlib`), serving as a third verified
//! kernel implementation alongside JS (`rt` / `effect`) and Go.
//!
//! Runs:
//! 1. Every test module under `tests/kernel/test/Kernel/*.fl` in debug and release
//!    profiles (`Arrays`, `Cells`, `Hooks`, `Numbers`, `Records`, `Strings`, ...).
//! 2. Every `tests/language/test/Language/*.fl` module listed in
//!    `tests/kernel/language.txt` (`BigInts`, `Equality`, `FixedWidth`, `Floats`,
//!    `Ints`, `NumericBoundaries`, `Show`, `Strings`) in debug and release profiles.
//!
//! Every suite asserts zero test failures and zero heap leaks (`Heap::finish()`
//! with `allocations == frees`).

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::ir::memory::Name as MemoryName;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_vm::tier0::{Heap, execute_tests_program};

fn discover_kernel_suites(kernel_root: &Path) -> Vec<(String, PathBuf)> {
    let dir = kernel_root.join("test/Kernel");
    let mut suites = Vec::new();
    for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) == Some("fl") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("module stem")
                .to_owned();
            suites.push((stem, path));
        }
    }
    suites.sort_by(|a, b| a.0.cmp(&b.0));
    suites
}

fn kernel_language_suites(kernel_root: &Path) -> Vec<String> {
    let manifest = kernel_root.join("language.txt");
    let text = fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn run_suites_both_profiles(
    label: &str,
    support_files: &[PathBuf],
    module_prefix: &str,
    suites: &[(String, PathBuf)],
) -> usize {
    let mut files = support_files.to_vec();
    for (_, path) in suites {
        files.push(path.clone());
    }
    let modules = run::compile_files(&files)
        .unwrap_or_else(|err| panic!("{label} suites failed to compile:\n{err}"));
    let entries: Vec<_> = suites
        .iter()
        .map(|(name, _)| ModelPath::new(&format!("{module_prefix}.{name}"), "tests"))
        .collect();

    let mut debug_total = 0;
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let (program, root) = run::counted_program(&modules, &entries, release)
            .unwrap_or_else(|err| panic!("{label} ({profile}) failed to lower: {err}"));
        let mut profile_total = 0;
        for (name, _) in suites {
            let entry = MemoryName::new(&format!("{module_prefix}.{name}"), "tests");
            let mut heap = Heap::new();
            let results = execute_tests_program(
                &mut heap,
                &program,
                &entry,
                root.as_ref(),
                42,
                5_000_000,
                &[],
            )
            .unwrap_or_else(|err| panic!("{module_prefix}.{name} ({profile}) runner error: {err}"));
            let stats = heap.finish().unwrap_or_else(|err| {
                panic!("{module_prefix}.{name} ({profile}) heap leak: {err}")
            });
            assert_eq!(
                stats.allocations, stats.frees,
                "{module_prefix}.{name} ({profile}) allocation mismatch: {stats:?}"
            );
            let failures: Vec<_> = results
                .iter()
                .filter(|r| !r.passed)
                .map(|r| format!("{}: {}", r.name, r.message.as_deref().unwrap_or("")))
                .collect();
            assert!(
                failures.is_empty(),
                "{module_prefix}.{name} ({profile}) had {} failing tests:\n{}",
                failures.len(),
                failures.join("\n")
            );
            let ran = results.iter().filter(|r| !r.skipped).count();
            assert!(
                ran > 0,
                "{module_prefix}.{name} ({profile}) executed zero tests"
            );
            profile_total += ran;
        }
        if release {
            assert_eq!(
                debug_total, profile_total,
                "{label}: debug ({debug_total}) and release ({profile_total}) test counts diverged"
            );
        } else {
            debug_total = profile_total;
        }
    }
    debug_total
}

#[test]
fn vm_kernel_conformance_passes_with_zero_leaks() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let kernel_root = repo_root.join("tests/kernel");
    let lang_root = repo_root.join("tests/language");

    let kernel_suites = discover_kernel_suites(&kernel_root);
    assert!(
        kernel_suites.len() >= 6,
        "expected >= 6 kernel test modules under tests/kernel/test/Kernel, found {}",
        kernel_suites.len()
    );
    let kernel_cases = run_suites_both_profiles("tests/kernel", &[], "Kernel", &kernel_suites);

    let lang_names = kernel_language_suites(&kernel_root);
    assert!(
        lang_names.len() >= 8,
        "expected >= 8 language modules in tests/kernel/language.txt, found {}",
        lang_names.len()
    );
    let lang_suites: Vec<(String, PathBuf)> = lang_names
        .into_iter()
        .map(|name| {
            let path = lang_root.join(format!("test/Language/{name}.fl"));
            (name, path)
        })
        .collect();
    let lang_support = [
        lang_root.join("src/Language/Shapes.fl"),
        lang_root.join("src/Language/ClassVarianceDefs.fl"),
    ];
    let lang_cases = run_suites_both_profiles(
        "tests/kernel/language.txt",
        &lang_support,
        "Language",
        &lang_suites,
    );

    println!(
        "flick-vm kernel conformance: {kernel_cases} tests/kernel cases + {lang_cases} tests/language cases passed in both debug and release with 0 heap leaks"
    );
    assert!(
        kernel_cases >= 80,
        "expected >= 80 tests/kernel cases, ran {kernel_cases}"
    );
    assert!(
        lang_cases >= 100,
        "expected >= 100 kernel language cases, ran {lang_cases}"
    );
}
