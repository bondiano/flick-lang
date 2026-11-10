//! Syntax, checking, lowering and both backends scale with long lines,
//! records and functions.
//! Checking budgets cover the target sizes (200 fields, 2000 lines);
//! growth ratios catch quadratic behavior without a machine-specific syntax budget.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{io, thread};

use flick_core::build::{self, Extras, Settings};
use flick_core::codegen::EffectBridge;
use flick_core::diagnostics::Severity;
use flick_core::project::config::Backend;
use flick_core::project::{self, Compilation, Host};
use flick_core::source::{FileId, SourceMap};
use flick_core::syntax::{self, lexer};

fn record_module(fields: usize) -> String {
    let names: Vec<String> = (0..fields).map(|i| format!("f{i}")).collect();
    let mut out = String::from("module App.Big exposing (total, make)\n\ntype alias Big = {\n");
    for name in &names {
        let _ = writeln!(out, "  {name} : Float");
    }
    out.push_str("}\n\nmake : Unit -> Big\nlet make () =\n  {\n");
    for (i, name) in names.iter().enumerate() {
        let _ = writeln!(out, "    {name} = {i}");
    }
    out.push_str("  }\n\ntotal : Big -> Float\nlet total big =\n  0");
    for name in &names {
        let _ = write!(out, " + big.{name}");
    }
    out.push('\n');
    out
}

fn long_function(lines: usize) -> String {
    let mut out =
        String::from("module App.Long exposing (run)\n\nrun : Float -> Float\nlet run x0 =\n");
    for i in 1..lines {
        let _ = writeln!(out, "  let x{i} = x{} + {i}", i - 1);
    }
    let _ = writeln!(out, "  x{}", lines - 1);
    out
}

/// The fastest of a few runs, to keep noise out of the comparison.
fn time(text: &str) -> Duration {
    (0..3)
        .map(|_| {
            let mut sources = SourceMap::default();
            let file = sources.add("big.fl", text);
            let start = Instant::now();
            let diagnostics = flick_core::check(&sources, file);
            let elapsed = start.elapsed();
            // the functions are long on purpose, so only errors count
            let errors: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{errors:?}");
            elapsed
        })
        .min()
        .expect("runs")
}

fn assert_linear(name: &str, make: fn(usize) -> String, size: usize) {
    let small = time(&make(size / 4));
    let full = time(&make(size));
    assert!(
        full < Duration::from_secs(2),
        "{name}: {size} takes {full:?}"
    );
    // linear growth gives about 4; quadratic would give about 16
    let ratio = full.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 9.0,
        "{name}: 4x the size takes {ratio:.1}x the time ({small:?} -> {full:?})"
    );
}

#[test]
fn record_of_200_fields_checks_in_linear_time() {
    assert_linear("record", record_module, 200);
}

#[test]
fn function_of_2000_lines_checks_in_linear_time() {
    assert_linear("function", long_function, 2000);
}

/// `tables` tables of 50 columns joined in one query that selects a column
/// of each.
fn wide_join(tables: usize) -> String {
    let mut out = String::from("module App.Wide exposing (wide)\n\nimport Sql exposing (Pg)\n\n");
    for t in 0..tables {
        let _ = writeln!(out, "sql table T{t} \"t{t}\" {{");
        let _ = writeln!(out, "  id : bigint primary key as BigInt");
        for c in 1..50 {
            let _ = writeln!(out, "  c{c} : text not null as String");
        }
        out.push_str("}\n\n");
    }
    out.push_str("wide : Sql.Template Pg { id : BigInt } {");
    let fields: Vec<String> = (0..tables).map(|t| format!(" r{t} : String")).collect();
    out.push_str(&fields.join(";"));
    out.push_str(" }\nlet wide =\n  sql p {\n    from a0 in T0\n");
    for t in 1..tables {
        let _ = writeln!(out, "    join a{t} in T{t} on a{t}.id == a{}.id", t - 1);
    }
    out.push_str("    where a0.id == p.id\n    select {\n");
    for t in 0..tables {
        let _ = writeln!(out, "      r{t} = a{t}.c{}", t + 1);
    }
    out.push_str("    }\n  }\n");
    out
}

#[test]
fn query_with_10_joins_of_50_columns_checks_in_budget() {
    let full = time(&wide_join(11));
    assert!(full < Duration::from_secs(2), "10 joins take {full:?}");
}

fn long_line(items: usize) -> String {
    let mut out = String::from("module App.Line exposing (xs)\nlet xs = [");
    for i in 0..items {
        if i != 0 {
            out.push_str(", ");
        }
        out.push_str("identity 1");
    }
    out.push_str("]\n");
    out
}

