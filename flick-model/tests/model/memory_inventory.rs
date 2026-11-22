//! Live counted-execution coverage inventory across the repository corpora.
//!
//! Runs every discovered program in both debug and release profiles through
//! `flick-core` → `perceus` → `flick-model::memory` without substitution
//! fallback, verifying exact expected outcomes where pinned and asserting
//! zero occupied heap slots (`Heap::finish`) on every executed run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{env, fs};

use flick_core::diagnostics::Severity;
use flick_core::project;
use flick_core::source::SourceMap;
use flick_model::memory::{
    Error as MemoryError, ExecutionReport, Heap, HostBridge, HostMode, ProgramOutcome, Value,
};
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;

mod examples;
mod helpers;
use helpers::{FsHost, compile_self_spawn_entry, describe_outcome_detail, load_doc_expectations};

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
            if env::var("FLICK_INVENTORY_DIFF").is_ok() {
                for item in &self.failed_outcome {
                    println!("  [FAIL]: {item}");
                }
            }
            let mut by_reason: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for item in &self.failed_outcome {
                let (name, rest) = item.split_once(": ").unwrap_or((item, ""));
                let key = rest.split(" expected=").next().unwrap_or(rest).to_owned();
                by_reason.entry(key).or_default().push(name.to_owned());
            }
            let mut ranked_fail: Vec<_> = by_reason.into_iter().collect();
            ranked_fail.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
            println!(
                "  [failed_outcome grouped ({} total)]:",
                self.failed_outcome.len()
            );
            for (reason, names) in ranked_fail {
                let sample: Vec<_> = names.iter().take(6).cloned().collect();
                let more = if names.len() > 6 {
                    format!(" ... +{}", names.len() - 6)
                } else {
                    String::new()
                };
                println!(
                    "    {:4}  {reason}  [{}]{more}",
                    names.len(),
                    sample.join(", ")
                );
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
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
) {
    check_file_fixture_with_expect(path, label, check_stdout, None, fuel, bridge, summary);
}

fn check_doc_compile_fail(path: &Path, label: &str, fail_code: &str, summary: &mut CorpusSummary) {
    summary.total_profiles += 2;
    match run::compile(path) {
        Err(err) if err.contains(fail_code) => summary.expected_failure += 2,
        Err(err) => {
            summary.failed_outcome.push(format!(
                "{label}: expected compile error {fail_code}, got {err}"
            ));
            summary.failed_outcome.push(format!(
                "{label} (release): expected compile error {fail_code}, got {err}"
            ));
        }
        Ok(_) => {
            summary.failed_outcome.push(format!(
                "{label}: expected compile error {fail_code}, but compiled cleanly"
            ));
            summary.failed_outcome.push(format!(
                "{label} (release): expected compile error {fail_code}, but compiled cleanly"
            ));
        }
    }
}

fn check_file_fixture_with_expect(
    path: &Path,
    label: &str,
    check_stdout: bool,
    doc_expect: Option<&str>,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
) {
    if let Some(fail_code) = doc_expect.and_then(|e| e.strip_prefix("fail ")) {
        check_doc_compile_fail(path, label, fail_code, summary);
        return;
    }
    let expected_stdout = if check_stdout {
        fs::read_to_string(path.with_extension("stdout")).ok()
    } else {
        None
    };
    let fetch_mock: Option<serde_json::Value> =
        fs::read_to_string(path.with_extension("fetch.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
    let expects_nonzero_exit = doc_expect == Some("ui")
        || fs::read_to_string(path.with_extension("exit"))
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
        let counted = match run::counted_main_with_bridge(&modules, &main, release) {
            Ok(counted) => counted,
            Err(error) => {
                summary.record_unsupported(&error);
                continue;
            }
        };
        if let Some(replies) = &fetch_mock {
            bridge
                .set_fetch_mock(Some(replies))
                .expect("install fetch mock");
        }
        let self_spawn = compile_self_spawn_entry(&repo(), path, label, &main.module, release);
        if let Some(script) = &self_spawn {
            bridge
                .set_entry_script(Some(script))
                .expect("install self-spawn entry script");
        }
        let started = Instant::now();
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, bridge);
        let elapsed = started.elapsed();
        if elapsed.as_secs() >= 4 {
            eprintln!(
                "[slow] {label} ({profile}) took {:.1}s",
                elapsed.as_secs_f64()
            );
        }
        if matches!(
            &report.outcome,
            ProgramOutcome::Fault(MemoryError::HostBridge(_))
        ) {
            *bridge = HostBridge::new().expect("respawn HostBridge after bridge fault");
        }
        record_fixture_report(
            &mut heap,
            report,
            label,
            profile,
            expected_stdout.as_deref(),
            expects_nonzero_exit,
            summary,
        );
    }
}

