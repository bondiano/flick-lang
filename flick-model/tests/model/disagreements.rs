//! Programs flick-core accepts that the model rejected by a rule of its
//! own while both targets ran them: each is checked by the model and run
//! against what the program prints by the language's rules.

use flick_model::eval::Outcome;
use flick_model::run;

/// Checks `text` in the model without an error and runs its `main`.
#[track_caller]
fn prints(text: &str, expected: &str) {
    let modules = run::compile_source(text).expect("flick-core accepts it");
    let program = run::program(&modules);
    assert_eq!(run::check(&modules, &program), Vec::<String>::new());
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 1_000_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(ran.stdout, expected);
}

/// Both functions raise into one row, `{Oops}`: the `Errors` of each
/// result is `Oops`, which `describe` matches.
#[test]
fn a_shared_error_row_projects_to_its_one_error() {
    prints(
        r#"module App.Main exposing (main)
type error Oops = Oops String

attempt : (Unit ->{; e} a) ->{} Result (Errors e) a
let attempt f =
  try Ok (f ()) with
  | err -> Err err

both : (Unit ->{; e} a) -> (Unit ->{; e} a) ->{} List (Result (Errors e) a)
let both f g = [attempt f, attempt g]

one : Unit ->{} Int
let one () = 1

failing : Unit ->{; Oops} Int
let failing () = from_result (Err (Oops "two"))

describe : Result Oops Int -> String
let describe result =
  match result with
  | Ok n -> show n
  | Err (Oops text) -> "oops " ++ text

main : Unit ->{Host.Console} Unit
let main () =
  match both one failing with
  | [a, b] -> Platform.stdout_line (describe a ++ ", " ++ describe b)
  | _ -> ()
"#,
        "1, oops two\n",
    );
}

/// A tuple read from a cell is shown as a tuple, whatever its type's bounds.
#[test]
fn a_tuple_from_a_cell_is_shown() {
    prints(
        "module App.Main exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  let cell = Ref.make (100, 0)
  Platform.stdout_line (show (Ref.get cell))
",
        "(100, 0)\n",
    );
}

/// `rest` of `omit` is only known by the row it is taken from: the
/// binding keeps it unknown until then rather than generalizing it.
#[test]
fn a_row_still_constrained_is_not_generalized() {
    prints(
        "module App.Main exposing (main)
service Config =
  prefix : Unit ->{} String

type Ctx (r : ServiceRow) = Ctx Int

make : ServiceTag r -> Int -> Ctx r
let make _ n = Ctx n

omit : ServiceTag r -> Ctx (r + rest) -> Ctx rest
let omit _ c =
  match c with
  | Ctx n -> Ctx (n - 1)

size : Ctx r -> Int
let size c =
  match c with
  | Ctx n -> n

main : Unit ->{Host.Console} Unit
let main () =
  let c : Ctx {Config} = make Config 2
  let without = omit Config c
  Platform.stdout_line (show (size without))
",
        "1\n",
    );
}

/// Work kept until `s` closes may wait: `Async` is the root's, so no
/// provider has to outlive `s`.
#[test]
fn kept_work_may_wait() {
    prints(
        "module App.Main exposing (main)
nap : Unit ->{Async} Unit
let nap () = ()

keep : Fiber.Provided r s => Fiber.Owner s -> (Unit ->{r} Unit) ->{r} Unit
let keep _ f = f ()

group : Fiber.Owner s ->{Async} Unit
let group o = keep o nap

main : Unit ->{Host.Console, Async} Unit
let main () =
  scope s { group s }
  Platform.stdout_line \"kept\"
",
        "kept\n",
    );
}