#[test]
fn long_line_lexing_and_parsing_scale_linearly() {
    for parse in [false, true] {
        let measure = |items| {
            let text = long_line(items);
            let mut sources = SourceMap::default();
            let file = sources.add("line.fl", &text);
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    if parse {
                        let parsed = syntax::parse(&text, file);
                        let elapsed = start.elapsed();
                        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                        assert_eq!(parsed.tree.text(&text), text);
                        elapsed
                    } else {
                        let (tokens, diagnostics) = lexer::lex(&text, file);
                        let elapsed = start.elapsed();
                        // the functions are long on purpose, so only errors count
                        let errors: Vec<_> = diagnostics
                            .iter()
                            .filter(|d| d.severity == Severity::Error)
                            .collect();
                        assert!(errors.is_empty(), "{errors:?}");
                        assert_eq!(tokens.last().expect("EOF token").end as usize, text.len());
                        elapsed
                    }
                })
                .min()
                .expect("runs")
        };
        let small = measure(2000);
        let full = measure(8000);
        let name = if parse {
            "parse (including lex)"
        } else {
            "lex"
        };
        let ratio = full.as_secs_f64() / small.as_secs_f64().max(1e-6);
        assert!(
            ratio < 9.0,
            "long-line {name}: 4x the size takes {ratio:.1}x the time ({small:?} -> {full:?})"
        );
    }
}

/// One module at `/Main.fl`, as a build reads it.
struct Module<'a>(&'a str);

impl Host for Module<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn source_path(_: FileId) -> String {
    "file:///Main.fl".to_owned()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the build settings require this shape"
)]
fn source_dir(_: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

fn settings(entries: &[(String, String)]) -> Settings<'_> {
    Settings {
        package: None,
        entries,
        typescript: false,
        release: false,
        extras: Extras::default(),
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend: Backend::Effect,
        effect_bridge: EffectBridge::Absent,
        ui: None,
        runners: &[],
        core_modules: &[],
        core_only: false,
    }
}

/// A function of `lines` lets that `main` runs, so every target reaches it.
fn long_program(lines: usize) -> String {
    let function = long_function(lines).replace("App.Long exposing (run)", "Main exposing (main)");
    format!(
        "{function}\nmain : Unit ->{{}} Unit\nlet main () = Platform.stdout_line (show (run 1))\n"
    )
}

/// A stage of a build after checking, timed on a compiled module.
type Stage = fn(&Compilation, &SourceMap) -> Duration;

fn compiled(text: &str) -> (SourceMap, Compilation) {
    let mut sources = SourceMap::default();
    let roots = [PathBuf::from("/Main.fl")];
    let (diagnostics, compilation) = project::compile(&Module(text), &mut sources, None, &roots);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    (sources, compilation)
}

fn emitting_js(compilation: &Compilation, sources: &SourceMap) -> Duration {
    let start = Instant::now();
    let (artifacts, diagnostics) = build::emit(compilation, sources, &settings(&[]));
    let elapsed = start.elapsed();
    assert_eq!(diagnostics, []);
    assert!(artifacts.iter().any(|a| a.path == "Main.js"));
    elapsed
}

fn emitting_go(compilation: &Compilation, sources: &SourceMap) -> Duration {
    let entries = [("main".to_owned(), "Main".to_owned())];
    let start = Instant::now();
    let (artifacts, diagnostics) =
        build::go::emit(compilation, sources, &settings(&entries), &[], &|_| None);
    let elapsed = start.elapsed();
    assert_eq!(diagnostics, []);
    assert!(!artifacts.is_empty());
    elapsed
}

/// The fastest of a few runs of `stage` on `text`, less what the stage
/// takes on a program of one line: the built-in modules every build emits.
fn stage_time(text: &str, stage: Stage) -> Duration {
    let fastest = |text: &str| {
        let (sources, compilation) = compiled(text);
        (0..3)
            .map(|_| stage(&compilation, &sources))
            .min()
            .expect("runs")
    };
    fastest(text).saturating_sub(fastest(&long_program(2)))
}

/// `work` on a stack that holds the recursion over a block of thousands of
/// lets: the passes after checking descend once per `let`.
fn deep<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(work)
        .expect("a thread")
        .join()
        .expect("the stage ends")
}

/// `stage` on a function of 2000 lines against one of 500.
fn assert_stage_linear(name: &'static str, stage: Stage) {
    let (small, full) = deep(move || {
        let small = stage_time(&long_program(500), stage);
        (small, stage_time(&long_program(2000), stage))
    });
    assert!(full < Duration::from_secs(2), "{name}: takes {full:?}");
    // linear growth gives about 4; quadratic would give about 16
    let ratio = full.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 9.0,
        "{name}: 4x the size takes {ratio:.1}x the time ({small:?} -> {full:?})"
    );
}

#[test]
fn function_of_2000_lines_checks_and_lowers_in_linear_time() {
    let fastest = |lines| {
        let text = long_program(lines);
        (0..3)
            .map(|_| {
                let start = Instant::now();
                compiled(&text);
                start.elapsed()
            })
            .min()
            .expect("runs")
    };
    let (small, full) = (fastest(500), fastest(2000));
    let ratio = full.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 9.0,
        "4x the size takes {ratio:.1}x the time ({small:?} -> {full:?})"
    );
}

#[test]
fn function_of_2000_lines_emits_javascript_in_linear_time() {
    assert_stage_linear("JavaScript", emitting_js);
}

#[test]
fn function_of_2000_lines_emits_go_in_linear_time() {
    assert_stage_linear("Go", emitting_go);
}
