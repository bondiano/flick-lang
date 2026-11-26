//! Wave 5.1 counted execution regressions:
//! - Live OS `Platform.env`/`Platform.vars` (`effect/host_services.fl`)
//! - Pure `flick:sql` query/template/raw-SQL lowering (`sql/queries.fl`)
//! - Pure multi-module `examples/` projects (`first-slice`, `hom-repository`,
//!   `mtl`, `typed-interpreter`)
//! - Generated `flick_model::generate` programs executed on the counted heap
//!   with zero leaked heap slots.

use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_core::diagnostics::Severity;
use flick_core::project::{self, Compiled};
use flick_core::source::SourceMap;
use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_model::types::Type;
use flick_test_support::files as fixture_files;
use flick_vm::tier0::{Heap, HostMode, ProgramOutcome, Value, execute_tests_program};

const FUEL: u64 = 10_000_000;

struct FsHost;

impl project::Host for FsHost {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn assert_fixture_live_os(rel: &str) {
    let path = repo().join("tests/run").join(rel);
    let expected = fs::read_to_string(path.with_extension("stdout")).expect("expected stdout");
    let modules = run::compile(&path).expect("fixture compiles");
    let main = run::main_of(&modules).expect("fixture exports main");
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = run::counted_main(&modules, &main, release)
            .unwrap_or_else(|e| panic!("{rel} ({profile}) lowering failed: {e}"));
        let mut heap = Heap::new();
        let report = counted.execute_with_host(&mut heap, FUEL, HostMode::LiveOs);
        let returned_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let outcome_dbg = format!("{:?}", report.outcome);
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("{rel} ({profile}) outcome release failed: {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{rel} ({profile}) leaked heap slots: {e}"));
        assert_eq!(stats.live_objects, 0, "{rel} ({profile}) live objects");
        assert!(
            returned_unit,
            "{rel} ({profile}) expected Returned(Unit), got {outcome_dbg}; stdout={:?}",
            report.stdout
        );
        assert_eq!(report.stdout, expected, "{rel} ({profile}) stdout mismatch");
    }
}

#[test]
fn stage5_host_services_live_os_matches_stdout_with_zero_leaks() {
    assert_fixture_live_os("effect/host_services.fl");
}

#[test]
fn stage5_sql_queries_matches_stdout_with_zero_leaks() {
    assert_fixture_live_os("sql/queries.fl");
}

#[test]
fn stage5_sql_raw_query_and_identifier_edge_cases_zero_leaks() {
    let source = r#"
module App.Main exposing (main)

import Sql

main : Unit ->{Host.Console} Unit
let main () =
  let ok_id = Sql.identifier "users_2026"
  let bad_digit = Sql.identifier "1users"
  let bad_long = Sql.identifier "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  let ok_63 = Sql.identifier "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  Platform.stdout_line (show (Option.is_some ok_id, Option.is_some bad_digit, Option.is_some bad_long, Option.is_some ok_63))
  match ok_id with
  | Some tbl ->
    let pieces = [
      Sql.unsafe_text "SELECT * FROM ",
      Sql.ident tbl,
      Sql.unsafe_text " WHERE a = ",
      Sql.param Int.json 7,
      Sql.unsafe_text " AND b = ",
      Sql.param String.json "hi",
    ]
    let q = Sql.unsafe_query pieces String.json
    Platform.stdout_line (Sql.text q)
    Platform.stdout_line (show (Sql.parameter_count q))
  | None -> ()
"#;
    let modules = run::compile_source(source).expect("sql raw compiles");
    let main = run::main_of(&modules).expect("exports main");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, FUEL, release)
            .unwrap_or_else(|e| panic!("release={release}: {e}"));
        assert_eq!(
            ran.stdout,
            "(true, false, false, true)\nSELECT * FROM \"users_2026\" WHERE a = $1 AND b = $2\n2\n"
        );
        assert_eq!(ran.memory.live_objects, 0);
    }
}

fn compile_example_project(name: &str) -> Vec<Compiled> {
    let dir = repo().join("examples").join(name);
    let mut roots = fixture_files(&dir.join("src"), "fl");
    if dir.join("test").is_dir() {
        roots.extend(fixture_files(&dir.join("test"), "fl"));
    }
    roots.sort();
    let canonical: Vec<PathBuf> = roots
        .into_iter()
        .map(|p| project::normalize(&fs::canonicalize(&p).expect("canonical path")))
        .collect();
    let dir = project::normalize(&fs::canonicalize(&dir).expect("canonical example dir"));
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
        "example {name} failed to compile:\n{}",
        errors.join("\n")
    );
    compilation.modules
}

