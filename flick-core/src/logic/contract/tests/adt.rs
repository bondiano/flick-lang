use std::rc::Rc;

use super::super::{
    Arm, ContractCert, ContractFailure, ContractSpec, INT_MAX, IntCmpOp, Pat, PathCert, Term,
    TermKind, VerifiedContracts, check_contract_cert, prove_contract,
};
use super::{FakeProgram, lang, span_at, t, tadd, tcmp, tif, tint, tlam, tvar};
use crate::diagnostics::codes;
use crate::logic::{Ctor, DOMAIN_V2, Data, DataDecl, Ty};

#[test]
fn prove_adt_match_and_rejects_match_data_tampering() {
    let prog = FakeProgram::default();
    let l = lang();
    let verified = VerifiedContracts::new();

    let mut data = Data::new();
    data.insert(
        ("App".into(), "Light".into()),
        DataDecl {
            params: vec![],
            constructors: vec![("Red".into(), vec![]), ("Green".into(), vec![])],
            recursive: false,
        },
    );

    let light_ty = Ty::Data {
        module: "App".into(),
        name: "Light".into(),
        args: vec![],
    };
    let red = Rc::new(Ctor {
        module: "App".into(),
        ty: "Light".into(),
        name: "Red".into(),
        arity: 0,
    });
    let green = Rc::new(Ctor {
        module: "App".into(),
        ty: "Light".into(),
        name: "Green".into(),
        arity: 0,
    });

    let next_light = ContractSpec {
        module: "App".into(),
        name: "next_light".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("light".into(), light_ty.clone())],
        result_ty: light_ty.clone(),
        requires: vec![],
        ensures: vec![(
            tlam(
                "l",
                t(TermKind::TagEq {
                    eq: false,
                    lhs: Rc::new(tvar("l")),
                    rhs: Rc::new(tvar("light")),
                }),
            ),
            span_at(1, 2),
        )],
    };
    let next_light_body = t(TermKind::Match(
        Rc::new(tvar("light")),
        vec![
            Arm {
                pat: Pat::Con(red.clone(), vec![]),
                body: t(TermKind::Con(green.clone())),
            },
            Arm {
                pat: Pat::Con(green.clone(), vec![]),
                body: t(TermKind::Con(red.clone())),
            },
        ]
        .into(),
    ));
    let cert = prove_contract(&next_light, &next_light_body, &data, &prog, &verified, &l)
        .expect("next_light certified");

    // Tamper MatchData: missing, reordered, duplicate constructors -> T083
    let PathCert::MatchData { branches } = &cert.root else {
        panic!("expected MatchData cert");
    };
    assert_eq!(branches.len(), 2);
    for (label, bad_branches) in [
        ("missing ctor", vec![branches[0].clone()]),
        (
            "reordered ctors",
            vec![branches[1].clone(), branches[0].clone()],
        ),
        (
            "duplicate ctor",
            vec![branches[0].clone(), branches[0].clone()],
        ),
    ] {
        let bad = ContractCert {
            domain: DOMAIN_V2,
            root: PathCert::MatchData {
                branches: bad_branches,
            },
        };
        let err = check_contract_cert(
            &next_light,
            &next_light_body,
            &bad,
            &data,
            &prog,
            &verified,
            &l,
        )
        .expect_err(label);
        assert_eq!(err.diagnostic("next_light").code, codes::T083, "{label}");
    }
}

#[test]
fn prove_adt_tageq_splits_two_unsplit_params() {
    let prog = FakeProgram::default();
    let l = lang();
    let verified = VerifiedContracts::new();
    let mut data = Data::new();
    data.insert(
        ("App".into(), "Light".into()),
        DataDecl {
            params: vec![],
            constructors: vec![("Red".into(), vec![]), ("Green".into(), vec![])],
            recursive: false,
        },
    );
    let light_ty = Ty::Data {
        module: "App".into(),
        name: "Light".into(),
        args: vec![],
    };
    let same_light = ContractSpec {
        module: "App".into(),
        name: "same_light".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("a".into(), light_ty.clone()), ("b".into(), light_ty)],
        result_ty: Ty::Bool,
        requires: vec![],
        ensures: vec![(
            tlam(
                "r",
                t(TermKind::BoolEq {
                    eq: true,
                    lhs: Rc::new(tvar("r")),
                    rhs: Rc::new(t(TermKind::TagEq {
                        eq: true,
                        lhs: Rc::new(tvar("a")),
                        rhs: Rc::new(tvar("b")),
                    })),
                }),
            ),
            span_at(1, 2),
        )],
    };
    let same_light_body = t(TermKind::TagEq {
        eq: true,
        lhs: Rc::new(tvar("a")),
        rhs: Rc::new(tvar("b")),
    });
    prove_contract(&same_light, &same_light_body, &data, &prog, &verified, &l)
        .expect("TagEq on two unsplit Lights certified");
}

