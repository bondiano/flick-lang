//! §1.4 under a `∀` of a signature: a result holds what the parameters
//! before it reach of the atoms outside the `∀`, never one of its binders
//! (`Ty.elide` of `.all`, `Caps.under` in `formal/Formal/Live/Elide.lean`).

use std::rc::Rc;

use super::Checker;
use crate::decls::Decls;
use crate::syntax::{Path, Term};
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

fn var(n: &str) -> Type {
    Type::Var(Var::Named(n.into()))
}

fn file(region: &str) -> Type {
    Type::app(Type::con("File", "File"), var(region))
}

/// `type G = G (Unit ->{…} Int)`: data with hidden storage.
fn g() -> Type {
    Type::con("T", "G")
}

fn decls() -> Decls {
    let mut d = Decls::default();
    let field = Type::fun(
        Type::con("Prelude", "Unit"),
        Stage::Runtime(Row::none(), Row::none()),
        Type::con("Prelude", "Int"),
    );
    let ctor = Scheme::new(Vec::new(), Type::fun(field, Stage::Pure, g()));
    d.constructors.insert(Path::new("T", "G"), ctor);
    d.variants
        .insert(Path::new("T", "G"), vec![("G".into(), 1)]);
    d
}

fn forall(vars: &[(&str, Kind)], body: Type) -> Type {
    let vars = vars.iter().map(|(n, k)| ((*n).into(), k.clone())).collect();
    Type::Forall(Rc::new(Scheme::new(vars, body)))
}

fn read(body: Type) -> Type {
    let d = decls();
    let s = Scheme::new(vec![("s".into(), Kind::Region)], body);
    Checker::new(&d).live_scheme(&s).body
}

fn held(t: Type, region: &str) -> Type {
    Type::Captured(Rc::new(t), Caps::atom(Var::Named(region.into())))
}

#[test]
fn under_a_forall_only_outer_atoms_are_held() {
    let int = || Type::con("Prelude", "Int");
    // `(forall t. File t -> G) -> File s -> Int`: `G` holds nothing of `t`
    let own = forall(
        &[("t", Kind::Region)],
        Type::fun(file("t"), Stage::Pure, g()),
    );
    let t = Type::fun(
        own.clone(),
        Stage::Pure,
        Type::fun(file("s"), Stage::Pure, int()),
    );
    assert_eq!(read(t.clone()), t);
    // `(forall a. File s -> a -> G) -> Int`: `G` holds `s`
    let outer = |g| {
        let inner = Type::fun(file("s"), Stage::Pure, Type::fun(var("a"), Stage::Pure, g));
        Type::fun(forall(&[("a", Kind::Type)], inner), Stage::Pure, int())
    };
    assert_eq!(read(outer(g())), outer(held(g(), "s")));
    // `File s -> (forall s. s -> G)`: the inner `s` is renamed, `G` holds the outer one
    let shadowed = Type::fun(
        file("s"),
        Stage::Pure,
        forall(&[("s", Kind::Type)], Type::fun(var("s"), Stage::Pure, g())),
    );
    let renamed = Type::fun(
        file("s"),
        Stage::Pure,
        forall(
            &[("s$0", Kind::Type)],
            Type::fun(var("s$0"), Stage::Pure, held(g(), "s")),
        ),
    );
    assert_eq!(read(shadowed), renamed);
}

