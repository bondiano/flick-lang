use std::rc::Rc;

use flick_ir::{BinOp, ExprKind, ItemKind, Param, Pat as IrPat, Stage};

use super::super::{
    ContractCert, ContractFailure, ContractSpec, INT_MAX, IntCmpOp, LeafCert, PathCert, PrimHint,
    VerifiedContracts, check_contract_cert, extract, prove_contract,
};
use super::{
    FakeProgram, bin, ce_int, check_and_lower, consume_spec, expr, int_lit, lang, local, span,
    span_at, tadd, tand, tapp, tcmp, tdef, tint, tlam, tsub, tvar, ty_name,
};
use crate::diagnostics::codes;
use crate::logic::{DOMAIN_V2, Data, Ty};
use crate::source::Span;

fn verified_consume_fixture(req_span: Span) -> (FakeProgram, VerifiedContracts) {
    let mut prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let mut verified = VerifiedContracts::new();

    // Uncontracted helper `is_nonneg` used inside `consume`'s contract
    prog.add_value(
        "App",
        "is_nonneg",
        expr(ExprKind::Lambda {
            params: vec![Param {
                pat: IrPat::Bind("n".into()),
                stage: Stage::Pure,
                ty: Some(ty_name("Prelude", "Int")),
            }],
            body: Box::new(bin(BinOp::Le, int_lit(0), local("n"))),
        }),
    );

    let mut consume = consume_spec(req_span, span_at(3, 4), span_at(0, 9));
    consume.requires = vec![(
        tand(
            tapp(tdef("App", "is_nonneg"), tvar("used"), span()),
            tcmp(IntCmpOp::Le, tvar("used"), tvar("available")),
        ),
        req_span,
    )];
    let consume_body = tsub(tvar("available"), tvar("used"), span_at(5, 6));
    verified
        .prove_and_record(&consume, &consume_body, &data, &prog, &l)
        .expect("consume certified");
    assert!(verified.is_verified("App", "consume"));
    (prog, verified)
}

#[test]
fn prove_caller_of_consume_certified_and_rejects_call_tampering() {
    let (prog, verified) = verified_consume_fixture(span_at(1, 2));
    let l = lang();
    let data = Data::new();

    let call_span = span_at(6, 9);
    let call_consume = tapp(
        tapp(tdef("App", "consume"), tvar("available"), span_at(6, 8)),
        tvar("used"),
        call_span,
    );
    let take_ok = ContractSpec {
        module: "App".into(),
        name: "take".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("available".into(), Ty::Int), ("used".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(
            tand(
                tcmp(IntCmpOp::Le, tint(0), tvar("used")),
                tcmp(IntCmpOp::Le, tvar("used"), tvar("available")),
            ),
            span_at(1, 2),
        )],
        ensures: vec![(
            tlam("remaining", tcmp(IntCmpOp::Le, tint(0), tvar("remaining"))),
            span_at(3, 4),
        )],
    };
    let take_cert = prove_contract(&take_ok, &call_consume, &data, &prog, &verified, &l)
        .expect("take with sufficient guard should be certified");
    assert!(matches!(
        &take_cert.root,
        PathCert::Call { callee, .. } if callee == "consume"
    ));

    let PathCert::Call { pre, post, .. } = &take_cert.root else {
        panic!("expected Call cert");
    };
    for (label, tampered) in [
        (
            "wrong callee",
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Call {
                    callee: "other".into(),
                    pre: pre.clone(),
                    post: post.clone(),
                },
            },
        ),
        (
            "skipped call",
            ContractCert {
                domain: DOMAIN_V2,
                root: (**post).clone(),
            },
        ),
        (
            "forged pre",
            ContractCert {
                domain: DOMAIN_V2,
                root: PathCert::Call {
                    callee: "consume".into(),
                    pre: Box::new(PathCert::Leaf(LeafCert::Trivial)),
                    post: post.clone(),
                },
            },
        ),
    ] {
        let err = check_contract_cert(
            &take_ok,
            &call_consume,
            &tampered,
            &data,
            &prog,
            &verified,
            &l,
        )
        .expect_err(label);
        assert_eq!(err.diagnostic("take").code, codes::T083, "{label}");
    }
}

