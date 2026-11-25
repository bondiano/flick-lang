//! Issue #740: reference substitution model, certificate emitter, and counted
//! memory model verify that structural records have no observable storage
//! identity or written field order, while field expressions evaluate in source
//! order.

use std::fs;
use std::path::Path;
use std::rc::Rc;

use flick_core::build;
use flick_core::ir::memory::{Instruction, Name as MemName, Operation};
use flick_core::opt::perceus;
use flick_model::cert::{certificate, sig::Sig};
use flick_model::check::Checker;
use flick_model::eval::{self, Machine, Outcome};
use flick_model::generate::Program;
use flick_model::run;
use flick_model::syntax::{Const, Lit, Name, Term};
use flick_model::types::{Fields, Scheme, Type};

fn eval_term(term: Term) -> Term {
    let program = eval::Program::default();
    let mut machine = Machine::new(&program, 10_000);
    match machine.run(term) {
        Outcome::Value(v) => v,
        other => panic!("expected value, got {other:?}"),
    }
}

fn int_record_pairs(term: &Term) -> Vec<(String, i32)> {
    let Term::Record(fields) = term else {
        panic!("expected canonical record, got {term:?}");
    };
    fields
        .iter()
        .map(|(l, v)| match v {
            Term::Lit(Lit::Int(n)) => (l.to_string(), *n),
            other => panic!("expected Int field, got {other:?}"),
        })
        .collect()
}

#[test]
fn substitution_model_canonicalizes_permuted_records_and_preserves_source_order() {
    // Two literals with the same fields in opposite written orders evaluate
    // to the exact same canonical Term::Record value.
    let za = Term::Record(Rc::from([
        (Name::from("z"), Term::Lit(Lit::Int(1))),
        (Name::from("a"), Term::Lit(Lit::Int(2))),
    ]));
    let az = Term::Record(Rc::from([
        (Name::from("a"), Term::Lit(Lit::Int(2))),
        (Name::from("z"), Term::Lit(Lit::Int(1))),
    ]));
    let v_za = eval_term(za);
    let v_az = eval_term(az);
    assert_eq!(
        int_record_pairs(&v_za),
        vec![("a".to_owned(), 2), ("z".to_owned(), 1)]
    );
    assert_eq!(int_record_pairs(&v_za), int_record_pairs(&v_az));

    // Effectful field expressions run strictly in source order: `z` sets `c`
    // to 10 before `a` reads `c`, yet the resulting record is canonical `(a, z)`.
    let make_ref = Term::app(Term::konst(Const::RefMake), Term::Lit(Lit::Int(1)));
    let set_c = Term::Let(
        Name::from("_"),
        None,
        Rc::new(Term::apps(
            Term::konst(Const::RefSet),
            [Term::Var("c".into()), Term::Lit(Lit::Int(10))],
        )),
        Rc::new(Term::Lit(Lit::Int(99))),
    );
    let get_c = Term::app(Term::konst(Const::RefGet), Term::Var("c".into()));
    let body = Term::Record(Rc::from([
        (Name::from("z"), set_c),
        (Name::from("a"), get_c),
    ]));
    let expr = Term::Let(Name::from("c"), None, Rc::new(make_ref), Rc::new(body));
    let result = eval_term(expr);
    assert_eq!(
        int_record_pairs(&result),
        vec![("a".to_owned(), 10), ("z".to_owned(), 99)]
    );
}

