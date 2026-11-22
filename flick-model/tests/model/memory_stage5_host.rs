//! Wave 5.3/5.4 permanent regression suite for the explicit opt-in counted Host/FFI bridge:
//! - Strict closed-world preservation (`counted_main` rejects foreign/host bindings)
//! - HTTP (`server`, `client`, `api_bodies`, `streams`, `redaction_context`, `error_cases_opaque`)
//! - RPC (`local`), CLI (`prompt`), `FileSystem` / `Stream.watch` (`stream_platform`), `ChildProcess`
//! - UI / Reactivity (`element`, `family`, `form`, `query`, `server`)
//! - Multi-module & bridged `examples/` (`cli-tool`, `first-slice` tests, `conduit/{contract,server,client}`)
//! - Fuel-prefix zero-leak sweeps on bridged HTTP and UI fixtures (zero Rust & zero host leaks).

use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_core::diagnostics::Severity;
use flick_core::project::{self, Compiled};
use flick_core::source::SourceMap;
use flick_model::memory::{
    Error as MemoryError, Heap, HostBridge, HostMode, ProgramOutcome, Value,
    execute_tests_program_with_bridge,
};
use flick_model::run::{compile, counted_main, counted_main_with_bridge, main_of};
use flick_model::syntax::Path as ModelPath;
use flick_test_support::files as fixture_files;

const FUEL: u64 = 20_000_000;

struct FsHost;

impl project::Host for FsHost {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join(rel)
}

fn run_bridged_fixture(bridge: &mut HostBridge, rel: &str) {
    let path = repo_path(rel);
    let expected_stdout = fs::read_to_string(path.with_extension("stdout"))
        .unwrap_or_else(|e| panic!("read stdout for {rel}: {e}"));
    let expect_fail = fs::read_to_string(path.with_extension("exit"))
        .ok()
        .is_some_and(|code| code.trim() != "0");

    let modules = compile(&path).unwrap_or_else(|e| panic!("compile {rel}: {e}"));
    let main = main_of(&modules).unwrap_or_else(|e| panic!("main of {rel}: {e}"));

    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main_with_bridge(&modules, &main, release)
            .unwrap_or_else(|e| panic!("lower {rel} ({profile}): {e}"));
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, FUEL, HostMode::LiveOs, bridge);
        let outcome_tag = match &report.outcome {
            ProgramOutcome::Returned(Value::Unit) => "returned_unit",
            ProgramOutcome::Returned(_) => "returned_non_unit",
            ProgramOutcome::Raised(_) => "raised",
            ProgramOutcome::Failed(_) => "failed",
            ProgramOutcome::Fault(_) => "fault",
        };
        let outcome_dbg = format!("{:?}", report.outcome);
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("release outcome for {rel} ({profile}): {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("Rust heap leak for {rel} ({profile}): {e}"));
        assert_eq!(stats.live_objects, 0, "{rel} ({profile}) live objects");
        assert_eq!(
            report.stdout, expected_stdout,
            "stdout mismatch for {rel} ({profile}); outcome={outcome_dbg}"
        );
        if expect_fail {
            assert!(
                matches!(outcome_tag, "raised" | "failed"),
                "expected Raised/Failed for {rel} ({profile}), got {outcome_dbg}"
            );
        } else {
            assert_eq!(
                outcome_tag, "returned_unit",
                "expected Returned(()) for {rel} ({profile}), got {outcome_dbg}"
            );
        }
    }
}

fn compile_project_roots(project_dir: &Path, roots: Vec<PathBuf>) -> Vec<Compiled> {
    let mut canonical: Vec<PathBuf> = roots
        .into_iter()
        .map(|p| project::normalize(&fs::canonicalize(&p).expect("canonical path")))
        .collect();
    canonical.sort();
    canonical.dedup();
    let dir = project::normalize(&fs::canonicalize(project_dir).expect("canonical dir"));
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
    assert!(
        errors.is_empty(),
        "project {} compile errors:\n{}",
        project_dir.display(),
        errors.join("\n")
    );
    compilation.modules
}

fn assert_bridged_suite(bridge: &mut HostBridge, modules: &[Compiled], module_name: &str) {
    let entry = ModelPath::new(module_name, "tests");
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main_with_bridge(modules, &entry, release)
            .unwrap_or_else(|e| panic!("lower {module_name} ({profile}): {e}"));
        let mut heap = Heap::new();
        let results = execute_tests_program_with_bridge(
            &mut heap,
            &counted.program,
            &counted.entry,
            counted.root.as_ref(),
            42,
            FUEL,
            &[],
            HostMode::LiveOs,
            bridge,
        )
        .unwrap_or_else(|e| panic!("run {module_name} ({profile}): {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("Rust heap leak in {module_name} ({profile}): {e}"));
        assert_eq!(stats.live_objects, 0);
        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert!(
            failed.is_empty(),
            "failing cases in {module_name} ({profile}): {failed:?}"
        );
        assert!(
            !results.is_empty(),
            "expected at least one test case in {module_name} ({profile})"
        );
    }
}

#[test]
fn stage5_closed_world_lowering_rejects_foreign_and_host_bindings() {
    for rel in [
        "tests/run/ffi/foreign.fl",
        "tests/run/stdlib/http/server.fl",
    ] {
        let path = repo_path(rel);
        let modules = compile(&path).expect("compile");
        let main = main_of(&modules).expect("main");
        for release in [false, true] {
            assert!(
                counted_main(&modules, &main, release).is_err(),
                "closed-world lowering must reject bridged fixture {rel} (release={release})"
            );
            assert!(
                counted_main_with_bridge(&modules, &main, release).is_ok(),
                "opt-in bridge lowering must accept {rel} (release={release})"
            );
        }
    }
}

#[test]
fn stage5_http_and_rpc_fixtures_zero_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in [
        "tests/run/stdlib/http/server.fl",
        "tests/run/stdlib/http/client.fl",
        "tests/run/stdlib/http/api_bodies.fl",
        "tests/run/stdlib/http/streams.fl",
        "tests/run/stdlib/http/redaction_context.fl",
        "tests/run/stdlib/http/error_cases_opaque.fl",
        "tests/run/stdlib/rpc/local.fl",
    ] {
        run_bridged_fixture(&mut bridge, rel);
    }
}

