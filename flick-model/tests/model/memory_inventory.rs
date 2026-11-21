//! Live counted-execution coverage inventory across the repository corpora.
//!
//! Runs every discovered program in both debug and release profiles through
//! `flick-core` → `perceus` → `flick-model::memory` without substitution
//! fallback, verifying exact expected outcomes where pinned and asserting
//! zero occupied heap slots (`Heap::finish`) on every executed run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use flick_core::diagnostics::Severity;
use flick_core::project;
use flick_core::source::SourceMap;
use flick_core::types::Thresholds;
use flick_model::memory::{Error as MemoryError, Heap, ProgramOutcome, Value};
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;

struct FsHost;

impl project::Host for FsHost {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

/// Fuel cap for debug `just model` runs so the inventory finishes quickly while
/// still executing every corpus program and verifying zero leaks on `OutOfFuel`.
const DEBUG_FUEL: u64 = 1_000_000;
/// Full fuel budget under `--release` (or via `FLICK_COUNTED_FUEL`) so 1M–10M
/// deep recursion and deep control corpus fixtures run to completion.
const RELEASE_FUEL: u64 = 2_000_000_000;

fn inventory_fuel() -> u64 {
    if let Ok(raw) = env::var("FLICK_COUNTED_FUEL")
        && let Ok(parsed) = raw.parse::<u64>()
    {
        return parsed;
    }
    if cfg!(debug_assertions) {
        DEBUG_FUEL
    } else {
        RELEASE_FUEL
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[derive(Default)]
struct CorpusSummary {
    passed: usize,
    expected_failure: usize,
    failed_outcome: Vec<String>,
    out_of_fuel: Vec<String>,
    leaked: Vec<String>,
    unsupported: BTreeMap<String, usize>,
    compile_error: usize,
    no_main: usize,
    total_profiles: usize,
}

impl CorpusSummary {
    fn record_unsupported(&mut self, error: &str) {
        // Normalize span suffix so identical lowering gaps aggregate cleanly.
        let key = error
            .split(" at Span")
            .next()
            .unwrap_or(error)
            .lines()
            .next()
            .unwrap_or(error)
            .to_owned();
        *self.unsupported.entry(key).or_default() += 1;
    }

    fn print(&self, title: &str) {
        let unsupported_total: usize = self.unsupported.values().sum();
        println!("=== {title} (profiles={}) ===", self.total_profiles);
        println!(
            "  passed={} expected_failure={} out_of_fuel={} failed_outcome={} leaked={} unsupported={} compile_error={} no_main={}",
            self.passed,
            self.expected_failure,
            self.out_of_fuel.len(),
            self.failed_outcome.len(),
            self.leaked.len(),
            unsupported_total,
            self.compile_error,
            self.no_main,
        );
        if !self.out_of_fuel.is_empty() {
            println!("  [out_of_fuel]: {}", self.out_of_fuel.join(", "));
        }
        if !self.failed_outcome.is_empty() {
            for item in self.failed_outcome.iter().take(20) {
                println!("  [failed_outcome]: {item}");
            }
            if self.failed_outcome.len() > 20 {
                println!("  ... and {} more", self.failed_outcome.len() - 20);
            }
        }
        if !self.leaked.is_empty() {
            for item in &self.leaked {
                println!("  [LEAK]: {item}");
            }
        }
        let mut ranked: Vec<_> = self.unsupported.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if !ranked.is_empty() {
            println!("  [top unsupported reasons]:");
            for (reason, count) in ranked.into_iter().take(25) {
                println!("    {count:4}  {reason}");
            }
        }
    }
}

fn check_file_fixture(
    path: &Path,
    label: &str,
    check_stdout: bool,
    fuel: u64,
    summary: &mut CorpusSummary,
) {
    let expected_stdout = if check_stdout {
        fs::read_to_string(path.with_extension("stdout")).ok()
    } else {
        None
    };
    let expects_nonzero_exit = fs::read_to_string(path.with_extension("exit"))
        .ok()
        .is_some_and(|code| code.trim() != "0");

    let Ok(modules) = run::compile(path) else {
        summary.total_profiles += 2;
        summary.compile_error += 2;
        return;
    };
    let Ok(main) = run::main_of(&modules) else {
        summary.total_profiles += 2;
        summary.no_main += 2;
        return;
    };

    for release in [false, true] {
        summary.total_profiles += 1;
        let profile = if release { "release" } else { "debug" };
        let counted = match run::counted_main(&modules, &main, release) {
            Ok(counted) => counted,
            Err(error) => {
                summary.record_unsupported(&error);
                continue;
            }
        };
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let outcome_tag = match &report.outcome {
            ProgramOutcome::Returned(Value::Unit) => "returned_unit",
            ProgramOutcome::Returned(_) => "returned_non_unit",
            ProgramOutcome::Raised(_) => "raised",
            ProgramOutcome::Failed(_) => "failed",
            ProgramOutcome::Fault(MemoryError::OutOfFuel) => "out_of_fuel",
            ProgramOutcome::Fault(_) => "fault",
        };
        let fault_detail = match &report.outcome {
            ProgramOutcome::Fault(err) => Some(err.to_string()),
            _ => None,
        };
        let release_err = report.outcome.release(&mut heap).err();
        let finish_res = heap.finish();
        if let Some(err) = release_err {
            summary
                .leaked
                .push(format!("{label} ({profile}): release error: {err}"));
            continue;
        }
        match finish_res {
            Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {}
            Ok(stats) => {
                summary.leaked.push(format!(
                    "{label} ({profile}): live={} alloc={} free={}",
                    stats.live_objects, stats.allocations, stats.frees
                ));
                continue;
            }
            Err(err) => {
                summary.leaked.push(format!("{label} ({profile}): {err}"));
                continue;
            }
        }

        if outcome_tag == "out_of_fuel" {
            summary.out_of_fuel.push(format!("{label} ({profile})"));
            continue;
        }
        if let Some(expected) = &expected_stdout
            && report.stdout != *expected
        {
            summary.failed_outcome.push(format!(
                "{label} ({profile}): stdout mismatch ({outcome_tag}{}) expected={expected:?} actual={:?}",
                fault_detail.map_or(String::new(), |d| format!(": {d}")),
                report.stdout,
            ));
            continue;
        }
        match outcome_tag {
            "returned_unit" if !expects_nonzero_exit => summary.passed += 1,
            "raised" | "failed" if expects_nonzero_exit => summary.expected_failure += 1,
            _ => summary.failed_outcome.push(format!(
                "{label} ({profile}): unexpected outcome {outcome_tag}{}",
                fault_detail.map_or(String::new(), |d| format!(": {d}"))
            )),
        }
    }
}

fn check_language_exports(fuel: u64, summary: &mut CorpusSummary) {
    let dir = repo().join("tests/language");
    let mut test_files = fixture_files(&dir.join("test"), "fl");
    test_files.sort();
    let mut roots: Vec<PathBuf> = fixture_files(&dir.join("src"), "fl");
    roots.extend(test_files.iter().cloned());
    roots.sort();
    let canonical: Vec<PathBuf> = roots
        .into_iter()
        .map(|p| project::normalize(&fs::canonicalize(&p).expect("canonical path")))
        .collect();
    let dir = project::normalize(&fs::canonicalize(&dir).expect("canonical language dir"));
    let mut sources = SourceMap::default();
    let config_file = sources.add(dir.join("flick.toml").display().to_string(), "");
    let project = project::Project {
        root: dir,
        config: project::Config {
            kind: project::config::Kind::App,
            source_dir: "src".into(),
            test_dir: "test".into(),
            proof_dir: "proofs".into(),
            entries: vec![],
            dependencies: vec![],
            js_dependencies: vec![],
            package: None,
            build: project::config::Build::default(),
            targets: BTreeMap::new(),
            sql_dialect: "postgres",
            allow: vec![],
            no_let_assert: false,
            thresholds: Thresholds::default(),
            plugins: BTreeMap::new(),
            tools: BTreeMap::new(),
        },
        config_file,
        workspace: None,
    };
    let (diagnostics, compilation) =
        project::compile(&FsHost, &mut sources, Some(&project), &canonical);
    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    if has_errors {
        summary.total_profiles += test_files.len() * 2;
        summary.compile_error += test_files.len() * 2;
        return;
    }
    for module in &compilation.modules {
        if !module.interface.exports_value("tests") {
            continue;
        }
        let entry = ModelPath::new(&module.name, "tests");
        for release in [false, true] {
            summary.total_profiles += 1;
            let profile = if release { "release" } else { "debug" };
            let counted = match run::counted_main(&compilation.modules, &entry, release) {
                Ok(counted) => counted,
                Err(error) => {
                    summary.record_unsupported(&error);
                    continue;
                }
            };
            let mut heap = Heap::new();
            let report = counted.execute(&mut heap, fuel);
            let is_ok = matches!(report.outcome, ProgramOutcome::Returned(_));
            let is_oof = matches!(
                report.outcome,
                ProgramOutcome::Fault(MemoryError::OutOfFuel)
            );
            if let Err(err) = report.outcome.release(&mut heap) {
                summary
                    .leaked
                    .push(format!("{} ({profile}): release error: {err}", module.name));
                continue;
            }
            if let Err(err) = heap.finish() {
                summary
                    .leaked
                    .push(format!("{} ({profile}): {err}", module.name));
                continue;
            }
            if is_ok {
                summary.passed += 1;
            } else if is_oof {
                summary
                    .out_of_fuel
                    .push(format!("{} ({profile})", module.name));
            } else {
                summary.failed_outcome.push(format!(
                    "{} ({profile}): failed evaluating tests list",
                    module.name
                ));
            }
        }
    }
}

#[test]
#[ignore = "full counted corpus coverage inventory across all corpora and profiles"]
fn counted_corpus_inventory() {
    let fuel = inventory_fuel();
    let root = repo();
    let mut run_summary = CorpusSummary::default();
    let mut run_files = fixture_files(&root.join("tests/run"), "fl");
    run_files.sort();
    for path in &run_files {
        let rel = path
            .strip_prefix(root.join("tests/run"))
            .unwrap_or(path)
            .with_extension("")
            .display()
            .to_string();
        check_file_fixture(path, &rel, true, fuel, &mut run_summary);
    }
    run_summary.print("tests/run");

    let mut rt_summary = CorpusSummary::default();
    let mut rt_files = fixture_files(&root.join("tests/rt"), "fl");
    rt_files.sort();
    for path in &rt_files {
        let rel = path
            .strip_prefix(root.join("tests/rt"))
            .unwrap_or(path)
            .with_extension("")
            .display()
            .to_string();
        check_file_fixture(path, &rel, true, fuel, &mut rt_summary);
    }
    rt_summary.print("tests/rt");

    let mut docs_summary = CorpusSummary::default();
    let mut doc_files = Vec::new();
    doc_files.extend(fixture_files(&root.join("docs/tour"), "fl"));
    doc_files.extend(
        fixture_files(&root.join("docs/exercises"), "fl")
            .into_iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "solution.fl")),
    );
    doc_files.extend(fixture_files(
        &root.join("docs/src/playground/examples"),
        "fl",
    ));
    doc_files.sort();
    for path in &doc_files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        check_file_fixture(path, &rel, false, fuel, &mut docs_summary);
    }
    docs_summary.print("docs (tour + exercise solutions + playground examples)");

    let mut lang_summary = CorpusSummary::default();
    check_language_exports(fuel, &mut lang_summary);
    lang_summary.print("tests/language exports");

    let total_leaks = run_summary.leaked.len()
        + rt_summary.leaked.len()
        + docs_summary.leaked.len()
        + lang_summary.leaked.len();
    assert_eq!(
        total_leaks, 0,
        "counted execution must never leak heap slots"
    );
}