#[test]
fn qualified_rank2_and_curried_keep_their_qualifier() {
    let mut d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let runtime = || Stage::Runtime(Row::none(), Row::none());
    let use_sig = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(file("s"), runtime(), unit()),
    );
    d.values.insert(Path::new("T", "use_file"), use_sig);

    let rank2 = |q: Option<&str>| {
        let f = forall(
            &[("a", Kind::Type)],
            Type::fun(unit(), Stage::Pure, Type::fun(var("a"), runtime(), unit())),
        );
        match q {
            Some(s) => held(f, s),
            None => f,
        }
    };
    // `drop_it : (forall a. Unit -> a ->{} Unit) -> Unit` (bare parameter, no `s`)
    d.values.insert(
        Path::new("T", "drop_it"),
        Scheme::new(Vec::new(), Type::fun(rank2(None), Stage::Pure, unit())),
    );
    // `take_fn : (Unit -> (forall a. Unit -> a ->{} Unit)) -> Unit` (tests Forall <= Forall)
    d.values.insert(
        Path::new("T", "take_fn"),
        Scheme::new(
            Vec::new(),
            Type::fun(
                Type::fun(unit(), Stage::Pure, rank2(None)),
                Stage::Pure,
                unit(),
            ),
        ),
    );

    // 1a. Passing a qualified rank-2 function to a bare rank-2 parameter is rejected.
    let mut c = Checker::new(&d);
    let sig_reject = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(rank2(Some("s")), Stage::Pure, unit()),
    );
    let body_drop = Term::lam(
        "f",
        Term::app(
            Term::Global(Path::new("T", "drop_it")),
            Term::Var("f".into()),
        ),
    );
    assert!(
        c.check_top(&body_drop, &sig_reject).is_err(),
        "dropping a rank-2 qualifier by passing to a bare rank-2 parameter must fail"
    );

    // 1b. Returning a qualified rank-2 function where a bare rank-2 result is expected fails.
    let mut c = Checker::new(&d);
    let body_ho = Term::lam(
        "f",
        Term::app(
            Term::Global(Path::new("T", "take_fn")),
            Term::lam("_", Term::Var("f".into())),
        ),
    );
    assert!(
        c.check_top(&body_ho, &sig_reject).is_err(),
        "dropping a rank-2 qualifier under `Forall <= Forall` must fail"
    );

    // 2. Checking a curried rank-2 lambda that uses `File s` against a closed-row
    //    qualified rank-2 type succeeds.
    let mut c = Checker::new(&d);
    let sig_accept = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(file("s"), Stage::Pure, rank2(Some("s"))),
    );
    let body_make = Term::lam(
        "h",
        Term::lam(
            "_u",
            Term::lam(
                "_x",
                Term::app(
                    Term::Global(Path::new("T", "use_file")),
                    Term::Var("h".into()),
                ),
            ),
        ),
    );
    let res = c.check_top(&body_make, &sig_accept);
    assert!(
        res.is_ok(),
        "curried rank-2 closure using `s` against `(forall a. Unit -> a ->{{}} Unit)^{{s}}` must pass: {res:?}"
    );
}

#[test]
fn bounded_curried_callable_keeps_its_region_bound() {
    let mut d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let runtime = || Stage::Runtime(Row::none(), Row::none());
    let curried = || Type::fun(unit(), Stage::Pure, Type::fun(unit(), runtime(), unit()));
    let bounded_curried = Type::Bounded(Rc::new(curried()), Caps::atom(Var::Named("s".into())));

    // Applying and subsuming a Type::Bounded curried callable must keep its bound live.
    d.values.insert(
        Path::new("T", "run_curried"),
        Scheme::new(Vec::new(), Type::fun(curried(), Stage::Pure, unit())),
    );
    d.values.insert(
        Path::new("T", "take_thunk"),
        Scheme::new(
            Vec::new(),
            Type::fun(Type::fun(unit(), runtime(), unit()), Stage::Pure, unit()),
        ),
    );
    let mut c = Checker::new(&d);
    let body_sub_bounded = Term::lam(
        "f",
        Term::app(
            Term::Global(Path::new("T", "run_curried")),
            Term::Var("f".into()),
        ),
    );
    assert!(
        c.check_top(
            &body_sub_bounded,
            &Scheme::new(
                vec![("s".into(), Kind::Region)],
                Type::fun(bounded_curried.clone(), Stage::Pure, unit()),
            ),
        )
        .is_err(),
        "subsuming a Type::Bounded curried callable to a bare one must fail"
    );

    let mut c = Checker::new(&d);
    let body_call_bounded = Term::lam(
        "f",
        Term::app(
            Term::Global(Path::new("T", "take_thunk")),
            Term::lam(
                "_",
                Term::app(Term::app(Term::Var("f".into()), Term::unit()), Term::unit()),
            ),
        ),
    );
    assert!(
        c.check_top(
            &body_call_bounded,
            &Scheme::new(
                vec![("s".into(), Kind::Region)],
                Type::fun(bounded_curried, Stage::Pure, unit()),
            ),
        )
        .is_err(),
        "calling a Type::Bounded curried callable inside a bare thunk must fail"
    );
}

