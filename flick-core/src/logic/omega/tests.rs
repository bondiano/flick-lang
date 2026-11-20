use super::{BUDGET, Linear, Outcome, Proof, Step, check, decide, decide_budgeted};

fn lin(terms: &[(usize, i128)], c: i128) -> Linear {
    Linear::new(terms.iter().copied(), c).expect("small")
}

/// `lo <= x_i <= hi` for each of `vars` variables.
fn boxed(vars: usize, lo: i128, hi: i128) -> Vec<Linear> {
    (0..vars)
        .flat_map(|x| [lin(&[(x, 1)], -lo), lin(&[(x, -1)], hi)])
        .collect()
}

fn points(vars: usize, lo: i128, hi: i128) -> Vec<Vec<i128>> {
    let mut out = vec![Vec::new()];
    for _ in 0..vars {
        out = out
            .into_iter()
            .flat_map(|p| (lo..=hi).map(move |v| [p.clone(), vec![v]].concat()))
            .collect();
    }
    out
}

/// Refuted exactly when no point of the box satisfies the facts, every
/// refutation a valid certificate, every model a point of the facts.
fn agrees(facts: &[Linear], vars: usize) {
    agrees_in(facts, vars, -4, 4);
}

fn agrees_in(facts: &[Linear], vars: usize, lo: i128, hi: i128) {
    let mut all = boxed(vars, lo, hi);
    all.extend_from_slice(facts);
    let any = points(vars, lo, hi)
        .into_iter()
        .any(|p| all.iter().all(|f| f.eval(&p).is_some_and(|v| v >= 0)));
    match decide(&all, vars, BUDGET) {
        Outcome::Refuted(proof) => {
            assert!(!any, "refuted a satisfiable system {all:?}");
            assert!(
                check(&all, vars, &proof),
                "an invalid certificate for {all:?}: {proof:?}"
            );
        }
        Outcome::Model(m) => {
            assert!(any, "a model of an empty system");
            assert!(
                all.iter().all(|f| f.eval(&m).is_some_and(|v| v >= 0)),
                "{m:?} is no point of {all:?}"
            );
        }
        Outcome::Unknown => panic!("undecided: {all:?}"),
    }
}

#[test]
fn a_contradiction_of_two_bounds_is_refuted() {
    // x >= 1 and x <= 0
    let facts = [lin(&[(0, 1)], -1), lin(&[(0, -1)], 0)];
    let Outcome::Refuted(proof) = decide(&facts, 1, 1000) else {
        panic!("not refuted")
    };
    assert!(check(&facts, 1, &proof));
}

#[test]
fn an_integer_gap_needs_tightening() {
    // 2x >= 1 and 2x <= 1: a rational point, no integer one
    let facts = [lin(&[(0, 2)], -1), lin(&[(0, -2)], 1)];
    let Outcome::Refuted(proof) = decide(&facts, 1, 1000) else {
        panic!("not refuted")
    };
    assert!(check(&facts, 1, &proof));
}

#[test]
fn an_equality_without_a_unit_coefficient_has_a_fresh_variable() {
    // 3x + 5y == 1 has integer points; with 0 <= x, y <= 1 it has none
    let facts = [
        lin(&[(0, 3), (1, 5)], -1),
        lin(&[(0, -3), (1, -5)], 1),
        lin(&[(0, 1)], 0),
        lin(&[(0, -1)], 1),
        lin(&[(1, 1)], 0),
        lin(&[(1, -1)], 1),
    ];
    let Outcome::Refuted(proof) = decide(&facts, 2, 10_000) else {
        panic!("not refuted")
    };
    assert!(check(&facts, 2, &proof));
    // without the box, a point: x = 2, y = -1
    let Outcome::Model(m) = decide(&facts[..2], 2, 10_000) else {
        panic!("no model")
    };
    assert_eq!(3 * m[0] + 5 * m[1], 1);
}

