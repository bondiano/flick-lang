//! Syntax, checking, lowering and both backends scale with long lines,
//! records and functions.
//! Checking budgets cover the target sizes (200 fields, 2000 lines);
//! growth ratios catch quadratic behavior without a machine-specific syntax budget.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
#[cfg(not(unix))]
use std::time::Instant as Cpu;
use std::{io, thread};

use flick_core::build::{self, Extras, Settings};
use flick_core::codegen::EffectBridge;
use flick_core::diagnostics::Severity;
use flick_core::project::config::Backend;
use flick_core::project::{self, Compilation, Host};
use flick_core::source::{FileId, SourceMap};
use flick_core::syntax::{self, lexer};
#[cfg(unix)]
use rustix::time::{ClockId, clock_gettime};

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

/// A point in the CPU time of this process: unlike the wall clock, it leaves
/// out the time the scheduler gives to other processes, which would slow a
/// long run more than a short one.
#[cfg(unix)]
struct Cpu(Duration);

#[cfg(unix)]
impl Cpu {
    fn now() -> Self {
        let now = clock_gettime(ClockId::ProcessCPUTime);
        Self(Duration::new(
            u64::try_from(now.tv_sec).expect("a time since the start"),
            u32::try_from(now.tv_nsec).expect("nanoseconds"),
        ))
    }

    fn elapsed(&self) -> Duration {
        Self::now().0.saturating_sub(self.0)
    }
}

/// Holds the other tests of this file back: the CPU time of the process
/// would count their work too.
fn alone() -> MutexGuard<'static, ()> {
    static ALONE: Mutex<()> = Mutex::new(());
    ALONE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One check of `text`.
fn time(text: &str) -> Duration {
    let mut sources = SourceMap::default();
    let file = sources.add("big.fl", text);
    let start = Cpu::now();
    let diagnostics = flick_core::check(&sources, file);
    let elapsed = start.elapsed();
    // the functions are long on purpose, so only errors count
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    elapsed
}

/// The fastest of five rounds of `runs`, one run of each per round: a load
/// that outlasts a run slows every size alike instead of only one of them.
fn fastest<const N: usize>(mut runs: [&mut dyn FnMut() -> Duration; N]) -> [Duration; N] {
    let mut best = [Duration::MAX; N];
    for _ in 0..5 {
        for (best, run) in best.iter_mut().zip(&mut runs) {
            *best = (*best).min(run());
        }
    }
    best
}

fn assert_ratio(name: &str, small: Duration, full: Duration) {
    // linear growth gives about 4; quadratic would give about 16
    let ratio = full.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 9.0,
        "{name}: 4x the size takes {ratio:.1}x the time ({small:?} -> {full:?})"
    );
}

fn assert_linear(name: &str, make: fn(usize) -> String, size: usize) {
    let _alone = alone();
    let (small, full) = (make(size / 4), make(size));
    let [small, full] = fastest([&mut || time(&small), &mut || time(&full)]);
    assert!(
        full < Duration::from_secs(2),
        "{name}: {size} takes {full:?}"
    );
    assert_ratio(name, small, full);
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
    let _alone = alone();
    let text = wide_join(11);
    let [full] = fastest([&mut || time(&text)]);
    assert!(full < Duration::from_secs(2), "10 joins take {full:?}");
}

/// `L2 a = L1 (L1 (L1 (L1 a)))` nests an alias in its own argument; `show`
/// of 256 nested lists reaches the budget of nested constraints. Each level
/// of the constraint unfolds the aliases once: one left folded is unfolded
/// again at every level below it (20 s in release).
const NESTED_ALIASES: &str = "module App.Deep exposing (shown)

type alias L1 a = List (List (List (List a)))
type alias L2 a = L1 (L1 (L1 (L1 a)))
type alias L3 a = L2 (L2 (L2 (L2 a)))
type alias L4 a = L3 (L3 (L3 (L3 a)))

shown : L4 Float -> String
let shown value = show value
";