#[test]
fn forall_subsumption_enforces_kinds_and_outlives() {
    let d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let wrap = |t: Type| Type::fun(unit(), Stage::Pure, t);

    // Forall <= Forall (nested under a function so check does not peel it)
    // must reject binder kind mismatches.
    let mut c = Checker::new(&d);
    let poly_type = forall(&[("a", Kind::Type)], Type::fun(unit(), Stage::Pure, unit()));
    let poly_region = forall(
        &[("r", Kind::Region)],
        Type::fun(unit(), Stage::Pure, unit()),
    );
    let sig_kind_mismatch = Scheme::new(
        Vec::new(),
        Type::fun(wrap(poly_type), Stage::Pure, wrap(poly_region)),
    );
    assert!(
        c.check_top(&Term::lam("f", Term::Var("f".into())), &sig_kind_mismatch)
            .is_err(),
        "Forall <= Forall must reject mismatched binder kinds"
    );

    // Forall <= Forall must check outlives constraints contravariantly.
    let constrained = Type::Forall(Rc::new(Scheme {
        vars: vec![("t".into(), Kind::Region)],
        outlives: vec![(
            Caps::atom(Var::Named("s".into())),
            Type::Var(Var::Named("t".into())),
        )],
        body: Type::fun(unit(), Stage::Pure, unit()),
    }));
    let unconstrained = forall(
        &[("t", Kind::Region)],
        Type::fun(unit(), Stage::Pure, unit()),
    );
    let mut c = Checker::new(&d);
    let sig_drop_outlives = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(wrap(constrained.clone()), Stage::Pure, wrap(unconstrained)),
    );
    assert!(
        c.check_top(&Term::lam("f", Term::Var("f".into())), &sig_drop_outlives)
            .is_err(),
        "Forall <= Forall must reject dropping an unprovable Outlives constraint"
    );
    let mut c = Checker::new(&d);
    let sig_keep_outlives = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(
            wrap(constrained.clone()),
            Stage::Pure,
            wrap(constrained.clone()),
        ),
    );
    assert!(
        c.check_top(&Term::lam("f", Term::Var("f".into())), &sig_keep_outlives)
            .is_ok(),
        "Forall <= Forall with matching Outlives constraint must pass"
    );

    // Invariant unification under Ref must reject differing Outlives constraints.
    let constrained_s1 = Type::Forall(Rc::new(Scheme {
        vars: vec![("t".into(), Kind::Region)],
        outlives: vec![(
            Caps::atom(Var::Named("s1".into())),
            Type::Var(Var::Named("t".into())),
        )],
        body: Type::fun(unit(), Stage::Pure, unit()),
    }));
    let constrained_s2 = Type::Forall(Rc::new(Scheme {
        vars: vec![("t".into(), Kind::Region)],
        outlives: vec![(
            Caps::atom(Var::Named("s2".into())),
            Type::Var(Var::Named("t".into())),
        )],
        body: Type::fun(unit(), Stage::Pure, unit()),
    }));
    let mut c = Checker::new(&d);
    let sig_swap_ref = Scheme::new(
        vec![("s1".into(), Kind::Region), ("s2".into(), Kind::Region)],
        Type::fun(
            Type::Ref(Rc::new(constrained_s1)),
            Stage::Pure,
            Type::Ref(Rc::new(constrained_s2)),
        ),
    );
    assert!(
        c.check_top(&Term::lam("r", Term::Var("r".into())), &sig_swap_ref)
            .is_err(),
        "Ref unification of Forall schemes must reject differing Outlives constraints"
    );
}

#[test]
fn pure_arrow_with_type_variable_result_does_not_absorb_region() {
    let mut d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let runtime = || Stage::Runtime(Row::none(), Row::none());
    let file = |s: &str| {
        Type::App(
            Rc::new(Type::con("T", "File")),
            Rc::new(Type::Var(Var::Named(s.into()))),
        )
    };
    let thunk = || Type::fun(unit(), runtime(), unit());
    d.values.insert(
        Path::new("T", "use_file"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(file("s"), runtime(), unit()),
        ),
    );
    let mut scope_row = Row::none();
    scope_row.elems.insert(
        Path::new("Prelude", "Scope"),
        Rc::from([Type::Var(Var::Named("s".into()))]),
    );
    d.values.insert(
        Path::new("T", "open_file"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(unit(), Stage::Runtime(scope_row, Row::none()), file("s")),
        ),
    );
    d.values
        .insert(Path::new("T", "noop"), Scheme::new(Vec::new(), thunk()));
    // `apply1_rev : forall b c. b -> (Unit -> b)^c ->{} b`
    let var_b = Type::Var(Var::Named("b".into()));
    let cap_c = Caps::var(Var::Named("c".into()));
    d.values.insert(
        Path::new("T", "apply1_rev"),
        Scheme::new(
            vec![("b".into(), Kind::Type), ("c".into(), Kind::Captures)],
            Type::fun(
                var_b.clone(),
                Stage::Pure,
                Type::fun(
                    Type::Captured(
                        Rc::new(Type::fun(unit(), Stage::Pure, var_b.clone())),
                        cap_c,
                    ),
                    runtime(),
                    var_b,
                ),
            ),
        ),
    );

    // Passing a curried lambda using `h : File s` into `apply1_rev noop` must fail:
    // `c` on `(Unit -> b)^c` must NOT dynamically move onto `b` after instantiation.
    let mut c = Checker::new(&d);
    let body = Term::lam(
        "_u",
        Term::Scope(
            "s".into(),
            Rc::new(Term::Let(
                "h".into(),
                None,
                Rc::new(Term::app(
                    Term::Global(Path::new("T", "open_file")),
                    Term::unit(),
                )),
                Rc::new(Term::app(
                    Term::app(
                        Term::Global(Path::new("T", "apply1_rev")),
                        Term::Global(Path::new("T", "noop")),
                    ),
                    Term::lam(
                        "_x",
                        Term::lam(
                            "_y",
                            Term::app(
                                Term::Global(Path::new("T", "use_file")),
                                Term::Var("h".into()),
                            ),
                        ),
                    ),
                )),
            )),
        ),
    );
    let sig = Scheme::new(Vec::new(), Type::fun(unit(), runtime(), thunk()));
    assert!(
        c.check_top(&body, &sig).is_err(),
        "apply1_rev must not let a curried closure escape scope via ?c"
    );
}

