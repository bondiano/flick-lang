//! Ordered guarded `match`es of data whose shape is known while some parts
//! are not: the arms tried in source order, a guard decided only when it
//! simplifies to a literal, and an observable part evaluated once, where
//! it is written.

use super::lazy_tests::optimized;
use super::tests::release;

/// `note` writes a line when its argument is evaluated.
const NOTE: &str =
    "note : Bool ->{Host.Console} Bool\nlet note b =\n  Platform.stdout_line \"flag\"\n  b\n\n";

/// `run` applies `pick` of `Language.Patterns`, local to it, to `arg`.
fn pick(signature: &str, params: &str, arg: &str) -> String {
    format!(
        "module App exposing (run)\n\n{NOTE}run : {signature}\nlet run {params} =\n  let pick = fun pair ->\n    match pair with\n    \
         | (Some n, Ok true) when n > 10 -> \"big and true\"\n    | (Some n, Ok _) -> \"some \" ++ show n\n    \
         | (None, Err e) -> \"error \" ++ e\n    | (_, Err _) -> \"some error\"\n    | (None, Ok b) -> \"none \" ++ show b\n  pick {arg}\n"
    )
}

/// `run` taking `params`, its body a `match` of `scrutinee` with `arms`.
fn matched(signature: &str, params: &str, scrutinee: &str, arms: &[&str]) -> String {
    let arms = (arms.iter()).fold(String::new(), |out, a| out + "  | " + a + "\n");
    format!(
        "module App exposing (run)\n\n{NOTE}run : {signature}\nlet run {params} =\n  match {scrutinee} with\n{arms}"
    )
}

/// The declaration of `name` in `js`, up to the blank line after it.
fn decl<'a>(js: &'a str, name: &str) -> &'a str {
    let start = format!("export const {name} = ");
    let at = js
        .find(&start)
        .unwrap_or_else(|| panic!("no {name} in\n{js}"));
    let rest = &js[at..];
    rest.split("\n\n").next().unwrap_or(rest).trim_end()
}

/// The definition of `run` in the printed optimized IR.
fn run_ir(text: &str) -> String {
    let ir = optimized(text);
    let line = ir.lines().find(|l| l.starts_with("let run = "));
    line.unwrap_or_else(|| panic!("no run in\n{ir}")).to_owned()
}

#[test]
fn a_known_tag_beside_a_dynamic_flag_keeps_one_conditional() {
    for (n, other) in [("11", "some 11"), ("36", "some 36")] {
        let text = pick("Bool -> String", "flag", &format!("(Some {n}, Ok flag)"));
        assert_eq!(
            decl(&release(&text), "run"),
            format!(
                "export const run = (flag) => {{\n  if (flag === true) {{\n    return \"big and true\";\n  }}\n  return \"{other}\";\n}};"
            )
        );
        assert_eq!(
            run_ir(&text),
            format!(
                "let run = (fun flag -> (match flag | true -> \"big and true\" | _ -> \"{other}\"))"
            )
        );
    }
}

#[test]
fn a_guard_known_false_drops_its_arm_though_the_flag_is_unknown() {
    for (n, shown) in [("3", "some 3"), ("(-1)", "some -1")] {
        let text = pick("Bool -> String", "flag", &format!("(Some {n}, Ok flag)"));
        assert_eq!(
            decl(&release(&text), "run"),
            format!("export const run = (flag) => \"{shown}\";")
        );
    }
    let all = pick("Bool -> String", "flag", "(Some 11, Ok true)");
    assert_eq!(
        decl(&release(&all), "run"),
        "export const run = (flag) => \"big and true\";"
    );
}

#[test]
fn none_takes_the_arm_its_other_part_chooses() {
    let ok = release(&pick("Bool -> String", "flag", "(None, Ok flag)"));
    assert_eq!(
        decl(&ok, "run"),
        "export const run = (flag) => \"none \" + $m_Instances.$i_Show_Bool.show(flag);"
    );
    let err = release(&pick("String -> String", "e", "(None, Err e)"));
    assert_eq!(
        decl(&err, "run"),
        "export const run = (e) => \"error \" + e;"
    );
}

