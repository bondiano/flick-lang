//! Regression tests for issue #696: the model preserves the expected type of
//! lowered operands and the scheme of local signatures.

use std::collections::HashSet;
use std::path::Path as FilePath;
use std::rc::Rc;

use flick_model::{
    check::{Checker, TypeError, check_program},
    decls::Decls,
    run,
    syntax::{Arm, Con, Const, Lit, Name, Pat, Path, Term},
    types::{Fields, Kind, Row, Scheme, Stage, Type, Var},
};

fn declared_external(error: &TypeError) -> bool {
    error.rule == "Translation" && error.message.starts_with("no native for")
}

fn check_source(source: &str) -> Vec<(Path, TypeError)> {
    let modules = run::compile_source(source).expect("flick-core accepts program");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(_, e)| !declared_external(e))
        .collect()
}

#[test]
fn lowered_operand_before_with_owner_evidence_checks_rank_two_record_field() {
    let source = r#"module App.Main exposing (main)

acquire : String ->{Scope s} String
let acquire name = name

main : Unit ->{Async; RcMap.ExceededCapacity} Unit
let main () =
  let full = scope s {
    let map : RcMap.RcMap s String none String =
      RcMap.make {
        lookup = fun (key : String) -> acquire key
        idleTimeToLive = Some (fun _ -> Duration.zero)
        capacity = None
      }
    "x"
  }
  Host.raw_stdout_line full
"#;
    let errors = check_source(source);
    assert!(
        errors.is_empty(),
        "lowered operand lost rank-2 expectation: {errors:?}"
    );
}

#[test]
fn lowered_operand_with_effectful_sibling_field_and_pipe_checks_rank_two_field() {
    let source = r#"module App.Main exposing (main)

acquire : String ->{Scope s} String
let acquire name = name

next_cap : Unit ->{} Int
let next_cap () = 4

main : Unit ->{Async; RcMap.ExceededCapacity} Unit
let main () =
  let full = scope s {
    let map : RcMap.RcMap s String none String =
      RcMap.make {
        lookup = fun (key : String) -> acquire key
        idleTimeToLive = Some (fun _ -> Duration.zero)
        capacity = Some (next_cap ())
      }
    let piped : RcMap.RcMap s String none String =
      RcMap.make <| {
        lookup = fun (key : String) -> acquire key
        idleTimeToLive = Some (fun _ -> Duration.zero)
        capacity = Some (next_cap ())
      }
    "x"
  }
  Host.raw_stdout_line full
"#;
    let errors = check_source(source);
    assert!(
        errors.is_empty(),
        "lowered operand with effectful field or pipe failed: {errors:?}"
    );
}

#[test]
fn local_polymorphic_recursion_fixture_checks_in_model() {
    let file =
        FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/types/recursion.fl");
    let modules = run::compile(&file).expect("recursion.fl compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "local polymorphic recursion failed in model: {errors:?}"
    );
}

