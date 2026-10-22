//! Checking time grows linearly with the size of records and functions: a
//! record of `n` fields read field by field, and a function of `n` lines.
//! The budget for the sizes the roadmap names (200 fields, 2000 lines) is
//! generous; the ratio between sizes catches quadratic behavior.

use std::fmt::Write;
use std::time::{Duration, Instant};

use flick_core::source::SourceMap;

fn record_module(fields: usize) -> String {
    let names: Vec<String> = (0..fields).map(|i| format!("f{i}")).collect();
    let mut out = String::from("module App.Big exposing (total)\n\ntype alias Big = {\n");
    for name in &names {
        let _ = writeln!(out, "  {name} : Number");
    }
    out.push_str("}\n\nmake : Unit -> Big\nlet make () =\n  {\n");
    for (i, name) in names.iter().enumerate() {
        let _ = writeln!(out, "    {name} = {i}");
    }
    out.push_str("  }\n\ntotal : Big -> Number\nlet total big =\n  0");
    for name in &names {
        let _ = write!(out, " + big.{name}");
    }
    out.push('\n');
    out
}

fn long_function(lines: usize) -> String {
    let mut out =
        String::from("module App.Long exposing (run)\n\nrun : Number -> Number\nlet run x0 =\n");
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
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
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
