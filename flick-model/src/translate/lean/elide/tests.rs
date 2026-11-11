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
    let theorem = |s: &Scheme| super::theorem("t", "t", s, s);
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
