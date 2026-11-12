//! A composition costs no more than the function it stands for: in release
//! it is the same JS as the code written without it.

use super::tests::release_with;
use crate::build::builtin_modules;
use crate::codegen::exported_worker;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage};
use crate::source::{FileId, Span};

const PRELUDE: &str = "module App exposing (main)\n\n\
    say : String ->{} Unit\nlet say line = Platform.stdout_line line\n\n\
    step : Int -> Int\nlet step n =\n  match n with\n  | 0 -> 1\n  | k -> k * 3\n\n\
    shrink : Int -> Int\nlet shrink n =\n  match n with\n  | 1 -> 0\n  | k -> k - 1\n\n\
    big : Int -> Bool\nlet big n = n > 2\n\n\
    seen : Int ->{} Int\nlet seen n =\n  say (show n)\n  n\n\n\
    divide : Int -> Int -> Int\nlet divide d =\n  let q = Int.unsafe_div 1 d\n  fun n -> n + q\n\n";

/// The workers of the built-in modules, as a build gives them.
fn workers(module: &str, name: &str) -> Option<usize> {
    let ir = builtin_modules().iter().find(|m| m.name == module)?;
    (ir.items.iter().filter_map(exported_worker))
        .find_map(|(n, arity)| (n == name).then_some(arity))
}

/// The release JS of the prelude, `definitions` and a `main` of `body`.
fn js(definitions: &str, body: &str) -> String {
    let text = format!("{PRELUDE}{definitions}main : Unit ->{{}} Unit\nlet main () =\n{body}\n");
    release_with(&text, &workers)
}

/// `composed` and `written` give the same module.
fn same(composed: (&str, &str), written: (&str, &str)) {
    let (composed, written) = (js(composed.0, composed.1), js(written.0, written.1));
    assert_eq!(composed, written);
}

#[test]
fn an_applied_composition_is_the_nested_call() {
    let call = "  say (show (shrink (step 4)))";
    same(("", "  say (show ((step >> shrink) 4))"), ("", call));
    same(("", "  say (show (4 |> (step >> shrink)))"), ("", call));
    same(("", "  say (show ((shrink << step) 4))"), ("", call));
    same(("", "  say ((step >> shrink >> show) 4)"), ("", call));
    same(
        ("", "  let h = step >> shrink\n  say (show (h 4))"),
        ("", call),
    );
}

#[test]
fn a_named_composition_is_inlined_as_a_written_function() {
    let uses = "  say (show (h 4))\n  say (show (4 |> h))";
    same(
        ("h : Int -> Int\nlet h = step >> shrink\n\n", uses),
        ("h : Int -> Int\nlet h = fun x -> shrink (step x)\n\n", uses),
    );
    assert!(js("h : Int -> Int\nlet h = step >> shrink\n\n", uses).contains("shrink(step(4))"));
    // a method of a known instance is read where the composition calls it,
    // as in the function written out
    let uses = "  say (back 4)\n  say (back 5)";
    let back = js("back : Int -> String\nlet back = show << shrink\n\n", uses);
    assert!(
        back.contains("export const back = ($x) => $m_Instances.$i_Show_Int.show(shrink($x));"),
        "{back}"
    );
}

#[test]
fn partial_applications_complete_where_the_composition_calls_them() {
    let call = "  say (show (List.filter big (List.map step [1, 2, 3])))";
    same(
        (
            "",
            "  say (show ((List.map step >> List.filter big) [1, 2, 3]))",
        ),
        ("", call),
    );
    let uses = "  say (show (ks [1, 2, 3]))";
    same(
        (
            "ks : List Int -> List Int\nlet ks = List.map step >> List.filter big\n\n",
            uses,
        ),
        (
            "ks : List Int -> List Int\nlet ks = fun xs -> List.filter big (List.map step xs)\n\n",
            uses,
        ),
    );
    assert!(js("", call).contains("filter$2(big, $m_List.map$2(step, [1, 2, 3]))"));
}

#[test]
fn an_effectful_composition_runs_its_calls_in_place() {
    same(
        ("", "  say (show ((seen >> step >> seen) 3))"),
        ("", "  say (show (seen (step (seen 3))))"),
    );
}

#[test]
fn an_operand_that_runs_code_is_evaluated_once_before() {
    // `divide 0` throws as soon as it has its argument: it stays first
    same(
        ("", "  say (show ((divide 0 >> shrink) 4))"),
        ("", "  let f = divide 0\n  say (show (shrink (f 4)))"),
    );
}

#[test]
fn a_pipe_into_a_call_is_the_call() {
    let piped = js("", "  say (show ([1, 2, 3] |> List.map step))");
    assert!(piped.contains("$m_List.map$2(step, [1, 2, 3])"), "{piped}");
}

/// `Within(outer, (fun x -> acquire (acquire x)) "a")`: both calls acquire
/// into `outer`. Code generation gives the owner to the first call it
/// meets, so the call keeps its shape; without an owner it is reduced.
#[test]
fn a_call_that_acquires_into_an_owner_keeps_its_shape() {
    let span = Span::new(FileId::default(), 0, 1);
    let e = |kind| Expr::new(kind, span);
    let call = |func: Expr, arg: Expr| {
        e(ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: Stage::Runtime,
            arg_first: false,
        })
    };
    let acquire = || {
        e(ExprKind::Global {
            module: "App".to_owned(),
            name: "acquire".to_owned(),
        })
    };
    let x = e(ExprKind::Local("x".to_owned()));
    let lambda = e(ExprKind::Lambda {
        params: vec![Param {
            pat: Pat::Bind("x".to_owned()),
            stage: Stage::Runtime,
            ty: None,
        }],
        body: Box::new(call(acquire(), call(acquire(), x))),
    });
    let applied = call(lambda, e(ExprKind::Lit(Lit::String("a".to_owned()))));
    let mut owned = e(ExprKind::Within(
        "outer".to_owned(),
        Box::new(applied.clone()),
    ));
    let before = owned.clone();
    super::simplify(&mut owned);
    assert_eq!(owned, before);
    let mut free = applied;
    super::simplify(&mut free);
    let nested = call(
        acquire(),
        call(acquire(), e(ExprKind::Lit(Lit::String("a".to_owned())))),
    );
    assert_eq!(free, nested);
}
