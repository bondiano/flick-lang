use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::sub::scope_service;
use super::tests::{
    add_instance, base_decls, cap_fn, data, dict, fn_pure, fn_rt, fn_rt_empty, p, ty, var,
};
use super::*;
use crate::check::Checker;
use crate::types::{Kind, Row, Scheme, Stage, Type};
use crate::variance::Sign;

#[test]
fn k1_prime_rejects_comparable_and_product_overlaps_in_both_orders() {
    let cases: &[(&str, Constraint, Constraint)] = &[
        // Comparable `-` heads: `fnP <= fnR`
        (
            "neg comparable",
            Constraint::new(p("Show"), vec![fn_pure()]),
            Constraint::new(p("Show"), vec![fn_rt("Async")]),
        ),
        // Comparable `+` heads: `Box fnP <= Box fnR`
        (
            "pos comparable",
            Constraint::new(p("Default"), vec![data("Box", vec![fn_pure()])]),
            Constraint::new(p("Default"), vec![data("Box", vec![fn_rt("Async")])]),
        ),
        // Mutually incomparable product heads sharing `Pair fnP fnP` under `-`
        (
            "neg incomparable product",
            Constraint::new(p("Show"), vec![data("Pair", vec![fn_rt("A"), fn_pure()])]),
            Constraint::new(p("Show"), vec![data("Pair", vec![fn_pure(), fn_rt("B")])]),
        ),
        // Mutually incomparable product heads sharing `Pair fnR(A+B) fnR(A+B)` under `+`
        (
            "pos incomparable product",
            Constraint::new(
                p("Default"),
                vec![data("Pair", vec![fn_rt("A"), fn_pure()])],
            ),
            Constraint::new(
                p("Default"),
                vec![data("Pair", vec![fn_pure(), fn_rt("B")])],
            ),
        ),
        // Distinct runtime stages sharing `fnP` under `-`
        (
            "neg distinct runtime stages",
            Constraint::new(p("Show"), vec![fn_rt("A")]),
            Constraint::new(p("Show"), vec![fn_rt("B")]),
        ),
        // Distinct runtime stages sharing `fnR(A+B)` under `+`
        (
            "pos distinct runtime stages",
            Constraint::new(p("Default"), vec![fn_rt("A")]),
            Constraint::new(p("Default"), vec![fn_rt("B")]),
        ),
        // Contravariant data flips `-` to `+`: `Sink fnP` and `Sink fnR` share `Sink fnR`
        (
            "contravariant data under neg",
            Constraint::new(p("Show"), vec![data("Sink", vec![fn_pure()])]),
            Constraint::new(p("Show"), vec![data("Sink", vec![fn_rt("A")])]),
        ),
    ];

    for (label, h1, h2) in cases {
        // Check BOTH declaration orders `(I, J)` and `(J, I)`!
        for (first_name, first, second_name, second) in
            [("inst_a", h1, "inst_b", h2), ("inst_a", h2, "inst_b", h1)]
        {
            let mut d = base_decls();
            add_instance(&mut d, first_name, &[], vec![], first.clone());
            add_instance(&mut d, second_name, &[], vec![], second.clone());
            d.finish();
            let err = check_coherence(&d).expect_err(&format!("{label} must overlap"));
            assert_eq!(err.len(), 1, "{label}");
            assert_eq!(err[0].class, first.class, "{label}");
        }
    }
}

#[test]
fn k1_prime_accepts_disjoint_inv_bi_invariant_data_and_nonlinear_vars() {
    struct NonOverlapCase {
        label: &'static str,
        v1: Vec<(&'static str, Kind)>,
        h1: Constraint,
        v2: Vec<(&'static str, Kind)>,
        h2: Constraint,
    }
    let non_overlapping = [
        NonOverlapCase {
            label: "inv distinct stages",
            v1: vec![],
            h1: Constraint::new(p("Monoid"), vec![fn_pure()]),
            v2: vec![],
            h2: Constraint::new(p("Monoid"), vec![fn_rt("A")]),
        },
        NonOverlapCase {
            label: "bi distinct stages",
            v1: vec![],
            h1: Constraint::new(p("TypeName"), vec![fn_pure()]),
            v2: vec![],
            h2: Constraint::new(p("TypeName"), vec![fn_rt("A")]),
        },
        NonOverlapCase {
            label: "invariant data wrapper under neg",
            v1: vec![],
            h1: Constraint::new(p("Show"), vec![data("Cell", vec![fn_pure()])]),
            v2: vec![],
            h2: Constraint::new(p("Show"), vec![data("Cell", vec![fn_rt("A")])]),
        },
        NonOverlapCase {
            label: "non-linear head var disjoint scalars",
            v1: vec![("a", Kind::Type)],
            h1: Constraint::new(p("Show"), vec![data("Pair", vec![var("a"), var("a")])]),
            v2: vec![],
            h2: Constraint::new(p("Show"), vec![data("Pair", vec![ty("Int"), ty("Bool")])]),
        },
    ];

    for case in non_overlapping {
        let mut d = base_decls();
        add_instance(&mut d, "inst_1", &case.v1, vec![], case.h1);
        add_instance(&mut d, "inst_2", &case.v2, vec![], case.h2);
        d.finish();
        assert!(
            check_coherence(&d).is_ok(),
            "{} should not overlap",
            case.label
        );
    }

    // Contrast: non-linear `Show (Pair a a)` DOES overlap `Show (Pair fnP fnR)`!
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_diag",
        &[("a", Kind::Type)],
        vec![],
        Constraint::new(p("Show"), vec![data("Pair", vec![var("a"), var("a")])]),
    );
    add_instance(
        &mut d,
        "inst_mix",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![data("Pair", vec![fn_pure(), fn_rt("A")])]),
    );
    d.finish();
    assert!(
        check_coherence(&d).is_err(),
        "Show (Pair a a) must overlap Show (Pair fnP fnR)"
    );
}

