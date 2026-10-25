//! Checking time grows linearly with the size of records and functions: a
//! record of `n` fields read field by field, and a function of `n` lines.
//! The budget for the sizes the roadmap names (200 fields, 2000 lines) is
//! generous; the ratio between sizes catches quadratic behavior.

use std::fmt::Write;
use std::time::{Duration, Instant};

use flick_core::source::SourceMap;

fn record_module(fields: usize) -> String {
    let names: Vec<String> = (0..fields).map(|i| format!("f{i}")).collect();
    let mut out = String::from("module App.Big exposing (total, make)\n\ntype alias Big = {\n");
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
