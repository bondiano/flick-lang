//! Tier-0 vs Tier-1 closed-world differential corpus checks across all five
//! inventory corpora (`tests/run`, `tests/rt`, `docs`, `tests/language`,
//! `examples/`).

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::project;
use flick_core::source::SourceMap;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;
use flick_vm::tier0::{Heap, HostMode};
use flick_vm::tier1;

use super::helpers::{FsHost, describe_outcome_detail};
use super::{CorpusSummary, DEBUG_FUEL, examples, outcome_tag, repo};
use crate::memory_tier1_diff::is_tier1_subset;

pub(super) fn verify_tier1_main_diff(
    closed: &run::CountedMain,
    label: &str,
    profile: &str,
    fuel: u64,
    summary: &mut CorpusSummary,
) {
    summary.tier1_checked += 1;
    let mut heap0 = Heap::new();
    let report0 = closed.execute_with_host(&mut heap0, fuel, HostMode::Deterministic);
    let tag0 = outcome_tag(&report0.outcome);
    if let Err(err) = report0.outcome.release(&mut heap0) {
        summary.leaked.push(format!(
            "{label} ({profile}) [tier0-closed]: release error: {err}"
        ));
        let _ = heap0.finish();
        return;
    }
    match heap0.finish() {
        Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {}
        Ok(stats) => {
            summary.leaked.push(format!(
                "{label} ({profile}) [tier0-closed]: live={} alloc={} free={}",
                stats.live_objects, stats.allocations, stats.frees
            ));
            return;
        }
        Err(err) => {
            summary
                .leaked
                .push(format!("{label} ({profile}) [tier0-closed]: {err}"));
            return;
        }
    }

    let mut heap1 = Heap::new();
    let res1 = match &closed.root {
        Some(root) => tier1::execute_rooted_main_program_outcome_with_args(
            &mut heap1,
            &closed.program,
            &closed.entry,
            root,
            fuel,
            HostMode::Deterministic,
            Vec::new(),
        ),
        None => tier1::execute_main_program_outcome_with_args(
            &mut heap1,
            &closed.program,
            &closed.entry,
            fuel,
            HostMode::Deterministic,
            Vec::new(),
        ),
    };
    let report1 = match res1 {
        Ok(r) => r,
        Err(err) => {
            let _ = heap1.finish();
            summary.failed_outcome.push(format!(
                "{label} ({profile}) [tier1]: prepare failed: {err}"
            ));
            return;
        }
    };
    let tag1 = outcome_tag(&report1.outcome);
    let detail1 = describe_outcome_detail(&heap1, &report1.outcome);
    if let Err(err) = report1.outcome.release(&mut heap1) {
        summary
            .leaked
            .push(format!("{label} ({profile}) [tier1]: release error: {err}"));
        let _ = heap1.finish();
        return;
    }
    match heap1.finish() {
        Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {}
        Ok(stats) => {
            summary.leaked.push(format!(
                "{label} ({profile}) [tier1]: live={} alloc={} free={}",
                stats.live_objects, stats.allocations, stats.frees
            ));
            return;
        }
        Err(err) => {
            summary
                .leaked
                .push(format!("{label} ({profile}) [tier1]: {err}"));
            return;
        }
    }
    if tag0 != "out_of_fuel" && tag1 != "out_of_fuel" {
        if tag0 != tag1 {
            summary.failed_outcome.push(format!(
                "{label} ({profile}) [tier1]: outcome mismatch tier0={tag0} tier1={tag1}{}",
                detail1.map_or(String::new(), |d| format!(" ({d})"))
            ));
        } else if report1.stdout != report0.stdout {
            summary.failed_outcome.push(format!(
                "{label} ({profile}) [tier1]: stdout mismatch tier0={:?} tier1={:?}",
                report0.stdout, report1.stdout
            ));
        }
    }
}

fn check_tier1_file_fixture(path: &Path, label: &str, fuel: u64, summary: &mut CorpusSummary) {
    let Ok(modules) = run::compile(path) else {
        return;
    };
    let Ok(main) = run::main_of(&modules) else {
        return;
    };
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        if let Ok(closed) = run::counted_main(&modules, &main, release)
            && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
        {
            summary.total_profiles += 1;
            verify_tier1_main_diff(&closed, label, profile, fuel, summary);
        }
    }
}

fn check_tier1_language_suites(fuel: u64, summary: &mut CorpusSummary) {
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
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        summary.compile_error += 1;
        return;
    }
    for module in &compilation.modules {
        if !module.interface.exports_value("tests") {
            continue;
        }
        let entry = ModelPath::new(&module.name, "tests");
        for release in [false, true] {
            let profile = if release { "release" } else { "debug" };
            if let Ok(closed) = run::counted_main(&compilation.modules, &entry, release)
                && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
            {
                summary.total_profiles += 1;
                examples::verify_tier1_tests_diff(&closed, &module.name, profile, fuel, summary);
            }
        }
    }
}

#[test]
#[ignore = "full tier 0 vs tier 1 differential corpus check across all five corpora"]
fn tier1_full_corpus_inventory() {
    let fuel: u64 = if cfg!(debug_assertions) {
        DEBUG_FUEL
    } else {
        20_000_000
    };
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
        check_tier1_file_fixture(path, &rel, fuel, &mut run_summary);
    }
    run_summary.print("tier1 tests/run");

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
        check_tier1_file_fixture(path, &rel, fuel, &mut rt_summary);
    }
    rt_summary.print("tier1 tests/rt");

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
        check_tier1_file_fixture(path, &rel, fuel, &mut docs_summary);
    }
    docs_summary.print("tier1 docs");

    let mut lang_summary = CorpusSummary::default();
    check_tier1_language_suites(fuel, &mut lang_summary);
    lang_summary.print("tier1 tests/language");

    let mut examples_summary = CorpusSummary::default();
    examples::check_examples_tier1_corpus(&root, fuel, &mut examples_summary);
    examples_summary.print("tier1 examples");

    for (name, s) in [
        ("tests/run", &run_summary),
        ("tests/rt", &rt_summary),
        ("docs", &docs_summary),
        ("tests/language", &lang_summary),
        ("examples", &examples_summary),
    ] {
        assert!(
            s.tier1_checked > 0,
            "{name}: zero tier1 differential profiles checked"
        );
        assert!(
            s.leaked.is_empty(),
            "{name}: heap leaks detected: {:?}",
            s.leaked
        );
        assert!(
            s.failed_outcome.is_empty() && s.compile_error == 0,
            "{name}: tier0/tier1 diff failures: {:?} (compile_error={})",
            s.failed_outcome,
            s.compile_error
        );
    }
}