#[test]
fn scope_ignores_vacuous_value_bound_from_rcmap_get() {
    let modules = run::compile_source(
        r#"module App.Main exposing (main)

acquire : String ->{Scope s} String
let acquire name = name

main : Unit ->{Async; RcMap.ExceededCapacity} Unit
let main () =
  let full = scope s {
    let map : RcMap.RcMap s String none String =
      RcMap.make { lookup = fun (key : String) -> acquire key; idleTimeToLive = None; capacity = None }
    RcMap.get "b" map
  }
  Host.raw_stdout_line ("rc map: " ++ full)
"#,
    )
    .expect("flick-core accepts repro 1");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn scope_does_not_leak_anonymous_region_through_partition_effect() {
    let modules = run::compile_source(
        r#"module App.Main exposing (main)

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  let checked = scope {
    let (passed, refused) =
      Stream.partition_effect (fun n -> if n > 2 then Ok n else Err n) (Stream.range 1 4)
    (Stream.run_collect passed, Stream.run_collect refused)
  }
  Platform.stdout_line ("partition_effect: " ++ show checked)
"#,
    )
    .expect("flick-core accepts repro 2");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert_eq!(errors, Vec::<String>::new());
}

/// A scope whose result directly returns a scoped capability (`File.open_scoped path`),
/// an opaque acquired resource (`Resource.acquire_release`), or a closure using the
/// region is still rejected by (Scope) with `FLICK-G002`.
#[test]
fn scope_still_rejects_escaping_resource_or_closure() {
    use std::rc::Rc;

    use flick_model::check::Checker;
    use flick_model::syntax::{Path, Term};

    let modules = run::compile_source(
        r#"module App.Escape exposing (..)

import File exposing (File, Path, FileSystem, OpenError, ReadError)

type Raw

make_conn : String ->{} Raw
foreign effect make_conn from "./conn.js" "make"

close_conn : forall c. Raw^c ->{} Unit
foreign effect close_conn from "./conn.js" "close"
  retention call

anchor : Path ->{Async, FileSystem; OpenError, ReadError} String
let anchor path =
  scope {
    let file = File.open_scoped path
    let _ = Resource.acquire_release (task { make_conn "db" }) close_conn
    File.read_all file
  }
"#,
    )
    .expect("signatures compile");
    let decls = run::decls(&modules);
    let global = |m: &str, n: &str| Term::Global(Path::new(m, n));
    let v = |x: &str| Term::Var(x.into());

    // `fun path -> scope s { File.open_scoped path }`
    let open_escape = Term::lam(
        "path",
        Term::Scope(
            "s".into(),
            Rc::new(Term::app(global("File", "open_scoped"), v("path"))),
        ),
    );
    let err = Checker::new(&decls)
        .infer_top(&open_escape)
        .expect_err("File.open_scoped must not escape its scope");
    assert_eq!(err.rule, "Scope", "{err}");
    assert!(err.message.contains("FLICK-G002"), "{err}");

    // `fun path -> scope s { let file = File.open_scoped path; fun () -> File.read_all file }`
    let closure_escape = Term::lam(
        "path",
        Term::Scope(
            "s".into(),
            Rc::new(Term::Let(
                "file".into(),
                None,
                Rc::new(Term::app(global("File", "open_scoped"), v("path"))),
                Rc::new(Term::lam(
                    "_",
                    Term::app(global("File", "read_all"), v("file")),
                )),
            )),
        ),
    );
    let err = Checker::new(&decls)
        .infer_top(&closure_escape)
        .expect_err("closure using scoped File must not escape its scope");
    assert_eq!(err.rule, "Scope", "{err}");
    assert!(err.message.contains("FLICK-G002"), "{err}");

    // `scope s { Resource.acquire_release (task { make_conn "db" }) close_conn }`
    let raw_escape = Term::Scope(
        "s".into(),
        Rc::new(Term::apps(
            global("Resource", "acquire_release"),
            [
                Term::Task(Rc::new(Term::app(
                    global("App.Escape", "make_conn"),
                    Term::string("db"),
                ))),
                global("App.Escape", "close_conn"),
            ],
        )),
    );
    let err = Checker::new(&decls)
        .infer_top(&raw_escape)
        .expect_err("acquired opaque Raw must not escape its scope");
    assert_eq!(err.rule, "Scope", "{err}");
    assert!(err.message.contains("FLICK-G002"), "{err}");
}
