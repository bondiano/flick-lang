use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::iter;

use super::foreign::{lookup, targets};
use super::{Output, Program, RUNTIME, go};
use crate::build::builtin_modules;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::tests::lower_text;
use crate::ir::{ItemKind, Module};
use crate::project::js_only;

/// The Go program of `text` with the built-in modules.
fn build(text: &str) -> Output {
    let ir = lower_text("App", text).expect("lowers");
    let modules: Vec<&Module> = iter::once(&ir).chain(builtin_modules()).collect();
    go(&Program {
        suites: &[],
        modules: &modules,
        entry: &ir.name,
        package: &|_| None,
        own: &|m| m == ir.name,
        location: &|_, _| None,
        foreign_file: &|_, _| None,
    })
}

/// The target diagnostics of `text` with the built-in modules.
fn compile(text: &str) -> Vec<Diagnostic> {
    build(text).diagnostics
}

/// The Go file of the module `App` of `text`.
fn app(text: &str) -> String {
    let out = build(text);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.files
        .into_iter()
        .find_map(|(name, go)| name.ends_with("m_App.go").then_some(go))
        .expect("the file of App")
}

const SUM: &str = "module App exposing (main, sum)\n\n\
sum : Int -> Int\nlet sum n =\n  let mut acc = 0\n  let mut i = n\n  \
while i > 0 do\n    acc = acc + i\n    i = i - 1\n  acc\n\n\
main : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show (sum 3))\n";

#[test]
fn a_while_nothing_leaves_is_a_plain_loop_over_variables_of_their_go_type() {
    let go = app(SUM);
    // boxed variables allocate on every assignment, and flags are tested on every turn
    assert!(go.contains("var l_acc int32 = int32(0)"), "{go}");
    assert!(go.contains("var l_i int32 = flickrt.AsInt(l_n)"), "{go}");
    assert!(
        !go.contains("_x24_loop") && !go.contains("flickrt.Same"),
        "{go}"
    );
}

const ONCE: &str = "module App exposing (main)\n\n\
type Mark = Low | High Int\n\n\
scale : BigInt -> BigInt\nlet scale n =\n  match n with\n  | 0n -> 10n\n  | _ -> n * 10n\n\n\
pick : Bool -> Mark\nlet pick high = if high then High 1 else Low\n\n\
label : Mark -> String\nlet label mark =\n  match mark with\n  | Low -> \"low\"\n  | High _ -> \"high\"\n\n\
main : Unit ->{; AppError} Unit\n\
let main () = Platform.stdout_line (label (pick false) ++ BigInt.to_string (scale 2n))\n";

#[test]
fn a_bigint_literal_and_a_constructor_without_fields_are_made_once() {
    let go = app(ONCE);
    // `10n` twice is one variable, and with `0n` in the pattern and `2n` three
    assert_eq!(go.matches("flickrt.BigLit(\"10\")").count(), 1, "{go}");
    assert_eq!(go.matches("flickrt.BigLit(").count(), 3, "{go}");
    assert_eq!(go.matches("flickrt.Nullary(").count(), 1, "{go}");
    // each is made by a package variable, not where the code runs
    let made = |line: &&str| line.contains("flickrt.BigLit(") || line.contains("flickrt.Nullary(");
    for line in go.lines().filter(made) {
        assert!(line.starts_with("var K_App__"), "{line}");
    }
}

#[test]
fn a_loop_with_an_exit_keeps_the_flags_of_its_exits() {
    let go = app(&SUM.replace(
        "    i = i - 1\n",
        "    i = i - 1\n    if acc > 100 then break\n",
    ));
    assert!(go.contains("_x24_loop"), "{go}");
    assert!(go.contains("var l_acc int32 = int32(0)"), "{go}");
}

#[test]
fn only_a_program_that_names_zones_embeds_the_zone_database() {
    let main_go = |body: &str| {
        let text =
            format!("module App exposing (main)\nmain : Unit ->{{}} Unit\nlet main () = {body}\n");
        let output = build(&text);
        assert_eq!(output.diagnostics, []);
        let (_, main) = output
            .files
            .into_iter()
            .find(|(path, _)| path == "main.go")
            .expect("main.go");
        main.contains("import _ \"time/tzdata\"") && main.contains("flickrt.UseZones()")
    };
    assert!(main_go(
        "Platform.stdout_line (show (Result.is_ok (DateTime.zone_make_named \"Europe/Paris\")))"
    ));
    assert!(!main_go(
        "Platform.stdout_line (DateTime.format_iso (DateTime.add DateTime.Day 1 (DateTime.now ())))"
    ));
}