#[test]
fn dual_qualifier_curried_and_rank2_are_reflexive_and_checkable() {
    let mut d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let runtime = || Stage::Runtime(Row::none(), Row::none());
    let file = |s: &str| {
        Type::App(
            Rc::new(Type::con("T", "File")),
            Rc::new(Type::Var(Var::Named(s.into()))),
        )
    };
    d.values.insert(
        Path::new("T", "use_file"),
        Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(file("s"), runtime(), unit()),
        ),
    );

    let dual_curried = Type::Captured(
        Rc::new(Type::fun(
            unit(),
            Stage::Pure,
            Type::Captured(
                Rc::new(Type::fun(unit(), runtime(), unit())),
                Caps::atom(Var::Named("t".into())),
            ),
        )),
        Caps::atom(Var::Named("s".into())),
    );
    let dual_rank2 = Type::Captured(
        Rc::new(forall(
            &[("a", Kind::Type)],
            Type::Captured(
                Rc::new(Type::fun(
                    Type::Var(Var::Named("a".into())),
                    runtime(),
                    unit(),
                )),
                Caps::atom(Var::Named("t".into())),
            ),
        )),
        Caps::atom(Var::Named("s".into())),
    );

    for (label, ty) in [
        ("curried", dual_curried.clone()),
        ("rank2", dual_rank2.clone()),
    ] {
        let mut c = Checker::new(&d);
        let id_sig = Scheme::new(
            vec![("s".into(), Kind::Region), ("t".into(), Kind::Region)],
            Type::fun(ty.clone(), Stage::Pure, ty),
        );
        let res = c.check_top(&Term::lam("f", Term::Var("f".into())), &id_sig);
        assert!(res.is_ok(), "{label}: T <= T must hold: {res:?}");
    }

    // Checking a lambda using both `hs : File s` and `ht : File t` succeeds.
    let use_both = Term::Let(
        "_".into(),
        None,
        Rc::new(Term::app(
            Term::Global(Path::new("T", "use_file")),
            Term::Var("hs".into()),
        )),
        Rc::new(Term::app(
            Term::Global(Path::new("T", "use_file")),
            Term::Var("ht".into()),
        )),
    );
    let mut c = Checker::new(&d);
    let curried_sig = Scheme::new(
        vec![("s".into(), Kind::Region), ("t".into(), Kind::Region)],
        Type::fun(
            file("s"),
            Stage::Pure,
            Type::fun(file("t"), Stage::Pure, dual_curried),
        ),
    );
    let res = c.check_top(
        &Term::lam(
            "hs",
            Term::lam("ht", Term::lam("_x", Term::lam("_y", use_both.clone()))),
        ),
        &curried_sig,
    );
    assert!(res.is_ok(), "dual curried lambda check must pass: {res:?}");

    let mut c = Checker::new(&d);
    let rank2_sig = Scheme::new(
        vec![("s".into(), Kind::Region), ("t".into(), Kind::Region)],
        Type::fun(
            file("s"),
            Stage::Pure,
            Type::fun(file("t"), Stage::Pure, dual_rank2),
        ),
    );
    let res = c.check_top(
        &Term::lam("hs", Term::lam("ht", Term::lam("_a", use_both))),
        &rank2_sig,
    );
    assert!(res.is_ok(), "dual rank-2 lambda check must pass: {res:?}");
}

#[test]
fn bounded_all_pure_curried_function_fits_bare_curried() {
    let d = Decls::default();
    let unit = || Type::con("Prelude", "Unit");
    let pure_curried = Type::fun(unit(), Stage::Pure, Type::fun(unit(), Stage::Pure, unit()));
    let bounded_pure = Type::Bounded(
        Rc::new(pure_curried.clone()),
        Caps::atom(Var::Named("s".into())),
    );
    let mut c = Checker::new(&d);
    let pure_sig = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(bounded_pure, Stage::Pure, pure_curried),
    );
    let res = c.check_top(&Term::lam("f", Term::Var("f".into())), &pure_sig);
    assert!(
        res.is_ok(),
        "bounded all-pure curried function must fit bare: {res:?}"
    );
}