#[test]
fn certificate_elaboration_preserves_source_order_for_out_of_order_records() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generate/Gen.fl");
    let modules = run::compile(&file).expect("Gen.fl compiles");
    let decls = run::decls(&modules);
    let sig = Sig::of(&decls);

    // `let f = fun x -> x in { z = f 1; a = f 2 }` has non-value fields out of
    // canonical order referencing an outer binding `f`; elaboration must emit
    // source-order `.let_` bindings (shifting `f` across the first binding)
    // before constructing the canonical `.rcons "a" ... (.rcons "z" ... .rnil)`.
    let call_one = Term::app(Term::Var("f".into()), Term::Lit(Lit::Int(1)));
    let call_two = Term::app(Term::Var("f".into()), Term::Lit(Lit::Int(2)));
    let int = Type::con("Prelude", "Int");
    let rec_ty = Type::Record(Fields {
        fields: [("a".into(), int.clone()), ("z".into(), int)].into(),
        tail: None,
    });
    let prog = Program {
        scheme: Scheme::new(Vec::new(), rec_ty),
        term: Term::Let(
            Name::from("f"),
            None,
            Rc::new(Term::lam("x", Term::Var("x".into()))),
            Rc::new(Term::Record(Rc::from([
                (Name::from("z"), call_one),
                (Name::from("a"), call_two),
            ]))),
        ),
    };
    let trace = Checker::new(&decls)
        .check_top_traced(&prog.term, &prog.scheme)
        .expect("model checks out-of-order record");
    let lean = certificate("out_of_order_rec", &prog, &trace, &sig)
        .expect("certificate emits for out-of-order record");
    let first_let = lean.find("(.let_ (.app (.var 0) (.lit (.int 1)))");
    assert!(
        first_let.is_some(),
        "expected source-order let for z referencing f at var 0: {lean}"
    );
    let second_let = lean.find("(.let_ (.app (.var 1) (.lit (.int 2)))");
    assert!(
        second_let.is_some(),
        "expected source-order let for a referencing shifted f at var 1: {lean}"
    );
    assert!(first_let < second_let, "z must bind before a: {lean}");
    assert!(
        lean.contains("(.rcons \"a\" (.var 0) (.rcons \"z\" (.var 1) .rnil))"),
        "expected canonical rcons spine referencing source-order bindings: {lean}"
    );
}