#[test]
fn safe_use_preserved_by_elaborated_dictionaries_and_core_checker() {
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_show_rt",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![fn_rt_empty()]),
    );
    add_instance(
        &mut d,
        "inst_default_box_pure",
        &[],
        vec![],
        Constraint::new(p("Default"), vec![data("Box", vec![fn_pure()])]),
    );
    // A hypothetical dictionary carrying a rigid region `s` in its head.
    add_instance(
        &mut d,
        "inst_show_scoped",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![cap_fn(fn_pure(), "s")]),
    );
    d.finish();

    // 1. Reach-dropping `-` dictionary is rejected by `serves` and `entail`
    //    (`Show (fnP^s)` cannot serve `Show fnP` because `{s} ⊈ ∅`).
    let q_pure = Constraint::new(p("Show"), vec![fn_pure()]);
    assert!(!serves(
        &d,
        &q_pure,
        &Constraint::new(p("Show"), vec![cap_fn(fn_pure(), "s")])
    ));

    // 2. Resolved `-` dictionary (`inst_show_rt : Show.Dict fnR`) type-checks at
    //    `Show.Dict fnP` in the core `Checker` (`LeLive.dict`).
    let resolved_show = entail(&d, &[], &q_pure).expect("resolves to inst_show_rt");
    let show_scheme = Scheme::new(vec![], q_pure.to_dict_type());
    assert!(
        Checker::new(&d)
            .check_top(&resolved_show, &show_scheme)
            .is_ok(),
        "resolved - dictionary must pass core Checker"
    );

    // 3. Resolved `+` dictionary (`inst_default_box_pure : Default.Dict (Box fnP)`)
    //    type-checks at `Default.Dict (Box (fnP^s))` inside `scope s` (`SafeUse`).
    let q_def_rt = Constraint::new(p("Default"), vec![data("Box", vec![fn_rt_empty()])]);
    let resolved_def = entail(&d, &[], &q_def_rt).expect("resolves to inst_default_box_pure");
    let def_scheme = Scheme::new(vec![], q_def_rt.to_dict_type());
    assert!(
        Checker::new(&d)
            .check_top(&resolved_def, &def_scheme)
            .is_ok(),
        "resolved + dictionary must pass core Checker"
    );

    let scoped_pos = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(
            dict("Default", vec![data("Box", vec![fn_pure()])]),
            Stage::Pure,
            dict("Default", vec![data("Box", vec![cap_fn(fn_pure(), "s")])]),
        ),
    );
    assert!(
        Checker::new(&d)
            .check_top(&Term::lam("x", Term::Var("x".into())), &scoped_pos)
            .is_ok(),
        "widening + dictionary with region reach must pass core Checker"
    );

    // 4. Core `Checker` ALSO rejects dropping reach on a `-` dictionary:
    //    `Show.Dict (fnP^s) -> Show.Dict fnP` violates `SafeUse` (`≤-Dict`).
    let leak_neg = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(
            dict("Show", vec![cap_fn(fn_pure(), "s")]),
            Stage::Pure,
            dict("Show", vec![fn_pure()]),
        ),
    );
    assert!(
        Checker::new(&d)
            .check_top(&Term::lam("x", Term::Var("x".into())), &leak_neg)
            .is_err(),
        "dropping region reach in - dictionary must be rejected by core SafeUse"
    );
}

#[test]
fn entail_is_order_independent_and_reports_ambiguous_when_k1_prime_fails() {
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_show_a",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![fn_rt("A")]),
    );
    add_instance(
        &mut d,
        "inst_show_b",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![fn_rt("B")]),
    );
    d.finish();
    assert!(check_coherence(&d).is_err());
    let err = entail(&d, &[], &Constraint::new(p("Show"), vec![fn_pure()]))
        .expect_err("overlapping instances must be ambiguous, not order-dependent");
    assert!(matches!(err, ElabError::Ambiguous(_, ref names) if names.len() == 2));
}