#[test]
fn stage5_cli_fs_process_fixtures_zero_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in [
        "tests/run/stdlib/cli/prompt.fl",
        "tests/run/stdlib/effect/logger.fl",
        "tests/run/stdlib/effect/stream_platform.fl",
        "tests/run/stdlib/process/child_process.fl",
        "tests/run/stdlib/reactivity/atom.fl",
        "tests/run/stdlib/server/streamed.fl",
    ] {
        run_bridged_fixture(&mut bridge, rel);
    }
}

#[test]
fn stage5_ui_fixtures_zero_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in [
        "tests/run/ui/element.fl",
        "tests/run/ui/family.fl",
        "tests/run/ui/form.fl",
        "tests/run/ui/hydrate.fl",
        "tests/run/ui/parity.fl",
        "tests/run/ui/query.fl",
        "tests/run/ui/server.fl",
    ] {
        run_bridged_fixture(&mut bridge, rel);
    }
}

#[test]
fn stage5_examples_bridged_tests_and_conduit_zero_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");

    let cli_dir = repo_path("examples/cli-tool");
    let mut cli_roots = fixture_files(&cli_dir.join("src"), "fl");
    cli_roots.extend(fixture_files(&cli_dir.join("test"), "fl"));
    let cli_mods = compile_project_roots(&cli_dir, cli_roots);
    assert_bridged_suite(&mut bridge, &cli_mods, "App.NotesTest");
    assert_bridged_suite(&mut bridge, &cli_mods, "App.WizardTest");

    let slice_dir = repo_path("examples/first-slice");
    let mut slice_roots = fixture_files(&slice_dir.join("src"), "fl");
    slice_roots.extend(fixture_files(&slice_dir.join("test"), "fl"));
    let slice_mods = compile_project_roots(&slice_dir, slice_roots);
    assert_bridged_suite(&mut bridge, &slice_mods, "App.UsersTest");

    let conduit = repo_path("examples/conduit");
    let contract_src = fixture_files(&conduit.join("contract/src"), "fl");

    let mut contract_roots = contract_src.clone();
    contract_roots.extend(fixture_files(&conduit.join("contract/test"), "fl"));
    let contract_mods = compile_project_roots(&conduit.join("contract"), contract_roots);
    assert_bridged_suite(&mut bridge, &contract_mods, "Conduit.InputTest");

    let mut server_roots = contract_src.clone();
    server_roots.push(conduit.join("server/src/Conduit/Server/Slug.fl"));
    server_roots.push(conduit.join("server/src/Conduit/Server/Validate.fl"));
    server_roots.push(conduit.join("server/test/Conduit/Server/CoreTest.fl"));
    let server_mods = compile_project_roots(&conduit.join("server"), server_roots);
    assert_bridged_suite(&mut bridge, &server_mods, "Conduit.Server.CoreTest");

    let mut client_roots = contract_src;
    client_roots.extend(fixture_files(&conduit.join("client/src"), "fl"));
    client_roots.extend(fixture_files(&conduit.join("client/test"), "fl"));
    let client_mods = compile_project_roots(&conduit.join("client"), client_roots);
    assert_bridged_suite(&mut bridge, &client_mods, "Conduit.Client.PagesTest");
}

#[test]
fn stage5_host_fuel_prefix_sweeps_have_zero_leaks_on_both_heaps() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in ["tests/run/stdlib/http/server.fl", "tests/run/ui/element.fl"] {
        let path = repo_path(rel);
        let modules = compile(&path).expect("compile");
        let main = main_of(&modules).expect("main");
        let counted = counted_main_with_bridge(&modules, &main, false).expect("lower");
        for fuel in [0, 1, 5, 25, 100, 500, 2000] {
            let mut heap = Heap::new();
            let report =
                counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, &mut bridge);
            match report.outcome {
                ProgramOutcome::Fault(MemoryError::LeakedForeignHandles(n)) => {
                    panic!("leaked {n} host handles at fuel={fuel} for {rel}");
                }
                ProgramOutcome::Fault(MemoryError::HostBridge(msg)) => {
                    panic!("unexpected host bridge fault at fuel={fuel} for {rel}: {msg}");
                }
                other => {
                    other.release(&mut heap).expect("release outcome");
                }
            }
            heap.finish()
                .unwrap_or_else(|e| panic!("Rust heap leak at fuel={fuel} for {rel}: {e}"));
        }
    }
}