fn record_fixture_report(
    heap: &mut Heap,
    report: ExecutionReport,
    label: &str,
    profile: &str,
    expected_stdout: Option<&str>,
    expects_nonzero_exit: bool,
    summary: &mut CorpusSummary,
) {
    let outcome_tag = match &report.outcome {
        ProgramOutcome::Returned(Value::Unit) => "returned_unit",
        ProgramOutcome::Returned(_) => "returned_non_unit",
        ProgramOutcome::Raised(_) => "raised",
        ProgramOutcome::Failed(_) => "failed",
        ProgramOutcome::Fault(MemoryError::OutOfFuel) => "out_of_fuel",
        ProgramOutcome::Fault(_) => "fault",
    };
    let fault_detail = describe_outcome_detail(heap, &report.outcome);
    let release_err = report.outcome.release(heap).err();
    let finish_res = heap.finish();
    if let Some(err) = release_err {
        summary
            .leaked
            .push(format!("{label} ({profile}): release error: {err}"));
        return;
    }
    match finish_res {
        Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {}
        Ok(stats) => {
            summary.leaked.push(format!(
                "{label} ({profile}): live={} alloc={} free={}",
                stats.live_objects, stats.allocations, stats.frees
            ));
            return;
        }
        Err(err) => {
            summary.leaked.push(format!("{label} ({profile}): {err}"));
            return;
        }
    }
    if outcome_tag == "out_of_fuel" {
        summary.out_of_fuel.push(format!("{label} ({profile})"));
        return;
    }
    if let Some(expected) = expected_stdout
        && report.stdout != expected
    {
        summary.failed_outcome.push(format!(
            "{label} ({profile}): stdout mismatch ({outcome_tag}{}) expected={expected:?} actual={:?}",
            fault_detail.map_or(String::new(), |d| format!(": {d}")),
            report.stdout,
        ));
        return;
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

fn check_language_exports(fuel: u64, bridge: &mut HostBridge, summary: &mut CorpusSummary) {
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
            sql_dialect: "postgres",
            ..project::Config::default()
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
            let counted = match run::counted_main_with_bridge(&compilation.modules, &entry, release)
            {
                Ok(counted) => counted,
                Err(error) => {
                    summary.record_unsupported(&error);
                    continue;
                }
            };
            let mut heap = Heap::new();
            let report = counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, bridge);
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
    let filter = env::var("FLICK_INVENTORY_FILTER").ok();
    let matches_filter = |name: &str| {
        filter.as_deref().is_none_or(|f| {
            f.split(',')
                .any(|pat| !pat.is_empty() && name.contains(pat))
        })
    };
    let doc_expects = load_doc_expectations(&root);
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
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
        if !matches_filter(&rel) {
            continue;
        }
        check_file_fixture(path, &rel, true, fuel, &mut bridge, &mut run_summary);
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
        if !matches_filter(&rel) {
            continue;
        }
        check_file_fixture(path, &rel, true, fuel, &mut bridge, &mut rt_summary);
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
        if !matches_filter(&rel) {
            continue;
        }
        let exp = doc_expects.get(&rel).map(String::as_str);
        check_file_fixture_with_expect(path, &rel, true, exp, fuel, &mut bridge, &mut docs_summary);
    }
    docs_summary.print("docs (tour + exercise solutions + playground examples)");

    let mut lang_summary = CorpusSummary::default();
    if filter.is_none() {
        check_language_exports(fuel, &mut bridge, &mut lang_summary);
    }
    lang_summary.print("tests/language exports");

    let mut examples_summary = CorpusSummary::default();
    examples::check_examples_corpus(
        &root,
        fuel,
        &mut bridge,
        &mut examples_summary,
        &matches_filter,
    );
    examples_summary.print("examples (runnable mains + test suites)");

    assert_inventory_gate(
        filter.is_some(),
        &[
            ("tests/run", &run_summary),
            ("tests/rt", &rt_summary),
            ("docs", &docs_summary),
            ("tests/language", &lang_summary),
            ("examples", &examples_summary),
        ],
    );
}

fn assert_inventory_gate(filtered: bool, corpora: &[(&str, &CorpusSummary)]) {
    let total_leaks: usize = corpora.iter().map(|(_, s)| s.leaked.len()).sum();
    assert_eq!(
        total_leaks, 0,
        "counted execution must never leak heap slots"
    );
    if !cfg!(debug_assertions) && !filtered {
        for (name, s) in corpora {
            assert!(
                s.failed_outcome.is_empty()
                    && s.out_of_fuel.is_empty()
                    && s.unsupported.is_empty()
                    && s.compile_error == 0
                    && s.no_main == 0,
                "{name} release corpus gate failed: failed={:?} oof={:?} unsup={:?} err={} nomain={}",
                s.failed_outcome,
                s.out_of_fuel,
                s.unsupported,
                s.compile_error,
                s.no_main
            );
        }
    }
}