#[test]
fn the_dark_shadow_and_its_splinters_decide_an_inexact_elimination() {
    // 2 <= 3x - 2y <= 3 and 1 <= 2y - x ... small systems with non-unit coefficients
    for c in -3..=3 {
        agrees(
            &[
                lin(&[(0, 3), (1, -2)], -2),
                lin(&[(0, -3), (1, 2)], 3),
                lin(&[(0, -1), (1, 2)], c),
            ],
            2,
        );
        agrees(
            &[
                lin(&[(0, 2), (1, 3)], c),
                lin(&[(0, -3), (1, -2)], 1),
                lin(&[(0, 5), (1, -3)], -c),
            ],
            2,
        );
    }
}

#[test]
fn the_omega_test_agrees_with_every_point_of_small_boxes() {
    // a deterministic walk over small systems of three variables
    let mut seed: u64 = 0x5eed_cafe;
    let mut next = |n: i128| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let span = u64::try_from(2 * n + 1).expect("a small span");
        i128::from(seed % span) - n
    };
    for _ in 0..300 {
        let facts: Vec<Linear> = (0..4)
            .map(|_| lin(&[(0, next(3)), (1, next(3)), (2, next(3))], next(6)))
            .collect();
        agrees(&facts, 3);
    }
    // systems of four variables a search with a smaller budget left open
    let hard = [
        [
            (-1, 4, -3, 4, 7),
            (4, -1, 2, -5, 1),
            (-1, -5, 1, 4, -5),
            (-4, 3, -4, -2, 1),
            (-5, 3, -1, 3, 9),
        ],
        [
            (5, -1, 3, -5, -2),
            (-4, 3, 3, 3, 4),
            (-1, -3, -2, 3, 7),
            (2, -1, -1, -4, 7),
            (3, 4, -1, 3, 8),
        ],
        [
            (3, 5, -4, -3, -2),
            (2, -3, 0, 3, 3),
            (-4, 4, 5, -5, -9),
            (2, 5, 5, -4, 0),
            (-2, 5, -3, -1, -1),
        ],
    ];
    for system in hard {
        let facts: Vec<Linear> = (system.iter())
            .map(|&(a, b, c, d, k)| lin(&[(0, a), (1, b), (2, c), (3, d)], k))
            .collect();
        agrees_in(&facts, 4, -3, 3);
    }
}

#[test]
fn a_forged_certificate_is_refused() {
    let facts = [lin(&[(0, 2)], -1), lin(&[(0, -2)], 1)];
    let Outcome::Refuted(proof) = decide(&facts, 1, 1000) else {
        panic!("not refuted")
    };
    // the same proof of facts a point satisfies
    let sat = [lin(&[(0, 2)], 0), lin(&[(0, -2)], 0)];
    assert!(!check(&sat, 1, &proof));
    // a combination with a coefficient that is not positive
    let bad = Proof::Derive(
        vec![Step::Comb(vec![(0, 1), (1, -1)])],
        Box::new(Proof::Absurd(2)),
    );
    assert!(!check(&facts, 1, &bad));
    // a constant that is not negative
    assert!(!check(&[lin(&[], 0)], 0, &Proof::Absurd(0)));
    // tightening by a divisor of not every coefficient
    let tight = Proof::Derive(vec![Step::Tighten(0, 2)], Box::new(Proof::Absurd(1)));
    assert!(!check(&[lin(&[(0, 3)], -1)], 1, &tight));
    // a fresh variable whose form is no multiple apart from the equality
    let fresh = Proof::Derive(
        vec![Step::Fresh {
            eq: 0,
            neg: 1,
            m: 3,
            form: lin(&[(0, 1)], 0),
        }],
        Box::new(Proof::Absurd(2)),
    );
    assert!(!check(&[lin(&[(0, 2)], 0), lin(&[(0, -2)], 0)], 1, &fresh));
}

#[test]
fn decide_budgeted_shares_and_depletes_one_budget() {
    let facts = [lin(&[(0, 1)], -1), lin(&[(0, -1)], 0)];
    let mut budget = 100;
    let Outcome::Refuted(proof) = decide_budgeted(&facts, 1, &mut budget) else {
        panic!("expected refutation");
    };
    assert!(check(&facts, 1, &proof));
    assert!(budget < 100, "budget should be spent: remaining {budget}");

    let mut zero = 0;
    assert!(matches!(
        decide_budgeted(&facts, 1, &mut zero),
        Outcome::Unknown
    ));
    assert_eq!(zero, 0);
}
