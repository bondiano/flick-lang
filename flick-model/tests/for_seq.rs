//! Regression tests for `for` loops and comprehensions over `Seq` and `List`
//! in the reference model (issue #695).

use std::path::{Path, PathBuf};

use flick_model::eval::Outcome;
use flick_model::run;

const FUEL: u64 = 50_000_000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run")
}

#[test]
fn for_loop_over_seq_and_list_repro() {
    let seq_text = r"module App.Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  for x <- Seq.range 0 2 do Platform.stdout_line (show x)
";
    let modules = run::compile_source(seq_text).expect("seq loop compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects `for` over `Seq`: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 100_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(ran.stdout, "0\n1\n");

    let list_text = r"module App.Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  for x <- [0, 1] do Platform.stdout_line (show x)
";
    let modules = run::compile_source(list_text).expect("list loop compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects `for` over `List`: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 100_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(ran.stdout, "0\n1\n");
}

#[test]
fn for_loop_over_seq_repeat_iteration_and_bounded_lazy_consumption() {
    let text = r#"module App.Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  let bounded = Seq.take 2 (Seq.from 0)
  for x <- bounded do Platform.stdout_line ("a:" ++ show x)
  for x <- bounded do Platform.stdout_line ("b:" ++ show x)
  let generated = for {
    n <- Seq.from 10
    if n % 2 == 0
  } yield n
  for x <- Seq.take 2 generated do Platform.stdout_line ("g1:" ++ show x)
  for x <- Seq.take 2 generated do Platform.stdout_line ("g2:" ++ show x)
  for n <- Seq.from 100 do {
    if n == 102 then break
    Platform.stdout_line ("brk:" ++ show n)
  }
"#;
    let modules = run::compile_source(text).expect("lazy seq loop compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects lazy/repeat `Seq` loops: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 200_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(
        ran.stdout,
        "a:0\na:1\nb:0\nb:1\ng1:10\ng1:12\ng2:10\ng2:12\nbrk:100\nbrk:101\n"
    );
}

#[test]
fn for_over_seq_supports_mutation_try_and_polymorphic_iterable() {
    let text = r#"module App.Main exposing (main)
type error Boom = Boom

emit_all : Iterable f => String -> f Int ->{Host.Console} Unit
let emit_all tag xs =
  for x <- xs do Platform.stdout_line (tag ++ ":" ++ show x)

main : Unit ->{Host.Console} Unit
let main () =
  emit_all "list" [1, 2]
  emit_all "seq" (Seq.range 3 5)
  let mut sum = 0
  try
    for x <- Seq.take 5 (Seq.from 1) do {
      if x == 2 then continue
      if x == 4 then break
      sum = sum + x
    }
  with
  | Boom -> sum = -1
  Platform.stdout_line ("sum:" ++ show sum)
"#;
    let modules = run::compile_source(text).expect("polymorphic/try seq loop compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects polymorphic/try `Seq` loop: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 200_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(ran.stdout, "list:1\nlist:2\nseq:3\nseq:4\nsum:4\n");
}

#[test]
fn for_loop_comprehension_and_ranges_paths_match_expected_values() {
    let for_comp = root().join("codegen/for_comprehension.fl");
    let modules = run::compile(&for_comp).expect("for_comprehension compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "for_comprehension rejected: {errors:?}");
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(
        ran.stdout,
        "[(3, 4, 5), (6, 8, 10), (5, 12, 13)]\n\
         [4, 16, 36]\n\
         [\"a\", \"b\"]\n\
         [6, 9]\n\
         [6, 9]\n\
         [\"a0\", \"b0\", \"b1\"]\n\
         [20, 22]\n"
    );

    let ranges = root().join("codegen/ranges.fl");
    let modules = run::compile(&ranges).expect("ranges compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "ranges rejected: {errors:?}");
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(
        ran.stdout,
        "[1, 2, 3, 4, 5]\n\
         [0, 1, 2, 3]\n\
         [10, 11, 12]\n\
         [2, 3, 4, 5, 6, 7, 8]\n\
         [(1, 1), (1, 2), (2, 2)]\n\
         []\n\
         [2, 3, 4]\n\
         [1, 2]1.5\n"
    );

    // `codegen/for_loop.fl` checks cleanly for every non-foreign definition
    // (`main` and `report`); only the JS-imported `sleep` has no model native.
    let for_loop = root().join("codegen/for_loop.fl");
    let modules = run::compile(&for_loop).expect("for_loop compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert_eq!(
        errors,
        vec!["App.Main.sleep: (Translation) no native for `sleep` (../runtime/timers.mjs sleep)"]
    );

    // Run the non-timer `for ... do` paths from `codegen/for_loop.fl` in the model.
    let loop_paths = r#"module App.Main exposing (main)

report : List (String, Float) ->{Host.Console} Unit
let report scores = for {
  (name, score) <- scores
  if score > 1
} do
  Platform.stdout_line (name ++ " " ++ show score)

main : Unit ->{Host.Console} Unit
let main () =
  for x <- [1, 2, 3] do Platform.stdout_line ("item " ++ show x)
  report [("ada", 3), ("bob", 1), ("cy", 2)]
  for {
    x <- Seq.range 0 2
    y <- ["a", "b"]
  } do {
    let label = show x ++ y
    Platform.stdout_line label
  }
  for n <- Seq.from 1 do {
    if n > 2 then break
    Platform.stdout_line ("from " ++ show n)
  }
"#;
    let modules = run::compile_source(loop_paths).expect("for_loop paths compile");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "for_loop paths rejected: {errors:?}");
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 200_000);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
    assert_eq!(
        ran.stdout,
        "item 1\nitem 2\nitem 3\nada 3\ncy 2\n0a\n0b\n1a\n1b\nfrom 1\nfrom 2\n"
    );
}
