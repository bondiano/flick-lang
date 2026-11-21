//! End-to-end reference model checks for packed modules (`Module Sig`) and
//! existential witnesses (`exists K : Sig. T`), covering both accepted programs
//! and cross-signature / mismatched-member rejections.

use std::rc::Rc;

use flick_model::run;
use flick_model::syntax::{Lit, Path as ModelPath, Term};

const PACKED_SIGNATURE_MISMATCH_SOURCE: &str = r#"module App.Main exposing (main)
module ChildA exposing (answer, Greeter, speak) =
  let answer = 42
  service Greeter =
    greet : String ->{} String
  speak : String ->{Greeter} String
  let speak name = Greeter.greet name

module ChildB exposing (Greeter, speak) =
  service Greeter =
    greet : String ->{} String
  speak : String ->{Greeter} String
  let speak name = Greeter.greet name

type module SigA =
  answer : Int

type module SigB =
  answer : Int

type module NomA =
  service Greeter = ChildA.Greeter
  speak : String ->{Greeter} String

type module NomB =
  service Greeter = ChildB.Greeter
  speak : String ->{Greeter} String

type module BoxA =
  type Key
  key : Key
  render : Key -> String

type module BoxB =
  type Key
  key : Key
  render : Key -> String

module ImplBox exposing (Key, key, render) =
  type alias Key = String
  let key = "ok"
  render : Key -> String
  let render s = s ++ "!"

packed_a : Module SigA
let packed_a = pack ChildA as SigA

packed_b : Module SigB
let packed_b = pack ChildA as SigB

nom_a : Module NomA
let nom_a = pack ChildA as NomA

nom_b : Module NomB
let nom_b = pack ChildB as NomB

box_a : Module BoxA
let box_a = pack ImplBox as BoxA

ex_a : exists K : BoxA. K.Key
let ex_a = unpack box_a as K in K.key

ex_b : exists K : BoxB. K.Key
let ex_b = unpack (pack ImplBox as BoxB) as K in K.key

main : Unit ->{Host.Console} Unit
let main () =
  let a = unpack packed_a as K in K.answer
  Platform.stdout_line (show a)
"#;

#[test]
fn packed_module_with_abstract_type_and_local_dictionary_checks_and_runs() {
    let text = r#"module App.Main exposing (main)
class Render a =
  render : a -> String

type module Policy =
  type Key
  key : Key
  renderer : Render.Dict Key
  instance Render Key from renderer

module Argument exposing (Key, key, renderer) =
  type alias Key = String
  let key = "x"
  renderer : Render.Dict Key
  let renderer = Render.dictionary { render = fun _ -> "chosen" }

packed : Module Policy
let packed = pack Argument as Policy

boxed : exists K : Policy. K.Key
let boxed = unpack packed as K in K.key

use_packed : Module Policy -> String
let use_packed m = unpack m as P in render P.key

main : Unit ->{Host.Console} Unit
let main () =
  let copy = packed
  let both = [copy, packed]
  let first = match both with
    | [m, _] -> use_packed m
    | _ -> "bad"
  let second = unpack boxed as K, item in render item
  Platform.stdout_line (first ++ ":" ++ second)
"#;
    let modules = run::compile_source(text).expect("source checks");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects abstract packed module / existential program: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 100_000);
    assert_eq!(ran.stdout, "chosen:chosen\n", "{:?}", ran.outcome);
}

#[test]
fn packed_module_used_at_another_signature_is_rejected() {
    let modules =
        run::compile_source(PACKED_SIGNATURE_MISMATCH_SOURCE).expect("baseline program compiles");
    let mut program = run::program(&modules);
    assert_eq!(run::check(&modules, &program), Vec::<String>::new());
    let nom_b_body = program.globals[&ModelPath::new("App.Main", "nom_b")].clone();

    let mut check_replaced = |name: &str, replacement: Term, expected_prefix: &str| {
        let path = ModelPath::new("App.Main", name);
        let original = program
            .globals
            .insert(path.clone(), replacement)
            .expect("existing global");
        let errors = run::check(&modules, &program);
        program.globals.insert(path, original);
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert!(
            errors[0].starts_with(expected_prefix),
            "{name}: expected prefix `{expected_prefix}`, got `{errors:?}`"
        );
    };

    // 1. Different signature name with identical fields (`Module SigA` at `Module SigB`)
    check_replaced(
        "packed_b",
        Term::Global(ModelPath::new("App.Main", "packed_a")),
        "App.Main.packed_b: (≡)",
    );

    // 2. Missing field when packing `Module SigA`
    check_replaced(
        "packed_a",
        Term::Record(Rc::from([])),
        "App.Main.packed_a: (Rec)",
    );

    // 3. Mismatched field type when packing `Module SigA`
    check_replaced(
        "packed_a",
        Term::Record(Rc::from([("answer".into(), Term::string("nope"))])),
        "App.Main.packed_a: (≡)",
    );

    // 4. Incompatible nominal service identity (`ChildB.speak` packed at `NomA`)
    check_replaced("nom_a", nom_b_body, "App.Main.nom_a: ");

    // 5. Inconsistent abstract witness (`key : Int` with `render : String -> String`)
    let render_fn = Term::Global(ModelPath::new("App.Main", "ImplBox$render"));
    check_replaced(
        "box_a",
        Term::Record(Rc::from([
            ("key".into(), Term::Lit(Lit::Int(7))),
            ("render".into(), render_fn),
        ])),
        "App.Main.box_a: (≡)",
    );

    // 6. Existential package used at another signature (`exists K : BoxA. K.Key` at `BoxB`)
    check_replaced(
        "ex_b",
        Term::Global(ModelPath::new("App.Main", "ex_a")),
        "App.Main.ex_b: (≡)",
    );
}
