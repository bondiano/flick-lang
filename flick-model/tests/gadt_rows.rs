//! Row indices constrain every solution of a constructor result, including
//! nonempty residual rows and capture rows shared across a branch.

use std::path::Path;

use flick_model::run;
use flick_model::{
    check::Checker,
    decls::Decls,
    syntax::{Path as ValuePath, Term},
    types::{Caps, Kind, Row, Scheme, Stage, Type, Var},
};
use std::rc::Rc;

#[test]
fn row_indexed_branches_check_in_the_independent_model() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/types/gadt_rows.fl");
    let modules = run::compile(&path).expect("the compiler accepts the fixture");
    let errors = run::check(&modules, &run::program(&modules));
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

fn callback() -> Type {
    Type::fun(
        Type::con("Prelude", "Unit"),
        Stage::Pure,
        Type::con("Prelude", "Int"),
    )
}

fn captures_parameter(result: Type) -> Scheme {
    Scheme::new(
        vec![("c".into(), Kind::Captures)],
        Type::fun(
            Type::Captured(Rc::new(callback()), Caps::var(Var::Named("c".into()))),
            Stage::Pure,
            result,
        ),
    )
}

#[test]
fn a_capture_variable_cannot_be_forgotten() {
    let decls = Decls::default();
    let result = Checker::new(&decls).check_top(
        &Term::lam("f", Term::Var("f".into())),
        &captures_parameter(callback()),
    );
    assert!(
        result.is_err(),
        "a closure holding c cannot become a closed callback"
    );
}

#[test]
fn an_inferred_eta_parameter_preserves_its_capture_qualifier() {
    let mut decls = Decls::default();
    let name = ValuePath::new("Test", "consume");
    let scheme = captures_parameter(Type::con("Prelude", "Unit"));
    decls.values.insert(name.clone(), scheme.clone());
    let eta = Term::lam(
        "arg",
        Term::app(Term::Global(name), Term::Var("arg".into())),
    );
    let term = Term::lam("f", Term::app(eta, Term::Var("f".into())));
    Checker::new(&decls)
        .check_top(&term, &scheme)
        .expect("eta expansion retains c");
}

#[test]
fn existing_gadt_constructor_bounds_keep_their_payload_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/types/gadt.fl");
    let modules = run::compile(&root).expect("the compiler accepts ordinary GADTs");
    let errors = run::check(&modules, &run::program(&modules));
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn matching_assumes_a_constructors_proven_payload_bounds() {
    let modules=run::compile_source("type Showable =\n  | Show b => Showable b : Showable\n\ndescribe : Showable -> String\nlet describe s =\n  match s with\n  | Showable x -> show x\n").expect("a supported existential constructor");
    let errors = run::check(&modules, &run::program(&modules));
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn value_positions_preserve_the_captures_their_type_arguments_show() {
    for fixture in [
        "classes/functor",
        "regions/captures",
        "regions/eta_payload",
        "regions/instance_elided",
    ] {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../tests/check/pass/{fixture}.fl"));
        let modules = run::compile(&path).expect("a supported value-bound program");
        let mut program = run::program(&modules);
        if fixture == "regions/captures" {
            // This fixture also declares external host APIs outside the model.
            // The regression here is its independently checked Functor instance.
            program.globals.retain(|p, _| {
                &*p.module == "App.Captures" && &*p.name == "instance Functor Option"
            });
            assert_eq!(program.globals.len(), 1, "the intended instance is present");
        }
        let errors = run::check(&modules, &program);
        assert!(errors.is_empty(), "{fixture}: {}", errors.join("\n"));
    }
}

#[test]
fn an_open_row_cannot_hide_a_second_service_instance() {
    let store = ValuePath::new("Test", "Store");
    let row = |ty| Row {
        elems: [(store.clone(), Rc::from([Type::con("Prelude", ty)]))].into(),
        ..Row::none()
    };
    let mut result = row("Int");
    result.vars.insert(Var::Named("r".into()));
    let packet = |r| {
        Type::App(
            Rc::new(Type::con("Test", "Packet")),
            Rc::new(Type::Errors(r)),
        )
    };
    let unit = Type::con("Prelude", "Unit");
    let task = |r| Type::Eff(r, Row::none(), Rc::new(unit.clone()));
    let name = ValuePath::new("Test", "pack");
    let mut decls = Decls::default();
    decls.values.insert(
        name.clone(),
        Scheme::new(
            vec![("r".into(), Kind::Services)],
            Type::fun(
                task(Row::var(Var::Named("r".into()))),
                Stage::Pure,
                packet(result),
            ),
        ),
    );
    let term = Term::lam(
        "work",
        Term::app(Term::Global(name), Term::Var("work".into())),
    );
    let scheme = Scheme::new(
        vec![],
        Type::fun(task(row("Bool")), Stage::Pure, packet(row("Int"))),
    );
    let matching = Scheme::new(
        vec![],
        Type::fun(task(row("Int")), Stage::Pure, packet(row("Int"))),
    );
    Checker::new(&decls)
        .check_top(&term, &matching)
        .expect("the repeated Store Int instance is well formed");
    let error = Checker::new(&decls)
        .check_top(&term, &scheme)
        .expect_err("Store Bool cannot disappear behind the explicit Store Int");
    assert_eq!(error.rule, "≡", "{error}");
}
