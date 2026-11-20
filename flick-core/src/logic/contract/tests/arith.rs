use super::super::{
    ArithOp, ContractCert, ContractFailure, ContractSpec, INT_MAX, INT_MIN, IntCmpOp, LeafCert,
    PathCert, RangeCert, VerifiedContracts, check_contract_cert, prove_contract,
};
use super::{
    FakeProgram, ce_int, consume_spec, lang, span, span_at, tadd, tcmp, tint, tlam, tneg, tsub,
    tvar,
};
use crate::diagnostics::codes;
use crate::logic::{DOMAIN_V2, Data, Ty, omega};

#[test]
fn prove_consume_certified_and_rejects_tampered_certificates() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let spec = consume_spec(span_at(1, 2), span_at(3, 4), span_at(0, 9));
    let body = tsub(tvar("available"), tvar("used"), span_at(5, 6));

    let cert = prove_contract(&spec, &body, &data, &prog, &verified, &l)
        .expect("consume should be certified");
    assert_eq!(cert.domain, DOMAIN_V2);
    assert!(matches!(cert.root, PathCert::Range { .. }));
    assert!(check_contract_cert(&spec, &body, &cert, &data, &prog, &verified, &l).is_ok());

    let assert_t083 = |tampered: ContractCert, label: &str| {
        let err = check_contract_cert(&spec, &body, &tampered, &data, &prog, &verified, &l)
            .expect_err(label);
        assert_eq!(
            err.diagnostic("consume").code,
            codes::T083,
            "{label}: {err:?}"
        );
    };

    // 1. Wrong domain
    let mut wrong_domain = cert.clone();
    wrong_domain.domain = "PureTotalV1";
    assert_t083(wrong_domain, "wrong domain");

    // 2. Forged Trivial on Range lower bound
    if let PathCert::Range { cert: rc, next } = &cert.root {
        assert_t083(
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Range {
                    cert: RangeCert {
                        lower: LeafCert::Trivial,
                        upper: rc.upper.clone(),
                    },
                    next: next.clone(),
                },
            },
            "forged trivial lower range bound",
        );

        // 3. Forged Trivial on Range upper bound
        assert_t083(
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Range {
                    cert: RangeCert {
                        lower: rc.lower.clone(),
                        upper: LeafCert::Trivial,
                    },
                    next: next.clone(),
                },
            },
            "forged trivial upper range bound",
        );

        // 4. Bogus Omega proof on Range upper bound
        assert_t083(
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Range {
                    cert: RangeCert {
                        lower: rc.lower.clone(),
                        upper: LeafCert::Omega(omega::Proof::Absurd(0)),
                    },
                    next: next.clone(),
                },
            },
            "corrupted omega proof on range",
        );

        // 5. Skipped Range node completely
        assert_t083(
            ContractCert {
                domain: DOMAIN_V2,
                root: (**next).clone(),
            },
            "skipped Range node",
        );

        // 6. Corrupted postcondition inside next
        assert_t083(
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Range {
                    cert: rc.clone(),
                    next: Box::new(PathCert::Leaf(LeafCert::Trivial)),
                },
            },
            "forged Leaf(Trivial) for postcondition",
        );
    }

    // 7. Claiming live path is Dead(Trivial) or Dead(bad Omega)
    assert_t083(
        ContractCert {
            domain: DOMAIN_V2,
            root: PathCert::Dead(LeafCert::Trivial),
        },
        "forged Dead(Trivial)",
    );
    assert_t083(
        ContractCert {
            domain: DOMAIN_V2,
            root: PathCert::Dead(LeafCert::Omega(omega::Proof::Absurd(0))),
        },
        "forged Dead(Omega)",
    );
}

#[test]
fn prove_consume_buggy_returns_available_fails_t087_with_verified_counterexample() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let ens_span = span_at(3, 7);
    let decl_span = span_at(0, 9);
    let spec = consume_spec(span_at(1, 2), ens_span, decl_span);
    let buggy_body = tvar("available");

    let err = prove_contract(&spec, &buggy_body, &data, &prog, &verified, &l)
        .expect_err("buggy consume must fail");
    let ContractFailure::Postcondition {
        span: got_span,
        decl_span: got_decl,
        counterexample: Some(ref ce),
    } = err
    else {
        panic!("expected Postcondition with counterexample, got: {err:?}");
    };
    assert_eq!(got_span, ens_span);
    assert_eq!(got_decl, decl_span);
    assert_eq!(err.diagnostic("consume").code, codes::T087);
    assert_eq!(ce.bindings[0].0, "available");
    assert_eq!(ce.bindings[1].0, "used");

    let avail = ce_int(ce, "available");
    let used = ce_int(ce, "used");
    assert!((INT_MIN..=INT_MAX).contains(&avail));
    assert!((INT_MIN..=INT_MAX).contains(&used));
    assert!(0 <= used && used <= avail, "must satisfy require: {ce:?}");
    assert!(
        !(0 <= avail && avail + used == avail),
        "must violate ensure: {ce:?}"
    );
}

