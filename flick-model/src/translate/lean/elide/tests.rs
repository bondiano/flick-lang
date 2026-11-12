//! The generated `ElideSchemes.lean` and rule 1 of the scheme translation.

use std::{env, fs};

/// `ElideSchemes.lean` is what the model generates; with
/// `FLICK_FORMAL_SCHEMES` set the test writes it.
#[test]
fn elide_schemes_are_generated() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../formal/Formal/Live/ElideSchemes.lean"
    );
    let text = super::elide_schemes();
    if env::var_os("FLICK_FORMAL_SCHEMES").is_some() {
        fs::write(path, &text).expect("write ElideSchemes.lean");
    }
    let committed = fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == text,
        "ElideSchemes.lean is stale: run `just formal-schemes`"
    );
}

/// Rule 1 reads a bound on a variable of the scheme as a fact; a bound
/// it cannot read so is skipped with its reason.
#[test]
fn a_value_bound_is_a_fact_of_the_scheme() {
    use std::rc::Rc;

    use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

    let var = |n: &str| Var::Named(n.into());
    let a = Rc::new(Type::Var(var("a")));
    let unit = || Rc::new(Type::Tuple(Rc::new([])));
    let bounded = |t: Rc<Type>| Rc::new(Type::Bounded(t, Caps::atom(var("c"))));
    let scheme = |param: Rc<Type>, stage: Stage| {
        Scheme::new(
            vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
            Type::Fun(param, stage, unit()),
        )
    };
    let theorem = |s: &Scheme| super::theorem("t", "t", (s, s), None);
    let pure = theorem(&scheme(bounded(a.clone()), Stage::Pure)).expect("a fact");
    assert!(pure.contains("Scheme.elide (fun x => x = .v 1) ⟨2, Facts.bnd 0 (fun x => x = .v 1),"));
    let runtime = Stage::Runtime(Row::none(), Row::none());
    assert_eq!(
        theorem(&scheme(bounded(a), runtime)),
        Err("a value bound in a scheme with a runtime stage".into())
    );
    assert_eq!(
        theorem(&scheme(bounded(unit()), Stage::Pure)),
        Err("a value bound on a type that is not a variable".into())
    );
}

/// A row the model substitutes for a row parameter (`Failure e`) is written
/// as the core instantiates the parameter, `.row`, so the instance of
/// `Failure (.tvar 0)` at `[.row r]` is the type written; `Errors e` given to
/// a type parameter (`Result (Errors e) a`) stays the type `.errors`.
#[test]
fn a_row_given_to_a_row_parameter_is_its_instance() {
    use std::collections::HashMap;
    use std::rc::Rc;

    use crate::check::{Inst, substitute};
    use crate::syntax::Path;
    use crate::types::{Kind, Row, Type, Var};

    let app = |name: &str, a: Type| Type::App(Rc::new(Type::con("Prelude", name)), Rc::new(a));
    let e = || Type::Var(Var::Named("e".into()));
    let at = HashMap::from([("e".into(), Inst::Row(Row::none()))]);
    let env = super::Env {
        types: Some(Rc::new(super::Types {
            params: HashMap::from([
                (Path::new("Prelude", "Failure"), vec![Kind::Errors]),
                (Path::new("Prelude", "Option"), vec![Kind::Type]),
            ]),
            ..super::Types::default()
        })),
        ..super::Env::default()
    };
    let written = |t: &Type| super::ty(&substitute(t, &at), &env);
    assert_eq!(
        written(&app("Failure", e())),
        Ok("(.tapp (.adt \"Prelude.Failure\") (.row Row.none))".into())
    );
    assert_eq!(
        written(&app(
            "Option",
            Type::Errors(Row::var(Var::Named("e".into())))
        )),
        Ok("(.tapp (.adt \"Prelude.Option\") (.errors Row.none))".into())
    );
}

/// A value of data with hidden storage carries the capture row it hides as
/// its first argument, as `Seq`; a bare one hides none.
#[test]
fn data_with_hidden_storage_shows_what_its_value_hides() {
    use std::collections::HashSet;
    use std::rc::Rc;

    use crate::syntax::Path;
    use crate::types::{Caps, Type, Var};

    let env = super::Env {
        vars: vec!["c".into()],
        types: Some(Rc::new(super::Types {
            stored: HashSet::from([Path::new("Fixture", "Sealed")]),
            ..super::Types::default()
        })),
        ..super::Env::default()
    };
    let sealed = || Rc::new(Type::con("Fixture", "Sealed"));
    let hidden = Type::Captured(sealed(), Caps::var(Var::Named("c".into())));
    assert_eq!(
        super::ty(&hidden, &env),
        Ok("(.tapp (.adt \"Fixture.Sealed\") (.caps (fun x => x = .v 0)))".into())
    );
    assert_eq!(
        super::ty(&sealed(), &env),
        Ok("(.tapp (.adt \"Fixture.Sealed\") (.caps Caps.none))".into())
    );
}