#[test]
fn stdlib_effect_cache_checks_in_model() {
    let file =
        FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/stdlib/effect/cache.fl");
    let modules = run::compile(&file).expect("cache.fl compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "cache.fl failed in model: {errors:?}");
}

#[test]
fn local_signatures_with_enclosing_tyvars_scopes_and_constraints_check() {
    let source = r#"module App.LocalSigs exposing (wrap_poly, nested_scopes, local_constrained, local_rec_capture, inline_rank2)

open_res : String ->{Scope s} String
let open_res name =
  Resource.acquire_release (task { name }) (fun _ -> ())

wrap_poly : forall a r e. (a ->{r; e} a) -> a ->{r; e} a
let wrap_poly f x =
  step : a ->{r; e} a
  let step y = f y
  step x

nested_scopes : Unit ->{} String
let nested_scopes () = scope s {
  outer_acq : Unit ->{Scope s} String
  let outer_acq () = open_res "outer"
  scope s {
    inner_acq : Unit ->{Scope s} String
    let inner_acq () = open_res "inner"
    let _ = inner_acq ()
    ()
  }
  outer_acq ()
}

local_constrained : Int -> String
let local_constrained n =
  render : Show a => a -> String
  let render x = show x
  render n ++ render true

local_rec_capture : Unit ->{} Unit
let local_rec_capture () = scope s {
  let h = open_res "held"
  go : Int ->{} Unit
  let rec go n =
    if n <= 0 then ()
    else
      Host.raw_stdout_line h
      go (n - 1)
  go 2
}

inline_rank2 : Unit -> (Int, String)
let inline_rank2 () =
  let box : { id : forall a. a -> a } = { id = fun x -> x }
  (box.id 1, box.id "ok")
"#;
    let errors = check_source(source);
    assert!(
        errors.is_empty(),
        "local signature scenarios failed in model: {errors:?}"
    );
}

fn scope_row(region: &str) -> Row {
    let mut row = Row::none();
    row.elems.insert(
        Path::new("Prelude", "Scope"),
        Rc::from([Type::Var(Var::Named(region.into()))]),
    );
    row
}

fn scoped_unit_sig() -> Scheme {
    let unit = Type::con("Prelude", "Unit");
    Scheme::new(
        Vec::new(),
        Type::fun(unit.clone(), Stage::Runtime(Row::none(), Row::none()), unit),
    )
}

fn rank_two_make_decls() -> Decls {
    let unit = Type::con("Prelude", "Unit");
    let int = Type::con("Prelude", "Int");
    let mut decls = Decls::default();
    let make_record = Type::Record(Fields {
        fields: [
            (
                "lookup".into(),
                Type::Forall(Rc::new(Scheme::new(
                    vec![("t".into(), Kind::Region)],
                    Type::fun(
                        unit.clone(),
                        Stage::Runtime(scope_row("t"), Row::none()),
                        unit.clone(),
                    ),
                ))),
            ),
            ("extra".into(), int),
        ]
        .into_iter()
        .collect(),
        tail: None,
    });
    decls.values.insert(
        Path::new("T", "make"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                make_record,
                Stage::Runtime(scope_row("s"), Row::none()),
                unit.clone(),
            ),
        ),
    );
    decls.values.insert(
        Path::new("T", "acquire_in"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                Type::app(
                    Type::con("Fiber", "Owner"),
                    Type::Var(Var::Named("s".into())),
                ),
                Stage::Runtime(scope_row("s"), Row::none()),
                unit.clone(),
            ),
        ),
    );
    decls.values.insert(
        Path::new("T", "acquire_any"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                unit.clone(),
                Stage::Runtime(scope_row("s"), Row::none()),
                unit,
            ),
        ),
    );
    decls
}

#[test]
fn non_polymorphic_lowered_operand_is_still_rejected_by_model() {
    let decls = rank_two_make_decls();
    let sig = scoped_unit_sig();

    // Record field `lookup` expects `forall t. Unit ->{Scope t} Unit`,
    // lowered before `WithOwnerEvidence`, given a closure fixed to enclosing `Scope s`.
    let non_poly_record = Term::Record(Rc::from([
        (
            "lookup".into(),
            Term::lam(
                "_u",
                Term::app(
                    Term::Global(Path::new("T", "acquire_in")),
                    Term::Var("s".into()),
                ),
            ),
        ),
        ("extra".into(), Term::Lit(Lit::Int(1))),
    ]));
    let lowered_call = Term::Let(
        "$operand".into(),
        None,
        Rc::new(Term::Global(Path::new("T", "make"))),
        Rc::new(Term::Let(
            "$operand$1".into(),
            None,
            Rc::new(non_poly_record),
            Rc::new(Term::WithOwnerEvidence(
                Rc::new(Term::Var("s".into())),
                Rc::new(Term::app(
                    Term::Var("$operand".into()),
                    Term::Var("$operand$1".into()),
                )),
            )),
        )),
    );
    let body = Term::lam("_", Term::Scope("s".into(), Rc::new(lowered_call)));
    let err = Checker::new(&decls)
        .check_top(&body, &sig)
        .expect_err("closure fixed to outer Scope s must not satisfy forall t");
    assert_eq!(err.rule, "≡", "{err:?}");

    // User-written `let cfg = ...; make cfg` does NOT inherit `make`'s parameter type.
    let user_record = Term::Record(Rc::from([
        (
            "lookup".into(),
            Term::lam(
                "_u",
                Term::app(Term::Global(Path::new("T", "acquire_any")), Term::unit()),
            ),
        ),
        ("extra".into(), Term::Lit(Lit::Int(1))),
    ]));
    let user_let_call = Term::Let(
        "cfg".into(),
        None,
        Rc::new(user_record),
        Rc::new(Term::Let(
            "$operand".into(),
            None,
            Rc::new(Term::Global(Path::new("T", "make"))),
            Rc::new(Term::WithOwnerEvidence(
                Rc::new(Term::Var("s".into())),
                Rc::new(Term::app(
                    Term::Var("$operand".into()),
                    Term::Var("cfg".into()),
                )),
            )),
        )),
    );
    let user_let_body = Term::lam("_", Term::Scope("s".into(), Rc::new(user_let_call)));
    let err = Checker::new(&decls)
        .check_top(&user_let_body, &sig)
        .expect_err("user-written unsigned let must not receive callee's rank-2 expectation");
    assert_eq!(err.rule, "≡", "{err:?}");
}