#[test]
fn prove_int_overflow_add_sub_and_guarded_variants() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let add_span = span_at(4, 7);

    // Unguarded `x + 1 > x` -> T089 at `+` with x = 2147483647
    let next_spec = ContractSpec {
        module: "App".into(),
        name: "next".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("y", tcmp(IntCmpOp::Gt, tvar("y"), tvar("x"))),
            span_at(2, 3),
        )],
    };
    let next_body = tadd(tvar("x"), tint(1), add_span);
    let err = prove_contract(&next_spec, &next_body, &data, &prog, &verified, &l)
        .expect_err("unguarded x + 1 must fail with overflow");
    let ContractFailure::Overflow {
        span: got_span,
        op,
        counterexample: Some(ref ce),
    } = err
    else {
        panic!("expected Overflow, got: {err:?}");
    };
    assert_eq!(got_span, add_span);
    assert_eq!(op, ArithOp::Add);
    assert_eq!(err.diagnostic("next").code, codes::T089);
    assert_eq!(ce_int(ce, "x"), INT_MAX);

    // Guarded `require (x < 2147483647)` -> certified
    let mut guarded_next = next_spec;
    guarded_next
        .requires
        .push((tcmp(IntCmpOp::Lt, tvar("x"), tint(INT_MAX)), span_at(1, 2)));
    prove_contract(&guarded_next, &next_body, &data, &prog, &verified, &l)
        .expect("guarded x + 1 should be certified");

    // Unguarded `x - 1 < x` -> T089 at `-` with x = -2147483648
    let sub_span = span_at(5, 8);
    let mut prev_spec = ContractSpec {
        module: "App".into(),
        name: "prev".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("y", tcmp(IntCmpOp::Lt, tvar("y"), tvar("x"))),
            span_at(2, 3),
        )],
    };
    let prev_body = tsub(tvar("x"), tint(1), sub_span);
    let err = prove_contract(&prev_spec, &prev_body, &data, &prog, &verified, &l)
        .expect_err("unguarded x - 1 must overflow");
    assert!(matches!(
        err,
        ContractFailure::Overflow {
            span,
            op: ArithOp::Sub,
            counterexample: Some(ref ce),
        } if span == sub_span && ce_int(ce, "x") == INT_MIN
    ));
    prev_spec
        .requires
        .push((tcmp(IntCmpOp::Gt, tvar("x"), tint(INT_MIN)), span_at(1, 2)));
    prove_contract(&prev_spec, &prev_body, &data, &prog, &verified, &l)
        .expect("guarded x - 1 should be certified");
}

#[test]
fn prove_const_int_arithmetic_in_range_and_overflow() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let const_spec = ContractSpec {
        module: "App".into(),
        name: "three".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("y", tcmp(IntCmpOp::Eq, tvar("y"), tint(3))),
            span_at(1, 2),
        )],
    };
    prove_contract(
        &const_spec,
        &tadd(tint(1), tint(2), span_at(3, 4)),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect("1 + 2 fits in Int");
    let const_ovf = prove_contract(
        &const_spec,
        &tadd(tint(INT_MAX), tint(1), span_at(3, 4)),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect_err("INT_MAX + 1 overflows");
    assert_eq!(const_ovf.diagnostic("three").code, codes::T089);
}

#[test]
fn prove_unary_neg_overflow_and_guarded_success() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let neg_span = span_at(6, 8);

    let mut neg_spec = ContractSpec {
        module: "App".into(),
        name: "negate".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam(
                "y",
                tcmp(IntCmpOp::Eq, tadd(tvar("y"), tvar("x"), span()), tint(0)),
            ),
            span_at(2, 4),
        )],
    };
    let neg_body = tneg(tvar("x"), neg_span);

    let err = prove_contract(&neg_spec, &neg_body, &data, &prog, &verified, &l)
        .expect_err("unguarded -x must fail at i32::MIN");
    let ContractFailure::Overflow {
        span: got_span,
        op,
        counterexample: Some(ref ce),
    } = err
    else {
        panic!("expected Overflow, got: {err:?}");
    };
    assert_eq!(got_span, neg_span);
    assert_eq!(op, ArithOp::Neg);
    assert_eq!(err.diagnostic("negate").code, codes::T089);
    assert_eq!(ce_int(ce, "x"), INT_MIN);

    neg_spec
        .requires
        .push((tcmp(IntCmpOp::Gt, tvar("x"), tint(INT_MIN)), span_at(1, 2)));
    prove_contract(&neg_spec, &neg_body, &data, &prog, &verified, &l)
        .expect("guarded -x should be certified");
}
