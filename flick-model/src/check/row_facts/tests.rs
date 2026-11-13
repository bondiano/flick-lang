use super::*;
use crate::decls::Decls;

fn variable(name: &str) -> Row {
    Row::var(Var::Named(name.into()))
}

#[test]
fn absorption_keeps_every_residual_solution() {
    let decls = Decls::default();
    let mut c = Checker::new(&decls);
    let r = variable("r");
    let db = Row::one(Path::new("Test", "Db"));
    c.row_equalities.push((
        r.clone(),
        db.sum(&r).expect("one nominal instance and a tail"),
    ));
    assert!(c.row_implied(&db, &r));
    assert!(!c.row_implied(&r, &Row::none()));

    c.row_equalities.clear();
    c.row_equalities.push((
        db.sum(&r).expect("one nominal instance and a tail"),
        db.clone(),
    ));
    assert!(c.row_implied(&r, &db));
    assert!(!c.row_implied(&r, &Row::none()));
    assert!(c.within(&r, &Row::none()).is_err());
}

#[test]
fn nominal_arguments_are_invariant_with_independent_row_facts() {
    let decls = Decls::default();
    let mut c = Checker::new(&decls);
    let instance = |name| Row {
        elems: [(
            Path::new("Test", "Store"),
            Rc::from([Type::con("Prelude", name)]),
        )]
        .into(),
        ..Row::none()
    };
    let (r, s) = (variable("r"), variable("s"));
    let (ints, strings) = (instance("Int"), instance("String"));
    c.row_equalities
        .extend([(r.clone(), ints.clone()), (s.clone(), strings.clone())]);
    assert!(c.row_implied(&ints, &r));
    assert!(c.row_implied(&strings, &s));
    assert!(!c.row_implied(&r, &s));
    assert!(!c.row_implied(&ints, &s));
}

#[test]
fn deferred_capture_checks_keep_their_branch_facts() {
    let decls = Decls::default();
    let mut c = Checker::new(&decls);
    let (a, b) = (
        Caps::var(Var::Named("a".into())),
        Caps::var(Var::Named("b".into())),
    );
    c.cap_equalities.push((a.clone(), b.clone()));
    c.hold(
        &[Type::Caps(a.clone())],
        &[Type::Caps(b.clone())],
        &Caps::default(),
        "≤-Cap",
    );
    c.cap_equalities.clear();
    assert!(!c.caps_implied(&a, &b));
    c.solve_regions().expect("the deferred check keeps a=b");

    c.hold(&[Type::Caps(a)], &[], &Caps::default(), "≤-Cap");
    assert!(
        c.solve_regions().is_err(),
        "another check cannot forget a capture row"
    );
}
