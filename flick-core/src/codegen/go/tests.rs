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
pub(super) fn app(text: &str) -> String {
    let out = build(text);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.files
        .into_iter()
        .find_map(|(name, go)| name.ends_with("m_App.go").then_some(go))
        .expect("the file of App")
}

#[test]
fn a_dictionary_built_from_others_is_built_once() {
    let go = app("module App exposing (main)\n\n\
         below : Option Int -> Bool\nlet below o = compare o (Some 5) == LT\n\n\
         both : Option Int -> Bool\nlet both o = below o && compare o None == GT\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (both (Some 1)))\n");
    // `Ord (Option Int)`, needed in two functions, is one package variable
    // built on first use; neither function builds it
    let made = "= flickrt.Lazy(func() flickrt.Value { return I_Instances__Ord";
    assert_eq!(go.matches(made).count(), 1, "{go}");
    // set in `init`: a variable whose own value named an instance that names it is a cycle to Go
    assert!(
        go.contains("var K_App__0 func() flickrt.Value\n\nfunc init() { K_App__0 = "),
        "{go}"
    );
    assert_eq!(
        go.matches("I_Instances__Ord_x20__x28_Option").count(),
        1,
        "{go}"
    );
}

#[test]
fn a_dictionary_whose_method_is_computed_is_built_where_it_is_needed() {
    let go = app("module App exposing (main)\n\n\
         class Named a =\n  named : a -> String\n\n\
         instance Show a => Named (Option a) =\n  \
         let named =\n    let prefix = show 1\n    fun _ -> prefix\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (named (Some 2))\n");
    // `prefix` is computed when the dictionary is built, by code that may
    // read what the fiber carries: built once it would keep the first answer
    assert!(!go.contains("flickrt.Lazy("), "{go}");
    assert!(
        go.contains("I_App__Named_x20__x28_Option_x20_a_x29_(fb, I_Instances__Show_x20_Int)"),
        "{go}"
    );
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
    assert!(go.contains("var l_i int32 = l_n"), "{go}");
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
fn the_labels_of_a_record_literal_are_placed_once() {
    let go = app("module App exposing (main)\n\n\
         type alias Point = { x : Int; y : Int }\n\n\
         point : Int -> Point\nlet point n = { y = n; x = n + 1 }\n\n\
         moved : Point -> Point\nlet moved p = { y = p.x; x = p.y }\n\n\
         raised : Point -> Point\nlet raised p = { p with y = p.y + 1 }\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (raised (moved (point 1))).x)\n");
    // two literals of the same labels in the same order: one shape, no label where a record is made
    assert_eq!(
        go.matches("flickrt.ShapeOf(\"y\", \"x\")").count(),
        1,
        "{go}"
    );
    assert_eq!(go.matches("flickrt.Shaped(K_App__").count(), 2, "{go}");
    assert!(!go.contains("flickrt.Rec("), "{go}");
    // the new values of a record go into its copy: no record of them is made
    assert!(go.contains("flickrt.With(\n\t\tl_p,\n\t\tK_App__"), "{go}");
    assert!(!go.contains("flickrt.Update("), "{go}");
}

#[test]
fn a_function_of_two_arguments_is_given_both_at_once() {
    let go = app("module App exposing (main)\n\n\
         add : Int -> Int -> Int\nlet add a b = a + b\n\n\
         twice : (Int -> Int -> Int) -> Int -> Int\nlet twice f x = f x x\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (twice add 1 + twice (fun a b -> a * b) 2))\n");
    // a function of numbers given as a value is one Go function that takes
    // both values, a lambda is one too, and a call through a value gives
    // both arguments: no function is made per argument
    assert!(
        go.contains("return F_App__add(fb, flickrt.AsInt(t2), flickrt.AsInt(t3))"),
        "{go}"
    );
    assert!(
        go.contains("flickrt.Fn2(func(fb *flickrt.Fiber, l_a, l_b flickrt.Value) flickrt.Value"),
        "{go}"
    );
    assert!(go.contains("flickrt.Apply2(fb, l_f, l_x, l_x)"), "{go}");
    assert!(!go.contains("flickrt.Fn(func"), "{go}");
}

#[test]
fn an_argument_that_runs_code_is_not_given_with_the_one_before_it() {
    let go = app("module App exposing (main)\n\n\
         next : Int -> Int\nlet next n = n + 1\n\n\
         twice : (Int -> Int -> Int) -> Int -> Int\nlet twice f x = f x (next x)\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (twice (fun a b -> a * b) 2))\n");
    // `f x` may fail before `next x` runs, as it does on JavaScript
    assert!(
        go.contains("flickrt.Apply(fb, flickrt.Apply(fb, l_f, l_x), F_App__next(fb, l_x))"),
        "{go}"
    );
}

#[test]
fn a_function_of_more_stages_than_a_word_has_bits_compiles() {
    let names: Vec<String> = (0..65).map(|i| format!("a{i}")).collect();
    let out = build(&format!(
        "module App exposing (main)\n\nmain : Unit ->{{; AppError}} Unit\nlet main () =\n  \
         let wide = fun {} -> 0\n  Platform.stdout_line (show (List.length [wide]))\n",
        names.join(" ")
    ));
    assert!(!out.files.is_empty(), "{:?}", out.diagnostics);
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

#[test]
fn a_scalar_stays_in_its_go_type_through_a_choice_and_a_test() {
    let go = app("module App exposing (main)\n\n\
         spread : Int -> Int -> String -> Int\n\
         let spread a b word =\n  \
         let wide = if a > b && not (word < \"m\") then a - b else b - a\n  \
         let mark =\n    match word with\n    | \"none\" -> 0\n    | _ -> 1\n  \
         wide * 2 + mark\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (spread 5 2 \"word\"))\n");
    // the result of an `if` and of a `match` whose branches are numbers is a number
    assert!(
        go.contains("var t1 int32\n") && go.contains("var t2 int32\n"),
        "{go}"
    );
    assert!(
        go.contains("var l_wide int32 = t1") && go.contains("var l_mark int32 = t2"),
        "{go}"
    );
    // a condition Go holds as a `bool` is tested as it is, strings are
    // ordered by the runtime with no value made of them, and a literal in a
    // pattern is compared as Go compares
    assert!(
        go.contains("if ((l_a > l_b) && !(flickrt.CompareStrings(l_word, \"m\") < 0)) {"),
        "{go}"
    );
    assert!(go.contains("if (l_word == \"none\") {"), "{go}");
    for call in [
        "flickrt.Bool(",
        "flickrt.Not(",
        "flickrt.Same(",
        "flickrt.Lt(",
    ] {
        assert!(!go.contains(call), "{call} in {go}");
    }
}

const TYPED: &str = "module App exposing (main)\n\n\
fib : Int -> Int\nlet rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)\n\n\
first : a -> b -> a\nlet first x _ = x\n\n\
shout : Int -> List String -> Int\n\
let shout n words = n + List.length (List.map (fun word -> word ++ \"!\") words)\n\n\
tick : Int ->{} Int\nlet tick n =\n  Platform.stdout_line \"tick\"\n  n + 1\n\n\
main : Unit ->{; AppError} Unit\n\
let main () =\n  \
let folded = List.fold_left first 0 [fib 5, shout 1 [\"a\"]]\n  \
Platform.stdout_line (show (tick folded))\n";

#[test]
fn a_function_takes_and_returns_numbers_in_their_go_type() {
    let go = app(TYPED);
    // a number is passed and returned as it is: no value is made for a call
    assert!(
        go.contains("func F_App__fib(fb *flickrt.Fiber, l_n int32) int32 {"),
        "{go}"
    );
    assert!(
        go.contains("F_App__fib(fb, flickrt.SubInt(l_n, int32(1))),"),
        "{go}"
    );
    assert!(!go.contains("flickrt.AsInt(F_App__fib("), "{go}");
    // so is one of a function that runs effects
    assert!(
        go.contains("func F_App__tick(fb *flickrt.Fiber, l_n int32) int32 {"),
        "{go}"
    );
    // a function of values is still a value as it is
    assert!(go.contains("flickrt.Fn2(F_App__first)"), "{go}");
}

#[test]
fn a_function_inside_a_function_of_a_go_type_returns_its_own_values() {
    let go = app(TYPED);
    // `shout` returns an `int32`; the function it passes on returns a text,
    // as a value, and is not made to return a number
    assert!(
        go.contains(
            "func F_App__shout(fb *flickrt.Fiber, l_n int32, l_words flickrt.Value) int32 {"
        ),
        "{go}"
    );
    assert!(go.contains("return flickrt.Concat(l_word, \"!\")"), "{go}");
}