#[test]
fn stage5_pure_examples_run_with_zero_leaks() {
    for name in ["hom-repository", "mtl", "typed-interpreter"] {
        let modules = compile_example_project(name);
        let main = run::main_of(&modules).expect("example exports main");
        let expected_stdout =
            fs::read_to_string(repo().join("examples").join(name).join("expected.stdout")).ok();
        for release in [false, true] {
            let profile = if release { "release" } else { "debug" };
            let counted = run::counted_main(&modules, &main, release)
                .unwrap_or_else(|e| panic!("example {name} ({profile}) lowering failed: {e}"));
            let mut heap = Heap::new();
            let report = counted.execute_with_host(&mut heap, FUEL, HostMode::LiveOs);
            let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
            let dbg = format!("{:?}", report.outcome);
            report
                .outcome
                .release(&mut heap)
                .unwrap_or_else(|e| panic!("example {name} ({profile}) release failed: {e}"));
            let stats = heap
                .finish()
                .unwrap_or_else(|e| panic!("example {name} ({profile}) leaked: {e}"));
            assert_eq!(stats.live_objects, 0);
            assert!(ok, "example {name} ({profile}) outcome={dbg}");
            if let Some(expected) = &expected_stdout {
                assert_eq!(
                    report.stdout, *expected,
                    "example {name} ({profile}) stdout"
                );
            }
            for module in &modules {
                if !module.interface.exports_value("tests") {
                    continue;
                }
                let entry = ModelPath::new(&module.name, "tests");
                let counted_tests =
                    run::counted_main(&modules, &entry, release).unwrap_or_else(|e| {
                        panic!("example {name} test {} ({profile}): {e}", module.name)
                    });
                let mut test_heap = Heap::new();
                let results = execute_tests_program(
                    &mut test_heap,
                    &counted_tests.program,
                    &counted_tests.entry,
                    counted_tests.root.as_ref(),
                    42,
                    FUEL,
                    &[],
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "example {name} test {} ({profile}) failed: {e}",
                        module.name
                    )
                });
                let test_stats = test_heap.finish().unwrap_or_else(|e| {
                    panic!(
                        "example {name} test {} ({profile}) leaked: {e}",
                        module.name
                    )
                });
                assert_eq!(test_stats.live_objects, 0);
                let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
                assert!(
                    failed.is_empty(),
                    "example {name} test {} ({profile}) failures: {failed:?}",
                    module.name
                );
            }
        }
    }
}

fn wrap_generated_program(p: &generate::Program) -> String {
    let base = generate::source(p);
    let call = match &p.scheme.body {
        Type::Fun(u, _, _) if **u == Type::con("Prelude", "Unit") => "program ()",
        Type::Eff(..) => "await program",
        _ => "program",
    };
    format!(
        "{base}\nmain : Unit ->{{}} Unit\nlet main () =\n  let _ = provide (Layer.succeed Log {{ log = fun _ -> () }}) {{\n    provide (Layer.succeed Counter {{ next = fun () -> 0 }}) {{\n      try\n        let _ = {call}\n        ()\n      with\n      | _ -> ()\n    }}\n  }}\n  ()\n"
    )
}

#[test]
fn stage5_counted_generated_programs_have_zero_leaks() {
    let mut executed = 0usize;
    let mut rng_state: u64 = 0x51a7_e501_cafe_babe;
    let mut next_u32 = || {
        rng_state = rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from(rng_state >> 32).expect("shifted u64 fits u32")
    };
    for _ in 0..80 {
        let len = (next_u32() as usize % 48) + 8;
        let choices: Vec<u32> = (0..len).map(|_| next_u32()).collect();
        let p = generate::program(&choices);
        if !generate::runs(&p) {
            continue;
        }
        let source = wrap_generated_program(&p);
        let Ok(modules) = run::compile_source(&source) else {
            continue;
        };
        let Ok(main) = run::main_of(&modules) else {
            continue;
        };
        for release in [false, true] {
            let Ok(counted) = run::counted_main(&modules, &main, release) else {
                continue;
            };
            let mut heap = Heap::new();
            let report = counted.execute(&mut heap, 50_000);
            report.outcome.release(&mut heap).expect("release outcome");
            let stats = heap
                .finish()
                .expect("zero leaked slots on generated program");
            assert_eq!(stats.live_objects, 0);
            executed += 1;
        }
    }
    assert!(
        executed >= 20,
        "expected at least 20 counted generated program executions, got {executed}"
    );
}