#[test]
fn phantom_bivariant_data_params_require_exact_equality_in_b1_and_k1_prime() {
    // `Phantom` has a bivariant (`Sign::Bi`) unused parameter.
    // Subtyping (`subsume_data`) keeps nominal `Bi` data arguments exact,
    // so B1 resolution and K1' overlap must agree:
    //  1) `Default (Phantom Int)` must NOT serve `Default (Phantom String)`.
    //  2) `Show (Phantom Int)` and `Show (Phantom String)` are disjoint
    //     (no K1' overlap) and resolve to their own dictionaries.
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_default_phantom_int",
        &[],
        vec![],
        Constraint::new(p("Default"), vec![data("Phantom", vec![ty("Int")])]),
    );
    add_instance(
        &mut d,
        "inst_show_phantom_int",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![data("Phantom", vec![ty("Int")])]),
    );
    add_instance(
        &mut d,
        "inst_show_phantom_str",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![data("Phantom", vec![ty("String")])]),
    );
    d.finish();

    assert_eq!(d.signs.get(&p("Phantom")), Some(&vec![Sign::Bi]));
    assert!(
        check_coherence(&d).is_ok(),
        "disjoint phantom-tag instances must not overlap under K1'"
    );

    let def_int = Constraint::new(p("Default"), vec![data("Phantom", vec![ty("Int")])]);
    let def_str = Constraint::new(p("Default"), vec![data("Phantom", vec![ty("String")])]);
    assert!(!serves(&d, &def_str, &def_int));
    assert!(
        entail(&d, &[], &def_str).is_err(),
        "Default (Phantom Int) must not serve Default (Phantom String)"
    );
    assert!(entail(&d, &[], &def_int).is_ok());

    let show_int = Constraint::new(p("Show"), vec![data("Phantom", vec![ty("Int")])]);
    let show_str = Constraint::new(p("Show"), vec![data("Phantom", vec![ty("String")])]);
    let dict_int = entail(&d, &[], &show_int).expect("Show (Phantom Int) resolves");
    let dict_str = entail(&d, &[], &show_str).expect("Show (Phantom String) resolves");
    assert!(matches!(dict_int, Term::Global(ref g) if *g == p("inst_show_phantom_int")));
    assert!(matches!(dict_str, Term::Global(ref g) if *g == p("inst_show_phantom_str")));
    assert!(
        Checker::new(&d)
            .check_top(&dict_int, &Scheme::new(vec![], show_int.to_dict_type()))
            .is_ok()
    );
    assert!(
        Checker::new(&d)
            .check_top(&dict_str, &Scheme::new(vec![], show_str.to_dict_type()))
            .is_ok()
    );
}

#[test]
fn k1_prime_detects_overlap_when_contravariant_sibling_supplies_reach() {
    // Counterexample for descending from `CommonLowerReach` into covariant
    // sub-positions:
    //   f1 = (Unit -> Unit) -> (Unit ->{Scope s} Unit)
    //   f2 = (Unit ->{Scope s} Unit) -> (Unit -> Unit)
    // Both have outer reach `{s}` and `f2 <= f1`, so BOTH serve `Q = f2`
    // under `-` (`Show`). K1' must report overlap in BOTH declaration orders.
    let mut scope_elems = BTreeMap::new();
    scope_elems.insert(scope_service(), Rc::from(vec![var("s")]));
    let fn_scope_s = Type::fun(
        ty("Unit"),
        Stage::Runtime(
            Row {
                elems: scope_elems,
                vars: BTreeSet::new(),
                less: BTreeSet::new(),
            },
            Row::none(),
        ),
        ty("Unit"),
    );
    let f1 = Type::fun(fn_pure(), Stage::Pure, fn_scope_s.clone());
    let f2 = Type::fun(fn_scope_s, Stage::Pure, fn_pure());
    let h1 = Constraint::new(p("Show"), vec![f1]);
    let h2 = Constraint::new(p("Show"), vec![f2]);

    for (first, second) in [(&h1, &h2), (&h2, &h1)] {
        let mut d = base_decls();
        add_instance(&mut d, "inst_a", &[], vec![], first.clone());
        add_instance(&mut d, "inst_b", &[], vec![], second.clone());
        d.finish();

        // Both instances serve `Q = h2` under `Cn.Serves`.
        assert!(serves(&d, &h2, &h1), "f1 must serve Q = f2");
        assert!(serves(&d, &h2, &h2), "f2 must serve Q = f2");
        let err = check_coherence(&d).expect_err(
            "K1' must detect overlap when a contravariant sibling supplies outer reach",
        );
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].class, p("Show"));
    }

    // Outer reach mismatch still stays disjoint when nothing supplies `{s}`.
    let mut d = base_decls();
    add_instance(
        &mut d,
        "inst_pure",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![fn_pure()]),
    );
    add_instance(
        &mut d,
        "inst_scoped",
        &[],
        vec![],
        Constraint::new(p("Show"), vec![cap_fn(fn_pure(), "s")]),
    );
    d.finish();
    assert!(
        check_coherence(&d).is_ok(),
        "outer reach mismatch {{}} vs {{s}} under `-` must stay disjoint"
    );
}