#[test]
fn non_polymorphic_direct_lowered_operand_is_rejected() {
    let unit = Type::con("Prelude", "Unit");
    let sig = scoped_unit_sig();
    let mut decls = Decls::default();

    // Direct non-polymorphic argument `fun x -> 1` for `forall a. a -> a`.
    decls.values.insert(
        Path::new("T", "use_id"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                Type::Forall(Rc::new(Scheme::new(
                    vec![("a".into(), Kind::Type)],
                    Type::fun(
                        Type::Var(Var::Named("a".into())),
                        Stage::Pure,
                        Type::Var(Var::Named("a".into())),
                    ),
                ))),
                Stage::Runtime(scope_row("s"), Row::none()),
                unit,
            ),
        ),
    );
    let bad_id_call = Term::Let(
        "$operand".into(),
        None,
        Rc::new(Term::Global(Path::new("T", "use_id"))),
        Rc::new(Term::Let(
            "$operand$1".into(),
            None,
            Rc::new(Term::lam("x", Term::Lit(Lit::Int(1)))),
            Rc::new(Term::WithOwnerEvidence(
                Rc::new(Term::Var("s".into())),
                Rc::new(Term::app(
                    Term::Var("$operand".into()),
                    Term::Var("$operand$1".into()),
                )),
            )),
        )),
    );
    let bad_id_body = Term::lam("_", Term::Scope("s".into(), Rc::new(bad_id_call)));
    let err = Checker::new(&decls)
        .check_top(&bad_id_body, &sig)
        .expect_err("monomorphic function must not satisfy forall a. a -> a");
    assert_eq!(err.rule, "≡", "{err:?}");
}