#[test]
fn constraint_on_aliases_nested_in_their_arguments_meets_its_budget_in_time() {
    let _alone = alone();
    let [full] = fastest([&mut || {
        let mut sources = SourceMap::default();
        let file = sources.add("deep.fl", NESTED_ALIASES);
        let start = Cpu::now();
        let diagnostics = flick_core::check(&sources, file);
        let elapsed = start.elapsed();
        let codes: Vec<String> = diagnostics.iter().map(|d| d.code.to_string()).collect();
        assert_eq!(codes, ["FLICK-C009"]);
        elapsed
    }]);
    assert!(
        full < Duration::from_secs(2),
        "nested aliases take {full:?}"
    );
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
    let _alone = alone();
    for parse in [false, true] {
        let measure = |text: &str| {
            let mut sources = SourceMap::default();
            let file = sources.add("line.fl", text);
            let start = Cpu::now();
            if parse {
                let parsed = syntax::parse(text, file);
                let elapsed = start.elapsed();
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                assert_eq!(parsed.tree.text(text), text);
                elapsed
            } else {
                let (tokens, diagnostics) = lexer::lex(text, file);
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
        };
        let (small, full) = (long_line(2000), long_line(8000));
        let [small, full] = fastest([&mut || measure(&small), &mut || measure(&full)]);
        let name = if parse {
            "long-line parse (including lex)"
        } else {
            "long-line lex"
        };
        assert_ratio(name, small, full);
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

fn optimizing(compilation: &Compilation, _: &SourceMap) -> Duration {
    let modules = &compilation.modules;
    let start = Cpu::now();
    for module in modules {
        build::optimized(modules, module, false);
    }
    start.elapsed()
}

fn emitting_js(compilation: &Compilation, sources: &SourceMap) -> Duration {
    let start = Cpu::now();
    let (artifacts, diagnostics) = build::emit(compilation, sources, &settings(&[]));
    let elapsed = start.elapsed();
    assert_eq!(diagnostics, []);
    assert!(artifacts.iter().any(|a| a.path == "Main.js"));
    elapsed
}

// the Go backend is part of a build that writes its artifacts
#[cfg(feature = "native-project")]
fn emitting_go(compilation: &Compilation, sources: &SourceMap) -> Duration {
    let entries = [("main".to_owned(), "Main".to_owned())];
    let start = Cpu::now();
    let (artifacts, diagnostics) =
        build::go::emit(compilation, sources, &settings(&entries), &[], &|_| None);
    let elapsed = start.elapsed();
    assert_eq!(diagnostics, []);
    assert!(!artifacts.is_empty());
    elapsed
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

/// `stage` on a function of 2000 lines against one of 500, each less what
/// the stage takes on a program of one line: the built-in modules every
/// build emits.
fn assert_stage_linear(name: &'static str, stage: Stage) {
    let _alone = alone();
    let [base, small, full] = deep(move || {
        let [base, small, full] = [2, 500, 2000].map(|lines| compiled(&long_program(lines)));
        let run = |(sources, compilation): &(SourceMap, Compilation)| stage(compilation, sources);
        fastest([&mut || run(&base), &mut || run(&small), &mut || run(&full)])
    });
    let (small, full) = (small.saturating_sub(base), full.saturating_sub(base));
    assert!(full < Duration::from_secs(2), "{name}: takes {full:?}");
    assert_ratio(name, small, full);
}

#[test]
fn function_of_2000_lines_checks_and_lowers_in_linear_time() {
    let _alone = alone();
    let compile = |text: &str| {
        let start = Cpu::now();
        compiled(text);
        start.elapsed()
    };
    let (small, full) = (long_program(500), long_program(2000));
    let [small, full] = fastest([&mut || compile(&small), &mut || compile(&full)]);
    assert_ratio("check and lower", small, full);
}

#[test]
fn function_of_2000_lines_optimizes_in_linear_time() {
    assert_stage_linear("optimizer", optimizing);
}

#[test]
fn function_of_2000_lines_emits_javascript_in_linear_time() {
    assert_stage_linear("JavaScript", emitting_js);
}

#[cfg(feature = "native-project")]
#[test]
fn function_of_2000_lines_emits_go_in_linear_time() {
    assert_stage_linear("Go", emitting_go);
}