#[test]
fn only_a_program_that_reaches_an_optional_package_imports_it() {
    let packages = |body: &str| {
        let text =
            format!("module App exposing (main)\nmain : Unit ->{{}} Unit\nlet main () = {body}\n");
        let output = build(&text);
        assert_eq!(output.diagnostics, []);
        let main = &output
            .files
            .iter()
            .find(|(path, _)| path == "main.go")
            .expect("main.go")
            .1;
        for package in &output.packages {
            assert!(
                main.contains(&format!("import _ \"flick.app/{package}\"")),
                "{main}"
            );
        }
        output.packages
    };
    assert_eq!(
        packages(
            "provide Database.pg (Database.config \"postgres://x\") { Platform.stdout_line \"pg\" }"
        ),
        ["flickpg"]
    );
    let compressed = |middleware: &str| {
        format!(
            "Platform.stdout_line (show (Http.ServerResponse.status (Http.Middleware.{middleware} \
             {{ algorithms = []; minSize = None; levels = [] }} \
             (fun _ -> Http.ServerResponse.text \"x\" Http.ServerResponse.defaults) \
             (Http.ServerRequest.from_client_request (Http.ClientRequest.get \"http://x/\")))))"
        )
    };
    assert_eq!(packages(&compressed("compression")), ["flickcompress"]);
    assert!(packages(&compressed("compression_web")).is_empty());
    assert!(packages("Platform.stdout_line \"plain\"").is_empty());
}

#[test]
fn optional_packages_are_the_runtime_s() {
    let mut named: Vec<&str> = super::main_file::OPTIONAL
        .iter()
        .map(|&(package, _, _)| package)
        .collect();
    let mut embedded: Vec<&str> = super::PACKAGES.iter().map(|p| p.name).collect();
    named.sort_unstable();
    embedded.sort_unstable();
    assert_eq!(named, embedded);
}

#[test]
fn a_program_s_go_mod_requires_the_modules_of_its_packages() {
    use crate::build::go::go_mod;
    assert_eq!(go_mod(&[]), "module flick.app\n\ngo 1.23\n");
    let each = super::PACKAGES.iter().map(|package| vec![package]);
    for packages in each.chain([super::PACKAGES.iter().collect()]) {
        let text = go_mod(&packages);
        let mut required: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with('\t'))
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        let mut modules: Vec<&str> = packages.iter().flat_map(|p| p.modules.lines()).collect();
        required.sort_unstable();
        modules.sort_unstable();
        assert_eq!(required, modules, "{text}");
        assert!(text.starts_with("module flick.app\n\ngo "), "{text}");
    }
}

#[test]
fn what_the_target_cannot_run_yet_is_reported() {
    let errors = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign now from global \"Date.now\"\n\
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
         now : Unit ->{} Float\nforeign now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    );
    assert_eq!(errors, []);
}

#[test]
fn a_missing_local_go_foreign_reports_a_target_diagnostic() {
    let errors = compile(
        "module App exposing (main)\n\
         wait : Float ->{Async;} Unit\nforeign wait from \"./gone.mjs\" \"wait\"\n\
         main : Unit ->{Async;} Unit\nlet main () = wait 1\n",
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        [codes::P012]
    );
    assert_eq!(errors[0].severity, Severity::Error);
    assert!(errors[0].span.is_some());
}

/// The `[module]` and export (binding name under `[global M]`) pairs of
/// `js_only.txt`.
fn js_only_exports() -> HashSet<(String, String)> {
    let mut module = "";
    let mut out = HashSet::new();
    for line in include_str!("js_only.txt").lines().map(str::trim) {
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
            let (from, key) = match &f.binding.from {
                Some(from) => (from.clone(), f.binding.name.clone()),
                None => (format!("global {}", module.name), f.name.clone()),
            };
            if lookup(
                &module.name,
                &f.name,
                f.binding.from.as_deref(),
                &f.binding.name,
            )
            .is_some()
                || js_only_exports.contains(&(from.clone(), key))
            {
                continue;
            }
            let entry = format!("{}.{} ({})", module.name, f.name, f.binding.name);
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
        "foreigns with no go binding, not in js_only.txt:\n{report}"
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
