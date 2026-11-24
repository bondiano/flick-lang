use std::rc::Rc;

use flick_ir::{Arith, BinOp, ExprKind, Num, Param, Pat as IrPat, Stage};

use super::super::{
    Arm, ContractFailure, ContractSpec, INT_MAX, IntCmpOp, Pat, TermKind, VerifiedContracts,
    prove_contract,
};
use super::{
    FakeProgram, bin, expr, int_lit, lang, local, span, span_at, t, tadd, tand, tapp, tcmp, tdef,
    tif, tint, tlam, tlet, tnot, tor, tpipe, tvar, ty_name,
};
use crate::diagnostics::codes;
use crate::logic::{Data, TUPLES, Ty, term::tuple as tuple_ctor};

#[test]
fn prove_short_circuit_and_or_and_branching() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();

    // 1. Short-circuit `&&`: `x < 2147483647 && x + 1 > x` establishes guard BEFORE `+`
    let sc_and_spec = ContractSpec {
        module: "App".into(),
        name: "sc_and".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Bool,
        requires: vec![],
        ensures: vec![(
            tlam(
                "r",
                t(TermKind::BoolEq {
                    eq: true,
                    lhs: Rc::new(tvar("r")),
                    rhs: Rc::new(tcmp(IntCmpOp::Lt, tvar("x"), tint(INT_MAX))),
                }),
            ),
            span_at(1, 2),
        )],
    };
    let sc_and_body = tand(
        tcmp(IntCmpOp::Lt, tvar("x"), tint(INT_MAX)),
        tcmp(
            IntCmpOp::Gt,
            tadd(tvar("x"), tint(1), span_at(3, 4)),
            tvar("x"),
        ),
    );
    prove_contract(&sc_and_spec, &sc_and_body, &data, &prog, &verified, &l)
        .expect("short-circuit && guard certified");

    // 2. Short-circuit `||`: `x >= 2147483647 || x + 1 > x` always true without overflow
    let sc_or_spec = ContractSpec {
        module: "App".into(),
        name: "sc_or".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Bool,
        requires: vec![],
        ensures: vec![(tlam("r", tvar("r")), span_at(1, 2))],
    };
    let sc_or_body = tor(
        tcmp(IntCmpOp::Ge, tvar("x"), tint(INT_MAX)),
        tcmp(
            IntCmpOp::Gt,
            tadd(tvar("x"), tint(1), span_at(3, 4)),
            tvar("x"),
        ),
    );
    prove_contract(&sc_or_spec, &sc_or_body, &data, &prog, &verified, &l)
        .expect("short-circuit || guard certified");
}

#[test]
fn prove_match_let_not_if_and_disjunctive_ensure() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();
    let sign_spec = ContractSpec {
        module: "App".into(),
        name: "sign".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![
            (
                tlam(
                    "r",
                    tor(
                        tcmp(IntCmpOp::Eq, tvar("r"), tint(-1)),
                        tor(
                            tcmp(IntCmpOp::Eq, tvar("r"), tint(0)),
                            tcmp(IntCmpOp::Eq, tvar("r"), tint(1)),
                        ),
                    ),
                ),
                span_at(1, 2),
            ),
            (
                tlam(
                    "r",
                    tand(
                        tcmp(IntCmpOp::Le, tint(-1), tvar("r")),
                        tcmp(IntCmpOp::Le, tvar("r"), tint(1)),
                    ),
                ),
                span_at(2, 3),
            ),
        ],
    };
    let sign_body = t(TermKind::Match(
        Rc::new(tvar("x")),
        vec![
            Arm {
                pat: Pat::Int(0),
                body: tint(0),
            },
            Arm {
                pat: Pat::Bind("n".into()),
                body: tlet(
                    "is_pos",
                    tnot(tcmp(IntCmpOp::Le, tvar("n"), tint(0))),
                    tif(tvar("is_pos"), tint(1), tint(-1)),
                ),
            },
        ]
        .into(),
    ));
    prove_contract(&sign_spec, &sign_body, &data, &prog, &verified, &l)
        .expect("sign with match/let/not/if/disjunctive ensure certified");
}

