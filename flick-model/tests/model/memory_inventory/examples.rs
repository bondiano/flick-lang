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
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;
use flick_vm::tier0::{
    Error as MemoryError, Heap, HostBridge, HostMode, ProgramOutcome, Value, execute_tests_program,
    execute_tests_program_with_bridge,
};
use flick_vm::tier1;

use super::helpers::FsHost;
use super::{CorpusSummary, verify_tier1_main_diff};
use crate::memory_tier1_diff::is_tier1_subset;

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
            Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {
                summary.record_stats(&stats);
            }
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
        if let Ok(closed) = run::counted_main(modules, entry, release)
            && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
        {
            verify_tier1_main_diff(&closed, label, profile, fuel, summary);
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
            Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {
                summary.record_stats(&stats);
            }
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
                if let Ok(closed) = run::counted_main(modules, &entry, release)
                    && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
                {
                    verify_tier1_tests_diff(&closed, label, profile, fuel, summary);
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
            "examples/conduit/server/Conduit.Server.CorePropertyTest",
            conduit.join("server"),
            {
                let mut r = contract_src.clone();
                r.push(conduit.join("server/src/Conduit/Server/Slug.fl"));
                r.push(conduit.join("server/test/Conduit/Server/CorePropertyTest.fl"));
                r
            },
            "Conduit.Server.CorePropertyTest",
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

pub(super) fn verify_tier1_tests_diff(
    closed: &run::CountedMain,
    label: &str,
    profile: &str,
    fuel: u64,
    summary: &mut CorpusSummary,
) {
    summary.tier1_checked += 1;
    let mut heap0 = Heap::new();
    let res0 = execute_tests_program(
        &mut heap0,
        &closed.program,
        &closed.entry,
        closed.root.as_ref(),
        42,
        fuel,
        &[],
    );
    match heap0.finish() {
        Ok(s) if s.live_objects == 0 && s.allocations == s.frees => {}
        Ok(s) => {
            summary.leaked.push(format!(
                "{label} ({profile}) [tier0-closed]: live={} alloc={} free={}",
                s.live_objects, s.allocations, s.frees
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
    let res1 = tier1::execute_tests_program_with_host(
        &mut heap1,
        &closed.program,
        &closed.entry,
        closed.root.as_ref(),
        42,
        fuel,
        &[],
        HostMode::Deterministic,
    );
    match heap1.finish() {
        Ok(s) if s.live_objects == 0 && s.allocations == s.frees => {}
        Ok(s) => {
            summary.leaked.push(format!(
                "{label} ({profile}) [tier1]: live={} alloc={} free={}",
                s.live_objects, s.allocations, s.frees
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

    match (res0, res1) {
        (Ok(r0), Ok(Ok(r1))) => {
            if r0 != r1 {
                summary.failed_outcome.push(format!(
                    "{label} ({profile}) [tier1]: test results differ from tier0"
                ));
            }
        }
        (Err(MemoryError::OutOfFuel), _) | (_, Ok(Err(MemoryError::OutOfFuel))) => {}
        (Err(e0), Ok(Err(e1))) if e0 == e1 => {}
        (r0, r1) => {
            summary.failed_outcome.push(format!(
                "{label} ({profile}) [tier1]: test runner mismatch tier0={r0:?} tier1={r1:?}"
            ));
        }
    }
}

pub(super) fn check_examples_tier1_corpus(repo: &Path, fuel: u64, summary: &mut CorpusSummary) {
    let standard: &[(&str, Option<&str>)] = &[
        ("first-slice", Some("App.Main")),
        ("hom-repository", Some("App.Main")),
        ("mtl", Some("App.Capabilities")),
        ("typed-interpreter", Some("App.Main")),
        ("cli-tool", None),
    ];
    for &(name, main_mod) in standard {
        let dir = repo.join("examples").join(name);
        let mut roots = fixture_files(&dir.join("src"), "fl");
        if dir.join("test").is_dir() {
            roots.extend(fixture_files(&dir.join("test"), "fl"));
        }
        let Ok(modules) = compile_example_roots(&dir, roots) else {
            summary.compile_error += 1;
            continue;
        };
        if let Some(mod_name) = main_mod {
            let entry = ModelPath::new(mod_name, "main");
            let label = format!("examples/{name}/{mod_name}");
            for release in [false, true] {
                let profile = if release { "release" } else { "debug" };
                if let Ok(closed) = run::counted_main(&modules, &entry, release)
                    && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
                {
                    summary.total_profiles += 1;
                    verify_tier1_main_diff(&closed, &label, profile, fuel, summary);
                }
            }
        }
        for module in &modules {
            if !module.interface.exports_value("tests") {
                continue;
            }
            let entry = ModelPath::new(&module.name, "tests");
            let label = format!("examples/{name}/{}", module.name);
            for release in [false, true] {
                let profile = if release { "release" } else { "debug" };
                if let Ok(closed) = run::counted_main(&modules, &entry, release)
                    && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
                {
                    summary.total_profiles += 1;
                    verify_tier1_tests_diff(&closed, &label, profile, fuel, summary);
                }
            }
        }
    }
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
        let Ok(modules) = compile_example_roots(dir, roots.clone()) else {
            summary.compile_error += 1;
            continue;
        };
        let entry = ModelPath::new(test_mod, "tests");
        for release in [false, true] {
            let profile = if release { "release" } else { "debug" };
            if let Ok(closed) = run::counted_main(&modules, &entry, release)
                && is_tier1_subset(&closed.program, &closed.entry, closed.root.as_ref())
            {
                summary.total_profiles += 1;
                verify_tier1_tests_diff(&closed, label, profile, fuel, summary);
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