#[test]
fn effectful_polymorphic_lowered_operand_is_rejected_by_let_sig() {
    let unit = Type::con("Prelude", "Unit");
    let sig = scoped_unit_sig();
    let mut decls = Decls::default();

    // Effectful lowered operand for a `forall` parameter is rejected by (Let-Sig).
    let none_con = Rc::new(Con {
        ty: Path::new("Prelude", "Option"),
        name: "None".into(),
        arity: 0,
        dicts: 0,
    });
    decls.constructors.insert(
        Path::new("Prelude", "None"),
        Scheme::new(
            vec![("a".into(), Kind::Type)],
            Type::app(
                Type::con("Prelude", "Option"),
                Type::Var(Var::Named("a".into())),
            ),
        ),
    );
    decls.values.insert(
        Path::new("T", "use_poly_ref"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                Type::Forall(Rc::new(Scheme::new(
                    vec![("a".into(), Kind::Type)],
                    Type::Ref(Rc::new(Type::app(
                        Type::con("Prelude", "Option"),
                        Type::Var(Var::Named("a".into())),
                    ))),
                ))),
                Stage::Runtime(scope_row("s"), Row::none()),
                unit,
            ),
        ),
    );
    let effectful_poly = Term::Let(
        "$operand".into(),
        None,
        Rc::new(Term::Global(Path::new("T", "use_poly_ref"))),
        Rc::new(Term::Let(
            "$operand$1".into(),
            None,
            Rc::new(Term::app(
                Term::konst(Const::RefMake),
                Term::konst_con(none_con),
            )),
            Rc::new(Term::WithOwnerEvidence(
                Rc::new(Term::Var("s".into())),
                Rc::new(Term::app(
                    Term::Var("$operand".into()),
                    Term::Var("$operand$1".into()),
                )),
            )),
        )),
    );
    let effectful_body = Term::lam("_", Term::Scope("s".into(), Rc::new(effectful_poly)));
    let err = Checker::new(&decls)
        .check_top(&effectful_body, &sig)
        .expect_err("runtime step building a polymorphic lowered operand must fail");
    assert_eq!(err.rule, "Let-Sig", "{err:?}");
}

#[test]
fn local_signature_mismatch_is_rejected_by_model() {
    let decls = Decls::default();
    let unit = Type::con("Prelude", "Unit");
    let int = Type::con("Prelude", "Int");
    let sig = Scheme::new(Vec::new(), Type::fun(unit, Stage::Pure, int.clone()));

    // Non-recursive local `let` claiming `forall a. a -> a` implemented by `fun _ -> 1`.
    let bad_poly = Scheme::new(
        vec![("a".into(), Kind::Type)],
        Type::fun(
            Type::Var(Var::Named("a".into())),
            Stage::Pure,
            Type::Var(Var::Named("a".into())),
        ),
    );
    let bad_let = Term::lam(
        "_",
        Term::Let(
            "id".into(),
            Some(Rc::new(bad_poly.clone())),
            Rc::new(Term::lam("_x", Term::Lit(Lit::Int(1)))),
            Rc::new(Term::app(Term::Var("id".into()), Term::Lit(Lit::Int(2)))),
        ),
    );
    let err = Checker::new(&decls)
        .check_top(&bad_let, &sig)
        .expect_err("local signature mismatch on let must fail");
    assert_eq!(err.rule, "≡", "{err:?}");

    // Recursive local `let rec` claiming `forall a. a -> a` implemented by `fun _ -> true`.
    let bad_let_rec = Term::lam(
        "_",
        Term::LetRec(
            "go".into(),
            Some(Rc::new(bad_poly)),
            Rc::new(Term::lam("_x", Term::Lit(Lit::Bool(true)))),
            Rc::new(Term::app(Term::Var("go".into()), Term::Lit(Lit::Int(2)))),
        ),
    );
    let err = Checker::new(&decls)
        .check_top(&bad_let_rec, &sig)
        .expect_err("local signature mismatch on let rec must fail");
    assert_eq!(err.rule, "≡", "{err:?}");

    // Recursive local `let rec` claiming `Int -> Int` returning `Bool` must fail.
    let bad_mono = Scheme::new(Vec::new(), Type::fun(int.clone(), Stage::Pure, int));
    let bad_mono_rec = Term::lam(
        "_",
        Term::LetRec(
            "go".into(),
            Some(Rc::new(bad_mono)),
            Rc::new(Term::lam(
                "n",
                Term::Match(
                    Rc::new(Term::Var("n".into())),
                    Rc::from([Arm {
                        pat: Pat::Wild,
                        guard: None,
                        body: Term::Lit(Lit::Bool(false)),
                    }]),
                ),
            )),
            Rc::new(Term::app(Term::Var("go".into()), Term::Lit(Lit::Int(0)))),
        ),
    );
    let err = Checker::new(&decls)
        .check_top(&bad_mono_rec, &sig)
        .expect_err("monomorphic local let rec mismatch must fail");
    assert_eq!(err.rule, "≡", "{err:?}");
}
