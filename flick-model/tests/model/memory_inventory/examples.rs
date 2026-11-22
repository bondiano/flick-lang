//! Runnable `examples/` project coverage for `counted_corpus_inventory`.
//!
//! Compiles and executes self-contained multi-module example applications and
//! their `Test.Test` suites (including host-bridged suites like `cli-tool`'s
//! `App.WizardTest`) in both debug and release profiles with zero Rust heap
//! leaks and zero live host bridge handles.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::project::{self, Compiled};
use flick_core::source::SourceMap;
use flick_model::memory::{
    Error as MemoryError, Heap, HostBridge, HostMode, ProgramOutcome, Value,
    execute_tests_program_with_bridge,
};
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;

use super::CorpusSummary;
use super::helpers::FsHost;

fn compile_example_roots(project_dir: &Path, roots: Vec<PathBuf>) -> Result<Vec<Compiled>, String> {
    let mut canonical: Vec<PathBuf> = roots
        .into_iter()
        .map(|p| project::normalize(&fs::canonicalize(&p).expect("canonical example path")))
        .collect();
    canonical.sort();
    canonical.dedup();
    let dir = project::normalize(&fs::canonicalize(project_dir).expect("canonical example dir"));
    let mut sources = SourceMap::default();
    let config_file = sources.add(dir.join("flick.toml").display().to_string(), "");
    let proj = project::Project {
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
        project::compile(&FsHost, &mut sources, Some(&proj), &canonical);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    if errors.is_empty() {
        Ok(compilation.modules)
    } else {
        Err(errors.join("; "))
    }
}

fn run_example_main(
    modules: &[Compiled],
    entry: &ModelPath,
    label: &str,
    expected_stdout: Option<&str>,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
) {
    for release in [false, true] {
        summary.total_profiles += 1;
        let profile = if release { "release" } else { "debug" };
        let counted = match run::counted_main_with_bridge(modules, entry, release) {
            Ok(c) => c,
            Err(err) => {
                summary.record_unsupported(&err);
                continue;
            }
        };
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, bridge);
        if matches!(
            &report.outcome,
            ProgramOutcome::Fault(MemoryError::HostBridge(_))
        ) {
            *bridge = HostBridge::new().expect("respawn HostBridge after bridge fault");
        }
        let is_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let is_oof = matches!(
            report.outcome,
            ProgramOutcome::Fault(MemoryError::OutOfFuel)
        );
        let outcome_dbg = format!("{:?}", report.outcome);
        if let Err(err) = report.outcome.release(&mut heap) {
            summary
                .leaked
                .push(format!("{label} ({profile}): release error: {err}"));
            continue;
        }
        match heap.finish() {
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
        if is_oof {
            summary.out_of_fuel.push(format!("{label} ({profile})"));
            continue;
        }
        if let Some(expected) = expected_stdout
            && report.stdout != expected
        {
            summary.failed_outcome.push(format!(
                "{label} ({profile}): stdout mismatch expected={expected:?} actual={:?}",
                report.stdout
            ));
            continue;
        }
        if is_unit {
            summary.passed += 1;
        } else {
            summary
                .failed_outcome
                .push(format!("{label} ({profile}): unexpected {outcome_dbg}"));
        }
    }
}

fn run_example_tests(
    modules: &[Compiled],
    module_name: &str,
    label: &str,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
) {
    let entry = ModelPath::new(module_name, "tests");
    for release in [false, true] {
        summary.total_profiles += 1;
        let profile = if release { "release" } else { "debug" };
        let counted = match run::counted_main_with_bridge(modules, &entry, release) {
            Ok(c) => c,
            Err(err) => {
                summary.record_unsupported(&err);
                continue;
            }
        };
        let mut heap = Heap::new();
        let run_res = execute_tests_program_with_bridge(
            &mut heap,
            &counted.program,
            &counted.entry,
            counted.root.as_ref(),
            42,
            fuel,
            &[],
            HostMode::LiveOs,
            bridge,
        );
        if matches!(&run_res, Err(MemoryError::HostBridge(_))) {
            *bridge = HostBridge::new().expect("respawn HostBridge after bridge fault");
        }
        match heap.finish() {
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
        match run_res {
            Ok(results) => {
                let failed: Vec<String> = results
                    .iter()
                    .filter(|r| !r.passed)
                    .map(|r| format!("{} ({:?})", r.name, r.message))
                    .collect();
                if failed.is_empty() {
                    summary.passed += 1;
                } else {
                    summary.failed_outcome.push(format!(
                        "{label} ({profile}): failing tests: {}",
                        failed.join(", ")
                    ));
                }
            }
            Err(MemoryError::OutOfFuel) => {
                summary.out_of_fuel.push(format!("{label} ({profile})"));
            }
            Err(err) => {
                summary
                    .failed_outcome
                    .push(format!("{label} ({profile}): test runner error: {err}"));
            }
        }
    }
}

fn check_standard_example(
    repo: &Path,
    name: &str,
    main_module: Option<&str>,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
    matches_filter: &dyn Fn(&str) -> bool,
) {
    let dir = repo.join("examples").join(name);
    let mut roots = fixture_files(&dir.join("src"), "fl");
    if dir.join("test").is_dir() {
        roots.extend(fixture_files(&dir.join("test"), "fl"));
    }
    let modules = match compile_example_roots(&dir, roots) {
        Ok(m) => m,
        Err(err) => {
            if matches_filter(&format!("examples/{name}")) {
                summary.total_profiles += 2;
                summary.compile_error += 2;
                summary
                    .failed_outcome
                    .push(format!("examples/{name}: compile error: {err}"));
            }
            return;
        }
    };
    if let Some(mod_name) = main_module {
        let label = format!("examples/{name}/{mod_name}");
        if matches_filter(&label) {
            let expected = fs::read_to_string(dir.join("expected.stdout")).ok();
            let entry = ModelPath::new(mod_name, "main");
            run_example_main(
                &modules,
                &entry,
                &label,
                expected.as_deref(),
                fuel,
                bridge,
                summary,
            );
        }
    }
    for module in &modules {
        if !module.interface.exports_value("tests") {
            continue;
        }
        let label = format!("examples/{name}/{}", module.name);
        if !matches_filter(&label) {
            continue;
        }
        run_example_tests(&modules, &module.name, &label, fuel, bridge, summary);
    }
}

fn check_conduit_examples(
    repo: &Path,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
    matches_filter: &dyn Fn(&str) -> bool,
) {
    let conduit = repo.join("examples/conduit");
    let contract_src = fixture_files(&conduit.join("contract/src"), "fl");
    let cases: &[(&str, PathBuf, Vec<PathBuf>, &str)] = &[
        (
            "examples/conduit/contract/Conduit.InputTest",
            conduit.join("contract"),
            {
                let mut r = contract_src.clone();
                r.extend(fixture_files(&conduit.join("contract/test"), "fl"));
                r
            },
            "Conduit.InputTest",
        ),
        (
            "examples/conduit/server/Conduit.Server.CoreTest",
            conduit.join("server"),
            {
                let mut r = contract_src.clone();
                r.push(conduit.join("server/src/Conduit/Server/Slug.fl"));
                r.push(conduit.join("server/src/Conduit/Server/Validate.fl"));
                r.push(conduit.join("server/test/Conduit/Server/CoreTest.fl"));
                r
            },
            "Conduit.Server.CoreTest",
        ),
        (
            "examples/conduit/client/Conduit.Client.PagesTest",
            conduit.join("client"),
            {
                let mut r = contract_src;
                r.extend(fixture_files(&conduit.join("client/src"), "fl"));
                r.extend(fixture_files(&conduit.join("client/test"), "fl"));
                r
            },
            "Conduit.Client.PagesTest",
        ),
    ];
    for (label, dir, roots, test_mod) in cases {
        if !matches_filter(label) {
            continue;
        }
        match compile_example_roots(dir, roots.clone()) {
            Ok(modules) => run_example_tests(&modules, test_mod, label, fuel, bridge, summary),
            Err(err) => {
                summary.total_profiles += 2;
                summary.compile_error += 2;
                summary
                    .failed_outcome
                    .push(format!("{label}: compile error: {err}"));
            }
        }
    }
}

pub(super) fn check_examples_corpus(
    repo: &Path,
    fuel: u64,
    bridge: &mut HostBridge,
    summary: &mut CorpusSummary,
    matches_filter: &dyn Fn(&str) -> bool,
) {
    let standard: &[(&str, Option<&str>)] = &[
        ("first-slice", Some("App.Main")),
        ("hom-repository", Some("App.Main")),
        ("mtl", Some("App.Capabilities")),
        ("typed-interpreter", Some("App.Main")),
        ("cli-tool", None),
    ];
    for &(name, main_mod) in standard {
        check_standard_example(repo, name, main_mod, fuel, bridge, summary, matches_filter);
    }
    check_conduit_examples(repo, fuel, bridge, summary, matches_filter);
}