#[test]
fn prove_option_int_field_enforces_32bit_bounds() {
    let prog = FakeProgram::default();
    let l = lang();
    let verified = VerifiedContracts::new();
    let mut data = Data::new();
    data.insert(
        ("Prelude".into(), "Option".into()),
        DataDecl {
            params: vec!["a".into()],
            constructors: vec![
                ("None".into(), vec![]),
                ("Some".into(), vec![Ty::Rigid("a".into())]),
            ],
            recursive: false,
        },
    );
    let none_c = Rc::new(Ctor {
        module: "Prelude".into(),
        ty: "Option".into(),
        name: "None".into(),
        arity: 0,
    });
    let some_c = Rc::new(Ctor {
        module: "Prelude".into(),
        ty: "Option".into(),
        name: "Some".into(),
        arity: 1,
    });
    let opt_int = Ty::Data {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![Ty::Int],
    };
    let inc_opt_spec = ContractSpec {
        module: "App".into(),
        name: "inc_opt".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("opt".into(), opt_int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![],
    };
    let unguarded_opt = t(TermKind::Match(
        Rc::new(tvar("opt")),
        vec![
            Arm {
                pat: Pat::Con(none_c.clone(), vec![]),
                body: tint(0),
            },
            Arm {
                pat: Pat::Con(some_c.clone(), vec![Pat::Bind("n".into())]),
                body: tadd(tvar("n"), tint(1), span_at(4, 5)),
            },
        ]
        .into(),
    ));
    let err = prove_contract(&inc_opt_spec, &unguarded_opt, &data, &prog, &verified, &l)
        .expect_err("Option Int field overflow must fail T089");
    let ContractFailure::Overflow {
        counterexample: Some(ref ce),
        ..
    } = err
    else {
        panic!("expected Overflow, got {err:?}");
    };
    assert_eq!(err.diagnostic("inc_opt").code, codes::T089);
    assert_eq!(ce.bindings, vec![("opt".into(), "Some 2147483647".into())]);

    let guarded_opt = t(TermKind::Match(
        Rc::new(tvar("opt")),
        vec![
            Arm {
                pat: Pat::Con(none_c, vec![]),
                body: tint(0),
            },
            Arm {
                pat: Pat::Con(some_c, vec![Pat::Bind("n".into())]),
                body: tif(
                    tcmp(IntCmpOp::Lt, tvar("n"), tint(INT_MAX)),
                    tadd(tvar("n"), tint(1), span_at(4, 5)),
                    tvar("n"),
                ),
            },
        ]
        .into(),
    ));
    prove_contract(&inc_opt_spec, &guarded_opt, &data, &prog, &verified, &l)
        .expect("guarded Option Int match certified");
}

#[test]
fn nested_parametric_adt_option_of_option_is_admitted_and_out_of_range_int_pat_refused_without_panic()
 {
    let l = lang();
    let prog = FakeProgram::default();
    let verified = VerifiedContracts::new();

    let mut data = Data::new();
    data.insert(
        ("Prelude".into(), "Option".into()),
        DataDecl {
            params: vec!["a".into()],
            constructors: vec![
                ("None".into(), vec![]),
                ("Some".into(), vec![Ty::Rigid("a".into())]),
            ],
            recursive: false,
        },
    );
    let nested_opt = Ty::Data {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![Ty::Data {
            module: "Prelude".into(),
            name: "Option".into(),
            args: vec![Ty::Int],
        }],
    };
    let spec = ContractSpec {
        module: "App".into(),
        name: "unwrap_or_zero".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("v".into(), nested_opt)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            Term::new(
                TermKind::Lam("r".into(), Rc::new(tcmp(IntCmpOp::Eq, tvar("r"), tint(0)))),
                span_at(2, 5),
            ),
            span_at(2, 5),
        )],
    };
    prove_contract(&spec, &tint(0), &data, &prog, &verified, &l)
        .expect("Option (Option Int) must not be falsely rejected as recursive");

    // Out-of-i32 IntLit in Body mode and Pat::Int(i128::MIN) must refuse cleanly without panic.
    let min_pat_spec = ContractSpec {
        module: "App".into(),
        name: "bad_pat".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![],
    };
    let match_min = Term::new(
        TermKind::Match(
            Rc::new(tvar("x")),
            Rc::from([
                Arm {
                    pat: Pat::Int(i128::MIN),
                    body: tint(0),
                },
                Arm {
                    pat: Pat::Wild,
                    body: tint(1),
                },
            ]),
        ),
        span_at(1, 5),
    );
    let err = prove_contract(&min_pat_spec, &match_min, &data, &prog, &verified, &l)
        .expect_err("Pat::Int(i128::MIN) must be refused cleanly");
    assert_eq!(err.diagnostic("bad_pat").code, codes::T082);

    let big_lit = Term::new(TermKind::IntLit(i128::from(i32::MAX) + 1), span_at(1, 5));
    let err = prove_contract(&min_pat_spec, &big_lit, &data, &prog, &verified, &l)
        .expect_err("out-of-i32 IntLit in body must be refused cleanly");
    assert_eq!(err.diagnostic("bad_pat").code, codes::T082);
}
