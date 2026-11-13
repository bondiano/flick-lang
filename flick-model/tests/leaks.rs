//! The model refuses, by its own rules, what the checker refuses for the
//! hidden row of data (`docs/formal/regions.md`, "The hidden row of data is
//! not what its arguments show" and "A capture row a declaration names is
//! invariant"). flick-core lowers no module with errors, so each leak is the
//! body of a signature flick-core accepted with a body that leaks nothing,
//! written here as the model's term.

use std::rc::Rc;

use flick_model::check::Checker;
use flick_model::decls::Decls;
use flick_model::run;
use flick_model::syntax::{Con, Head, Path, Term};
use flick_model::types::Scheme;

const SOURCE: &str = r#"module App exposing (..)

import File exposing (File, Path, FileSystem, OpenError, ReadError)

type G s a = G (s ->{Async, FileSystem; ReadError} a)

run : G s a -> s ->{Async, FileSystem; ReadError} a
let run (G f) x = f x

compose : G a b -> G s a ->{Async, FileSystem; ReadError} G s b
let compose inner outer = G (fun x -> run inner (run outer x))

leaked : Path ->{Async, FileSystem; OpenError, ReadError} G Unit String
let leaked _ = G (fun _ -> "")

reads : Path ->{Async, FileSystem; OpenError, ReadError} String
let reads _ = ""

type K (c : CaptureRow) = K ((Unit ->{} Unit)^c ->{} Unit)

widen : K {} -> K c
let widen _ = K (fun _ -> ())
"#;

fn decls() -> Decls {
    let modules = run::compile_source(SOURCE).expect("flick-core accepts the signatures");
    run::decls(&modules)
}

fn scheme<'d>(d: &'d Decls, name: &str) -> &'d Scheme {
    &d.values[&Path::new("App", name)]
}

fn global(module: &str, name: &str) -> Term {
    Term::Global(Path::new(module, name))
}

fn v(x: &str) -> Term {
    Term::Var(x.into())
}

/// `G f`.
fn g(f: Term) -> Term {
    let con = Con {
        ty: Path::new("App", "G"),
        name: "G".into(),
        arity: 1,
        dicts: 0,
    };
    Term::Apply(Head::Con(Rc::new(con)), Rc::from([f]))
}

/// `fun path -> scope s { let file = File.open_scoped path; body }`.
fn in_scope(body: Term) -> Term {
    let open = Term::app(global("File", "open_scoped"), v("path"));
    let scoped = Term::Let("file".into(), None, Rc::new(open), Rc::new(body));
    Term::lam("path", Term::Scope("s".into(), Rc::new(scoped)))
}

/// `compose (G (fun f -> File.read_all f)) (G (fun _ -> file))`.
fn composition() -> Term {
    let read = g(Term::lam(
        "f",
        Term::app(global("File", "read_all"), v("f")),
    ));
    let file = g(Term::lam("u", v("file")));
    Term::apps(global("App", "compose"), [read, file])
}

/// leak6: the composition holds the file in its hidden row, which widening
/// never drops, so it does not leave the `scope` as a bare `G Unit String`
/// (`FLICK-G001`, `through_run` of `hidden_through_helper.fl`). Run inside
/// the `scope`, the same composition checks.
#[test]
fn the_model_refuses_leak6() {
    let d = decls();
    let inside = in_scope(Term::apps(
        global("App", "run"),
        [composition(), Term::unit()],
    ));
    let ran = Checker::new(&d).check_top(&inside, scheme(&d, "reads"));
    assert!(ran.is_ok(), "{ran:?}");
    let leaked = Checker::new(&d).check_top(&in_scope(composition()), scheme(&d, "leaked"));
    let error = leaked.expect_err("the model accepts leak6");
    assert_eq!(error.rule, "≤-Cap", "{error}");
}

/// A capture row a declaration names does not widen: `K {}` is no `K c`
/// (`widen` of `tests/check/fail/regions/hidden_rows.fl`).
#[test]
fn the_model_refuses_widening_a_named_row() {
    let d = decls();
    let widened = Checker::new(&d).check_top(&Term::lam("k", v("k")), scheme(&d, "widen"));
    let error = widened.expect_err("the model widens `K {}` to `K c`");
    // the row is compared by `≡`, as flick-core's `FLICK-T008`
    assert_eq!(error.rule, "≡", "{error}");
}