#[test]
fn a_pattern_name_shadows_the_local_it_spells() {
    let text = matched(
        "Int -> Bool -> String",
        "n flag",
        "(Some 11, Ok flag)",
        &[
            "(Some n, Ok true) when n > 10 -> \"big \" ++ show n",
            "(Some k, Ok _) -> show k ++ \" not \" ++ show n",
            "(None, _) -> \"none\"",
            "(_, Err _) -> \"error\"",
        ],
    );
    assert_eq!(
        run_ir(&text),
        "let run = (fun n flag -> (match flag | true -> \"big 11\" | _ -> (\"11\" ++ (\" not \" ++ (<Show Int>.show n)))))"
    );
}

#[test]
fn constructors_spelled_as_option_are_their_own_type() {
    let text = matched(
        "Bool -> String",
        "flag",
        "(Some 11, Ok flag)",
        &[
            "(Some n, Ok true) when n > 10 -> \"big and true\"",
            "(Some n, Ok _) -> \"some \" ++ show n",
            "(None, _) -> \"none\"",
            "(_, Err _) -> \"error\"",
        ],
    )
    .replace("run : ", "type Choice = Some Int | None\n\nrun : ");
    let js = release(&text);
    assert!(js.contains("_tag: \"Some#"), "{js}");
    assert_eq!(
        run_ir(&text),
        "let run = (fun flag -> (match flag | true -> \"big and true\" | _ -> \"some 11\"))"
    );
}

#[test]
fn an_unknown_guard_is_tested_before_the_wildcard() {
    let text = matched(
        "Int -> String",
        "limit",
        "Some 3",
        &["Some n when n > limit -> \"above\"", "_ -> \"other\""],
    );
    let ir = run_ir(&text);
    assert!(
        ir.contains("when (n > limit) -> \"above\" | _ -> \"other\""),
        "{ir}"
    );
    // an unknown guard keeps the arms after it, the impossible ones dropped
    let text = pick("Int -> Bool -> String", "k flag", "(Some k, Ok flag)");
    let ir = run_ir(&text);
    assert!(ir.contains("when (n > 10) -> \"big and true\""), "{ir}");
    assert!(ir.contains("\"some \""), "{ir}");
    assert!(!ir.contains("error"), "{ir}");
}

#[test]
fn a_guard_on_the_tail_of_a_known_list_is_decided() {
    let text = matched(
        "Int -> String",
        "x",
        "[x, 2, 3]",
        &[
            "[_, ...rest] when (match rest with | [] -> true | _ -> false) -> \"short\"",
            "[a, b] -> \"two\"",
            "[a, b, ...rest] when b > 5 -> \"big second\"",
            "[a, ...rest] -> \"head \" ++ show a",
            "[] -> \"empty\"",
        ],
    );
    assert_eq!(
        run_ir(&text),
        "let run = (fun x -> (\"head \" ++ (<Show Int>.show x)))"
    );
}

#[test]
fn an_earlier_arm_comes_first() {
    let first = matched(
        "Bool -> String",
        "_",
        "Some 5",
        &[
            "Some n when n > 1 -> \"first\"",
            "Some n when n > 0 -> \"second\"",
            "_ -> \"other\"",
        ],
    );
    assert_eq!(run_ir(&first), "let run = (fun _ -> \"first\")");
    // an Int literal, an or-pattern and a String literal decide their parts
    for (scrutinee, arms) in [
        (
            "(Some 5, flag)",
            [
                "(Some 4, _) -> \"no\"",
                "(Some 5, true) -> \"yes and true\"",
                "(Some n, _) when n > 0 -> \"yes\"",
            ],
        ),
        (
            "(Some 2, flag)",
            [
                "(Some (1 | 3), _) -> \"no\"",
                "(Some (1 | 2), true) -> \"yes and true\"",
                "(Some (1 | 2), _) -> \"yes\"",
            ],
        ),
        (
            "(\"a\", flag)",
            [
                "(\"b\", _) -> \"no\"",
                "(\"a\", true) -> \"yes and true\"",
                "(\"a\", _) -> \"yes\"",
            ],
        ),
    ] {
        let mut all = arms.to_vec();
        all.push("_ -> \"other\"");
        let text = matched("Bool -> String", "flag", scrutinee, &all);
        assert_eq!(
            run_ir(&text),
            "let run = (fun flag -> (match flag | true -> \"yes and true\" | _ -> \"yes\"))"
        );
    }
}