#[test]
fn prove_tuple_match_pipe_order_helpers_and_bool_counterexample() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();

    // Tuple match `(Int, Int)`
    let pair_ty = Ty::Data {
        module: TUPLES.into(),
        name: "Tuple".into(),
        args: vec![Ty::Int, Ty::Int],
    };
    let swap_sum = ContractSpec {
        module: "App".into(),
        name: "sum_pair".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("p".into(), pair_ty)],
        result_ty: Ty::Int,
        requires: vec![(
            t(TermKind::Match(
                Rc::new(tvar("p")),
                vec![Arm {
                    pat: Pat::Con(
                        tuple_ctor(2),
                        vec![Pat::Bind("a".into()), Pat::Bind("b".into())],
                    ),
                    body: tand(
                        tand(
                            tcmp(IntCmpOp::Le, tint(0), tvar("a")),
                            tcmp(IntCmpOp::Le, tvar("a"), tint(100)),
                        ),
                        tand(
                            tcmp(IntCmpOp::Le, tint(0), tvar("b")),
                            tcmp(IntCmpOp::Le, tvar("b"), tint(100)),
                        ),
                    ),
                }]
                .into(),
            )),
            span_at(1, 2),
        )],
        ensures: vec![(
            tlam(
                "r",
                tand(
                    tcmp(IntCmpOp::Le, tint(0), tvar("r")),
                    tcmp(IntCmpOp::Le, tvar("r"), tint(200)),
                ),
            ),
            span_at(2, 3),
        )],
    };
    let swap_body = t(TermKind::Match(
        Rc::new(tvar("p")),
        vec![Arm {
            pat: Pat::Con(
                tuple_ctor(2),
                vec![Pat::Bind("x".into()), Pat::Bind("y".into())],
            ),
            body: tadd(tvar("x"), tvar("y"), span_at(4, 5)),
        }]
        .into(),
    ));
    prove_contract(&swap_sum, &swap_body, &data, &prog, &verified, &l)
        .expect("tuple match certified");

    // Pipe order (`arg_first = true`) checks `x + 1` even when ignored, stripping `$1`
    let pipe_spec = ContractSpec {
        module: "App".into(),
        name: "pipe_ovf".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x$1".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("r", tcmp(IntCmpOp::Eq, tvar("r"), tint(0))),
            span_at(1, 2),
        )],
    };
    let pipe_body = tpipe(
        tadd(tvar("x$1"), tint(1), span_at(4, 5)),
        tlam("_ignored", tint(0)),
        span_at(3, 6),
    );
    let err = prove_contract(&pipe_spec, &pipe_body, &data, &prog, &verified, &l)
        .expect_err("pipe arg overflow must fire even when ignored");
    assert!(matches!(
        err,
        ContractFailure::Overflow {
            span,
            counterexample: Some(ref ce),
            ..
        } if span == span_at(4, 5) && ce.bindings == vec![("x".into(), "2147483647".into())]
    ));
}

#[test]
fn prove_unfolded_helper_range_and_bool_counterexample() {
    let mut prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();

    // Uncontracted helper unfolded in Body enforces range check
    prog.add_value(
        "App",
        "inc",
        expr(ExprKind::Lambda {
            params: vec![Param {
                pat: IrPat::Bind("v".into()),
                stage: Stage::Pure,
                ty: Some(ty_name("Prelude", "Int")),
                binder_types: Vec::new(),
            }],
            body: Box::new(bin(
                BinOp::Arith(Arith::Add, Num::Int),
                local("v"),
                int_lit(1),
            )),
        }),
    );
    let helper_spec = ContractSpec {
        module: "App".into(),
        name: "use_inc".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam(
                "r",
                tcmp(
                    IntCmpOp::Eq,
                    tvar("r"),
                    tapp(tdef("App", "inc"), tvar("x"), span()),
                ),
            ),
            span_at(1, 2),
        )],
    };
    let err = prove_contract(
        &helper_spec,
        &tapp(tdef("App", "inc"), tvar("x"), span_at(5, 6)),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect_err("unfolded helper in body must enforce range check");
    assert_eq!(err.diagnostic("use_inc").code, codes::T089);

    // Pure boolean failure still queries Omega so Int param satisfies `require`
    let bool_fail_spec = ContractSpec {
        module: "App".into(),
        name: "bool_fail".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int), ("flag".into(), Ty::Bool)],
        result_ty: Ty::Int,
        requires: vec![(tcmp(IntCmpOp::Eq, tvar("x"), tint(7)), span_at(1, 2))],
        ensures: vec![(tlam("_r", tvar("flag")), span_at(3, 4))],
    };
    let err = prove_contract(&bool_fail_spec, &tvar("x"), &data, &prog, &verified, &l)
        .expect_err("unconstrained Bool must fail T087");
    assert!(matches!(
        err,
        ContractFailure::Postcondition {
            counterexample: Some(ref ce),
            ..
        } if ce.bindings == vec![("x".into(), "7".into()), ("flag".into(), "false".into())]
    ));
}