#[test]
fn counted_lowering_and_execution_canonicalize_records_with_zero_leaks() {
    let source = "
module App.Main exposing (main)

main : Unit -> { a : Int; z : Int }
let main () = { z = 1; a = 2 }
";
    let modules = run::compile_source(source).expect("source compiles");
    let ir: Vec<_> = modules.iter().map(|m| (*m.ir).clone()).collect();
    let entry = MemName::new("App.Main", "main");
    let program = perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        &entry,
    )
    .expect("counted lowering");
    let record_labels: Vec<Vec<String>> = program
        .functions
        .values()
        .flat_map(|f| f.body.instructions.iter())
        .filter_map(|instr| match instr {
            Instruction::Let {
                value: Operation::Record { fields },
                ..
            } => Some(fields.iter().map(|(l, _)| l.clone()).collect()),
            _ => None,
        })
        .collect();
    assert!(
        record_labels.contains(&vec!["a".to_owned(), "z".to_owned()]),
        "Operation::Record must canonicalize field order: {record_labels:?}"
    );
    assert!(
        record_labels.iter().all(|ls| ls
            .windows(2)
            .all(|w| w[0].encode_utf16().le(w[1].encode_utf16()))),
        "all lowered Operation::Record instructions must be in canonical UTF-16 order: {record_labels:?}"
    );

    // Run both full record_order fixtures on the counted path in debug and release.
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in ["codegen/record_order", "codegen/record_order_std"] {
        let path = run_root.join(format!("{case}.fl"));
        let expected =
            fs::read_to_string(run_root.join(format!("{case}.stdout"))).expect("expected stdout");
        let compiled = run::compile(&path).expect("fixture compiles");
        let main = run::main_of(&compiled).expect("main export");
        for release in [false, true] {
            let ran = run::run_main_counted(&compiled, &main, 2_000_000, release)
                .unwrap_or_else(|err| panic!("{case} release={release}: {err}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0, "{case} release={release}");
            assert_eq!(ran.memory.allocations, ran.memory.frees, "{case}");
        }
    }
}

#[test]
fn counted_unsafe_record_schema_sorts_fields_canonically_with_zero_leaks() {
    // Direct Schema.unsafe_record with unsorted field lists canonicalizes
    // JSON encoding, JSON Schema export, and seeded Arbitrary sampling.
    let unsafe_rec_src = r#"
module App.Main exposing (main)

import Codec

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let s_za : Schema { a : String; z : Float } =
    Schema.unsafe_record [Schema.field "z" Schema.number, Schema.field "a" Schema.string]
  let s_az : Schema { a : String; z : Float } =
    Schema.unsafe_record [Schema.field "a" Schema.string, Schema.field "z" Schema.number]
  let c_za = Codec.json s_za
  let c_az = Codec.json s_az
  let json_za =
    match Codec.encode_result c_za { z = 1.5; a = "hi" } with
    | Ok w -> Json.stringify w
    | Err _ -> "err"
  let json_az =
    match Codec.encode_result c_az { a = "hi"; z = 1.5 } with
    | Ok w -> Json.stringify w
    | Err _ -> "err"
  let sch_eq =
    match (Codec.json_schema "R" c_za, Codec.json_schema "R" c_az) with
    | (Ok x, Ok y) -> x == y
    | _ -> false
  let opts : Arbitrary.SampleOptions = {
    count = Some 4
    size = None
    maxDiscards = None
    seed = Some 17
  }
  let g1 =
    try Arbitrary.sample opts (Arbitrary.from_schema s_za) with
    | Arbitrary.SampleError _ -> []
  let g2 =
    try Arbitrary.sample opts (Arbitrary.from_schema s_az) with
    | Arbitrary.SampleError _ -> []
  Platform.stdout_line (json_za ++ " | " ++ show (json_za == json_az, sch_eq, List.length g1 == 4 && g1 == g2))
"#;
    let compiled = run::compile_source(unsafe_rec_src).expect("unsafe_record source compiles");
    let main = run::main_of(&compiled).expect("main export");
    let ran = run::run_main_counted(&compiled, &main, 2_000_000, false)
        .expect("unsafe_record counted execution");
    assert_eq!(
        ran.stdout,
        "{\"a\":\"hi\",\"z\":1.5} | (true, true, true)\n"
    );
    assert_eq!(ran.memory.live_objects, 0);
    assert_eq!(ran.memory.allocations, ran.memory.frees);
}

#[test]
fn counted_filter_equals_strict_uses_structural_same_value_with_zero_leaks() {
    let source = r#"
module App.Main exposing (main)

import Codec

main : Unit ->{Host.Console} Unit
let main () =
  let r1 = { z = 1; a = "hi" }
  let r2 = { a = "hi"; z = 1 }
  let rec_ok = Filter.to_predicate (Filter.equals_strict r1) r2
  let tup_ok = Filter.to_predicate (Filter.equals_strict (1, r1)) (1, r2)
  let tup_no = Filter.to_predicate (Filter.equals_strict (1, r1)) (2, r2)
  let zero_diff = Filter.to_predicate (Filter.equals_strict 0.0) (-0.0)
  let neg_zero_same = Filter.to_predicate (Filter.equals_strict (-0.0)) (-0.0)
  let nan_same = Filter.to_predicate (Filter.equals_strict (0.0 / 0.0)) (0.0 / 0.0)
  let c1 = Ref.make 1
  let c2 = Ref.make 1
  let cell_same = Filter.to_predicate (Filter.equals_strict c1) c1
  let cell_diff = Filter.to_predicate (Filter.equals_strict c1) c2
  let num_codec = Codec.json Schema.number
  let patch_len =
    match (Codec.encode_result num_codec 0.0, Codec.encode_result num_codec (-0.0)) with
    | (Ok z0, Ok zn) -> List.length (JsonPatch.get z0 zn)
    | _ -> -1
  let d_ref_a = Record.from_entries [("$ref", "A")]
  let d_ref_b = Record.from_entries [("$ref", "B")]
  let dollar_diff = Filter.to_predicate (Filter.equals_strict d_ref_a) d_ref_b
  let json_dollar_len =
    match (Json.parse "{\"$ref\":\"A\"}", Json.parse "{\"$ref\":\"B\"}") with
    | (Ok ua, Ok ub) ->
      (
        match (Codec.decode_result Json.json ua, Codec.decode_result Json.json ub) with
        | (Ok ja, Ok jb) -> List.length (JsonPatch.get ja jb)
        | _ -> -1
      )
    | _ -> -2
  Platform.stdout_line (show (rec_ok, tup_ok, tup_no, zero_diff, neg_zero_same, nan_same, cell_same, cell_diff, patch_len, dollar_diff, json_dollar_len))
"#;
    let compiled = run::compile_source(source).expect("Filter.equals_strict source compiles");
    let main = run::main_of(&compiled).expect("main export");
    for release in [false, true] {
        let ran = run::run_main_counted(&compiled, &main, 2_000_000, release)
            .unwrap_or_else(|err| panic!("release={release}: {err}"));
        assert_eq!(
            ran.stdout, "(true, true, false, false, true, true, true, false, 1, false, 1)\n",
            "release={release}"
        );
        assert_eq!(ran.memory.live_objects, 0, "release={release}");
        assert_eq!(
            ran.memory.allocations, ran.memory.frees,
            "release={release}"
        );
    }
}