#[test]
fn an_undecided_alternative_keeps_the_pattern_undecided() {
    let text = matched(
        "Bool -> String",
        "flag",
        "(Some 2, flag)",
        &["(_, true) | (Some 2, _) -> \"either\"", "_ -> \"other\""],
    );
    let ir = run_ir(&text);
    assert!(
        ir.contains("\"either\"") && ir.contains("\"other\""),
        "{ir}"
    );
}

#[test]
fn an_observable_part_is_evaluated_once_where_it_is() {
    let effect = "Bool ->{Host.Console} String";
    let big = release(&pick(effect, "flag", "(Some 11, Ok (note flag))"));
    assert_eq!(
        decl(&big, "run"),
        "export const run = (flag) => Effect.gen(function* () {\n  const $p1 = yield* note(flag);\n  \
         if ($p1 === true) {\n    return \"big and true\";\n  }\n  return \"some 11\";\n});"
    );
    let small = release(&pick(effect, "flag", "(Some 3, Ok (note flag))"));
    assert_eq!(
        decl(&small, "run"),
        "export const run = (flag) => Effect.gen(function* () {\n  const $p1 = yield* note(flag);\n  return \"some 3\";\n});"
    );
    let same = release(&matched(
        effect,
        "flag",
        "(Some 11, Ok (note flag))",
        &[
            "(Some n, Ok true) when n > 10 -> \"same\"",
            "(Some n, Ok _) -> \"same\"",
            "_ -> \"other\"",
        ],
    ));
    let run = decl(&same, "run");
    assert_eq!(run.matches("note(flag)").count(), 1, "{run}");
    let first = "export const run = (flag) => Effect.gen(function* () {\n  const $p1 = yield* note(flag);\n";
    assert!(run.starts_with(first), "{run}");
    assert!(!run.contains("other"), "{run}");
}

/// The Go of `App`, whose `main` gives `run` a flag known only when it runs.
#[cfg(feature = "native-project")]
fn go_run(text: &str) -> String {
    use std::collections::BTreeMap;

    use super::tests::{builtin_arity, builtin_generic};
    use crate::build::builtin_modules;
    use crate::build::go::bindings;
    use crate::codegen::{Known, go};
    use crate::ir::tests::check_text;
    use crate::ir::{Module, lang_items};

    let text = format!(
        "{}\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line (run (note true))\n",
        text.replace("exposing (run)", "exposing (main, run)")
    );
    let (mut ir, interface) = check_text("App", &text).expect("lowers");
    super::optimize(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
    );
    let table = lang_items();
    let known = Known::of(&table);
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let out = go::go(&go::Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: bindings(),
        lang: &known,
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    (out.files.into_iter())
        .find_map(|(name, code)| name.ends_with("m_App.go").then_some(code))
        .expect("m_App.go")
}

#[cfg(feature = "native-project")]
#[test]
fn go_tests_the_flag_alone() {
    let go = go_run(&pick("Bool -> String", "flag", "(Some 11, Ok flag)"));
    let run = &go[go.find("func F_App__run(").expect("F_App__run")..];
    assert!(
        run.contains(
            "\tif (l_flag == true) {\n\t\treturn \"big and true\"\n\t}\n\treturn \"some 11\""
        ),
        "{run}"
    );
    assert_eq!(run.matches("if ").count(), 1, "{run}");
    let small = go_run(&pick("Bool -> String", "flag", "(Some 3, Ok flag)"));
    assert!(
        !small.contains("if ") && small.contains("\"some 3\""),
        "{small}"
    );
}
