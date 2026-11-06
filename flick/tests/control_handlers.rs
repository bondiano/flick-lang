//! Handlers of `control` operations run on Go: `resume` is the rest of the
//! body, an arm that never resumes discontinues it and closes its scopes, a
//! handler hands a call of a handler around it on, and a second `resume`
//! is a defect. On JavaScript they wait for a runtime of their own (P012).
//! Ignored by default: part of `just e2e`, needs Go.

mod common;

use std::fs;

use common::{APP, flick_in, project};

const MAIN: &str = "module App.Main exposing (main)

effect Emit a =
  control emit : a ->{} Unit

effect Choose =
  control choose : Unit ->{} Bool

say : String ->{} Unit
let say line = Platform.stdout_line line

collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []

reversed : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let reversed body =
  try body () with
  | Emit.emit x ->
    let rest = resume ()
    rest ++ [x]
  | return () -> []

first : (Unit ->{Emit Int; none} Unit) ->{} Int
let first body =
  try body () with
  | Emit.emit x -> x
  | return () -> 0

three : Unit ->{Emit Int; none} Unit
let three () =
  Emit.emit 1
  Emit.emit 2
  Emit.emit 3

main : Unit ->{} Unit
let main () =
  say (show (collect three))
  say (show (reversed three))
  let x = first (fun () -> scope {
    Scope.add_finalizer (task { say \"scope closed\" })
    Emit.emit 7
    say \"not reached\"
  })
  say (show x)
  let chosen = collect (fun () ->
    try
      Emit.emit (if Choose.choose () then 1 else 2)
      Emit.emit 3
    with
    | Choose.choose () -> resume true)
  say (show chosen)
  let twice = try (if Choose.choose () then 1 else 2) with
    | Choose.choose () -> resume true + resume false
  say (show twice)
";

#[test]
#[ignore = "needs Go"]
fn control_handlers_run_on_go_and_wait_on_javascript() {
    let dir = project("control_handlers", &[("src/App/Main.fl", MAIN)]);
    fs::write(
        dir.join("flick.toml"),
        format!("{APP}\n[build]\nemit = \"go\"\n"),
    )
    .expect("write");
    let run = flick_in(&dir, &["run"]);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[1, 2, 3]\n[3, 2, 1]\nscope closed\n7\n[1, 3]\n",
        "{run:?}"
    );
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("`resume` ran twice"), "{stderr}");

    fs::write(
        dir.join("flick.toml"),
        format!("{APP}\n[build]\nemit = \"js\"\n"),
    )
    .expect("write");
    let build = flick_in(&dir, &["build"]);
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(
        !build.status.success() && stderr.contains("FLICK-P012"),
        "{stderr}"
    );
}
