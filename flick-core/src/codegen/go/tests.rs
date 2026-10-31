use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::iter;

use super::foreign::{lookup, targets};
use super::{Program, RUNTIME, go};
use crate::build::builtin_modules;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::tests::lower_text;
use crate::ir::{ItemKind, Module};
use crate::project::js_only;

/// The runtime modules (`global M` for the JS globals of module `M`) whose
/// foreigns the go target does not bind all of yet; G10e empties it.
const PENDING: &[&str] = &[
    // the rest: Record, Loop and failures of the core
    "@flick-lang/runtime",
    // Schedule: `cron` needs Cron
    "@flick-lang/runtime/effect/Schedule",
    // queues and streams
    "@flick-lang/runtime/effect/PubSub",
    "@flick-lang/runtime/effect/Channel",
    "@flick-lang/runtime/effect/ChannelSchema",
    "@flick-lang/runtime/effect/Sink",
    "@flick-lang/runtime/effect/Stream",
    "@flick-lang/runtime/effect/SubscriptionRef",
    // transactions
    "@flick-lang/runtime/effect/TxPubSub",
    "@flick-lang/runtime/effect/TxQueue",
    "@flick-lang/runtime/effect/TxSubscriptionRef",
    // time
    "@flick-lang/runtime/effect/Cron",
    "@flick-lang/runtime/effect/DateTime",
    "@flick-lang/runtime/effect/Random",
    // observability
    "@flick-lang/runtime/effect/Logger",
    "@flick-lang/runtime/effect/Metric",
    "@flick-lang/runtime/effect/References",
    "@flick-lang/runtime/effect/Tracer",
    "@flick-lang/runtime/effect/observability/PrometheusMetrics",
    // resources, caches and requests
    "@flick-lang/runtime/effect/Cache",
    "@flick-lang/runtime/effect/Pool",
    "@flick-lang/runtime/effect/RcMap",
    "@flick-lang/runtime/effect/RcRef",
    "@flick-lang/runtime/effect/Request",
    "@flick-lang/runtime/effect/RequestResolver",
    "@flick-lang/runtime/effect/Resource",
    "@flick-lang/runtime/effect/ScopedCache",
    "@flick-lang/runtime/effect/ScopedRef",
    // Effect and Cause
    "@flick-lang/runtime/effect/Cause",
    "@flick-lang/runtime/effect/Effect",
    // the rest
    "@flick-lang/runtime/effect/http/HttpClientResponse",
    "@flick-lang/runtime/effect/reactivity/AsyncResult",
    "@flick-lang/runtime/effect/server/Functions",
    "@flick-lang/runtime/effect/server/FunctionsNode",
];

/// The target diagnostics of `text` with the built-in modules.
fn compile(text: &str) -> Vec<Diagnostic> {
    let ir = lower_text("App", text).expect("lowers");
    let modules: Vec<&Module> = iter::once(&ir).chain(builtin_modules()).collect();
    let output = go(&Program {
        suites: &[],
        modules: &modules,
        entry: &ir.name,
        package: &|_| None,
        own: &|m| m == ir.name,
        location: &|_, _| None,
        foreign_file: &|_, _| None,
    });
    output.diagnostics
}

#[test]
fn what_the_target_cannot_run_yet_is_reported() {
    let errors = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (now ()))\n",
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        [codes::P012]
    );
    assert_eq!(errors[0].severity, Severity::Error);
    assert!(errors[0].span.is_some());
}

#[test]
fn unreachable_foreigns_do_not_reject_a_supported_entry() {
    let errors = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    );
    assert_eq!(errors, []);
}

#[test]
fn a_missing_local_go_foreign_reports_a_target_diagnostic() {
    let errors = compile(
        "module App exposing (main)\n\
         wait : Float ->{} Unit\nforeign async wait from \"./gone.mjs\" \"wait\"\n\
         main : Unit ->{} Unit\nlet main () = wait 1\n",
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        [codes::P012]
    );
    assert_eq!(errors[0].severity, Severity::Error);
    assert!(errors[0].span.is_some());
}

/// The `[module]` and export (binding name under `[global M]`) pairs of
/// `bindings/js_only.txt`.
fn js_only_exports() -> HashSet<(String, String)> {
    let mut module = "";
    let mut out = HashSet::new();
    for line in include_str!("bindings/js_only.txt").lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            Some(section) => module = section,
            None => {
                out.insert((module.to_owned(), line.to_owned()));
            }
        }
    }
    out
}

#[test]
fn go_bindings_cover_builtins() {
    let js_only_exports = js_only_exports();
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for module in builtin_modules().iter().filter(|m| !js_only(&m.name)) {
        for item in &module.items {
            let ItemKind::Foreign(f) = &item.kind else {
                continue;
            };
            let (from, key) = match &f.module {
                Some(from) => (from.clone(), f.export.clone()),
                None => (format!("global {}", module.name), f.name.clone()),
            };
            if PENDING.contains(&from.as_str())
                || lookup(&module.name, &f.name, f.module.as_deref(), &f.export).is_some()
                || js_only_exports.contains(&(from.clone(), key))
            {
                continue;
            }
            let entry = format!("{}.{} ({})", module.name, f.name, f.export);
            missing.entry(from).or_default().push(entry);
        }
    }
    let report = missing
        .iter()
        .fold(String::new(), |mut out, (from, names)| {
            let _ = writeln!(out, "{from} ({}): {}", names.len(), names.join(", "));
            out
        });
    assert!(
        missing.is_empty(),
        "foreigns with no go binding, not in bindings/js_only.txt nor PENDING:\n{report}"
    );
}

/// The function, variable or constant a line of Go declares at the top
/// level or in a `var` block.
fn declared(line: &str) -> Option<&str> {
    fn ident(s: &str) -> Option<&str> {
        let end = s
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(s.len());
        (end > 0).then(|| &s[..end])
    }
    if let Some(rest) = line.strip_prefix("func ") {
        return ident(rest);
    }
    if let Some(rest) = line
        .strip_prefix("var ")
        .or_else(|| line.strip_prefix("const "))
    {
        return ident(rest);
    }
    let rest = line.strip_prefix('\t').filter(|r| !r.starts_with('\t'))?;
    ident(rest).filter(|_| rest.contains(" = "))
}

#[test]
fn go_bindings_name_runtime_functions() {
    let defined: HashSet<&str> = RUNTIME
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter_map(declared)
        .collect();
    let mut unknown: Vec<&str> = targets().filter(|go| !defined.contains(go)).collect();
    unknown.sort_unstable();
    unknown.dedup();
    assert!(
        unknown.is_empty(),
        "bindings to no function of flickrt: {unknown:?}"
    );
}