#[test]
fn prove_caller_of_consume_looser_guard_t088_and_summary_composition() {
    let req_span = span_at(1, 2);
    let (prog, verified) = verified_consume_fixture(req_span);
    let l = lang();
    let data = Data::new();

    let call_span = span_at(6, 9);
    let call_consume = tapp(
        tapp(tdef("App", "consume"), tvar("available"), span_at(6, 8)),
        tvar("used"),
        call_span,
    );
    let take_bad = ContractSpec {
        module: "App".into(),
        name: "take".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("available".into(), Ty::Int), ("used".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(tcmp(IntCmpOp::Le, tint(0), tvar("used")), span_at(1, 2))],
        ensures: vec![(
            tlam("remaining", tcmp(IntCmpOp::Le, tint(0), tvar("remaining"))),
            span_at(3, 4),
        )],
    };
    let err = prove_contract(&take_bad, &call_consume, &data, &prog, &verified, &l)
        .expect_err("looser guard must fail T088");
    let ContractFailure::Precondition {
        span: got_span,
        ref callee,
        callee_require,
        counterexample: Some(ref ce),
    } = err
    else {
        panic!("expected Precondition with counterexample, got: {err:?}");
    };
    assert_eq!(got_span, call_span);
    assert_eq!(callee, "consume");
    assert_eq!(callee_require, Some(req_span));
    let diag = err.diagnostic("take");
    assert_eq!(diag.code, codes::T088);
    assert_eq!(diag.related.len(), 1);
    let avail = ce_int(ce, "available");
    let used = ce_int(ce, "used");
    assert!(
        0 <= used && used > avail,
        "unexpected counterexample: {ce:?}"
    );

    // Caller using callee's `ensure` to discharge caller's `+ 1` range and postcondition
    let take_plus_one = ContractSpec {
        module: "App".into(),
        name: "take_plus_one".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("available".into(), Ty::Int), ("used".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(
            tand(
                tand(
                    tcmp(IntCmpOp::Le, tint(0), tvar("used")),
                    tcmp(IntCmpOp::Le, tvar("used"), tvar("available")),
                ),
                tcmp(IntCmpOp::Lt, tvar("available"), tint(INT_MAX)),
            ),
            span_at(1, 2),
        )],
        ensures: vec![(
            tlam(
                "r",
                tand(
                    tcmp(IntCmpOp::Ge, tvar("r"), tint(1)),
                    tcmp(
                        IntCmpOp::Eq,
                        tadd(tvar("r"), tvar("used"), span()),
                        tadd(tvar("available"), tint(1), span()),
                    ),
                ),
            ),
            span_at(3, 4),
        )],
    };
    prove_contract(
        &take_plus_one,
        &tadd(call_consume, tint(1), span_at(7, 8)),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect("callee ensure should discharge caller range and postcondition");
}

#[test]
fn prove_zero_arity_contracted_value_summary() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let mut verified = VerifiedContracts::new();
    let limit_spec = ContractSpec {
        module: "App".into(),
        name: "limit".into(),
        decl_span: span_at(0, 5),
        contract_span: span_at(0, 5),
        params: vec![],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("r", tcmp(IntCmpOp::Eq, tvar("r"), tint(10))),
            span_at(1, 2),
        )],
    };
    verified
        .prove_and_record(&limit_spec, &tint(10), &data, &prog, &l)
        .expect("0-arity contracted value certified");
    let use_limit = ContractSpec {
        module: "App".into(),
        name: "use_limit".into(),
        decl_span: span_at(0, 5),
        contract_span: span_at(0, 5),
        params: vec![],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("r", tcmp(IntCmpOp::Eq, tvar("r"), tint(11))),
            span_at(1, 2),
        )],
    };
    prove_contract(
        &use_limit,
        &tadd(tdef("App", "limit"), tint(1), span_at(3, 4)),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect("caller of 0-arity contracted value certified");
}

#[test]
fn prove_extracted_real_flick_ir_for_consume() {
    let source = r"module App.LiveC3 exposing (..)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used
";
    let (typing, ir_mod) = check_and_lower("App/LiveC3.fl", source);
    let info = typing
        .contracts
        .iter()
        .find(|c| c.name == "consume")
        .expect("consume contract metadata");
    let body_ir = ir_mod
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "consume" => match &body.kind {
                ExprKind::Lambda { body: inner, .. } => Some(inner.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .expect("consume body IR");

    let prog = FakeProgram::default();
    let l = lang();
    let ty_env: Vec<(Rc<str>, PrimHint)> = vec![
        ("available".into(), PrimHint::Int),
        ("used".into(), PrimHint::Int),
    ];
    let body_term = extract(body_ir, &ty_env, &prog, &l).expect("extract real consume IR");
    let spec = consume_spec(span_at(1, 2), span_at(3, 4), span_at(0, 9));
    let verified = VerifiedContracts::new();
    prove_contract(&spec, &body_term, &info.data, &prog, &verified, &l)
        .expect("real checked IR for consume should certify");
}
