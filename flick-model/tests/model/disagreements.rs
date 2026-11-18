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
