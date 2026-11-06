//! Syntax and checking time scale with long lines, records and functions.
//! Checking budgets cover the target sizes (200 fields, 2000 lines);
//! growth ratios catch quadratic behavior without a machine-specific syntax budget.

use std::fmt::Write;
use std::time::{Duration, Instant};

use flick_core::diagnostics::Severity;
use flick_core::source::SourceMap;
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
    let mut out =
        String::from("module App.Wide exposing (wide)\n\nimport Sql exposing (Pg, plugin sql)\n\n");
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
        eprintln!("long-line {name}: 2000 = {small:?}, 8000 = {full:?}, ratio = {ratio:.1}");
    }
}
