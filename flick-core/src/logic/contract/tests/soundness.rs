use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use flick_ir::{Arith, BinOp, ExprKind, Num, Param, Pat as IrPat, Stage};

use super::super::{
    Budgets, ContractSpec, IntCmpOp, LeafCert, PathCert, TermKind, VerifiedContracts,
    check_contract_cert_with_budgets, prove_contract, prove_contract_with_budgets,
};
use super::{
    FakeProgram, bin, consume_spec, expr, global, int_lit, lang, local, span, span_at, t, tadd,
    tand, tapp, tbool, tcmp, tdef, tif, tint, tlam, tsub, tvar, ty_name,
};
use crate::diagnostics::codes;
use crate::logic::{
    Cert as V1Cert, Data, DataDecl, Def, Equation, Failure as V1Failure, Goal, Rule, Statement,
    Term as V1Term, Ty, prove as prove_v1,
};

#[test]
fn prove_refuses_failed_cyclic_self_recursive_and_escaping_contracted() {
    let mut prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let mut verified = VerifiedContracts::new();

    // 1. Failed contract is NEVER added to `VerifiedContracts`, and callers get T082
    let bad_consume = consume_spec(span_at(1, 2), span_at(3, 4), span_at(0, 9));
    // Also place a valid-looking body in `prog` to prove caller NEVER silently unfolds it!
    prog.add_value("App", "consume", int_lit(0));
    assert!(
        verified
            .prove_and_record(&bad_consume, &tvar("available"), &data, &prog, &l)
            .is_err()
    );
    assert!(!verified.is_verified("App", "consume"));
    assert!(verified.is_unverified("App", "consume"));

    let caller = ContractSpec {
        module: "App".into(),
        name: "caller".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("a".into(), Ty::Int), ("u".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(
            tand(
                tcmp(IntCmpOp::Le, tint(0), tvar("u")),
                tcmp(IntCmpOp::Le, tvar("u"), tvar("a")),
            ),
            span_at(1, 2),
        )],
        ensures: vec![(
            tlam("r", tcmp(IntCmpOp::Le, tint(0), tvar("r"))),
            span_at(3, 4),
        )],
    };
    let caller_body = tapp(
        tapp(tdef("App", "consume"), tvar("a"), span()),
        tvar("u"),
        span_at(5, 6),
    );
    let err = prove_contract(&caller, &caller_body, &data, &prog, &verified, &l)
        .expect_err("failed callee must never be assumed or unfolded");
    assert_eq!(err.diagnostic("caller").code, codes::T082);

    // 2. Self-recursive contracted function refused with T082 even under `require false`
    let self_rec = ContractSpec {
        module: "App".into(),
        name: "spin".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(tbool(false), span_at(1, 2))],
        ensures: vec![(
            tlam("y", tcmp(IntCmpOp::Eq, tvar("y"), tint(0))),
            span_at(3, 4),
        )],
    };
    let self_rec_body = tapp(tdef("App", "spin"), tvar("x"), span_at(5, 6));
    let err = prove_contract(&self_rec, &self_rec_body, &data, &prog, &verified, &l)
        .expect_err("self-recursive contract must be refused even with require false");
    assert_eq!(err.diagnostic("spin").code, codes::T082);

    // 3. Uncontracted recursive helper refused even under `require false`
    prog.add_value("App", "loop_h", global("App", "loop_h"));
    let calls_loop = ContractSpec {
        module: "App".into(),
        name: "calls_loop".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(tbool(false), span_at(1, 2))],
        ensures: vec![],
    };
    let err = prove_contract(
        &calls_loop,
        &tdef("App", "loop_h"),
        &data,
        &prog,
        &verified,
        &l,
    )
    .expect_err("recursive helper must be refused even under require false");
    assert_eq!(err.diagnostic("calls_loop").code, codes::T082);
}

#[test]
fn prove_refuses_escaping_contracted_and_unsupported_types() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let mut verified_ok = VerifiedContracts::new();

    let ok_consume = consume_spec(span_at(1, 2), span_at(3, 4), span_at(0, 9));
    verified_ok
        .prove_and_record(
            &ok_consume,
            &tsub(tvar("available"), tvar("used"), span_at(5, 6)),
            &data,
            &prog,
            &l,
        )
        .expect("consume certified");

    let partial_escape = ContractSpec {
        module: "App".into(),
        name: "partial".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("a".into(), Ty::Int)],
        result_ty: Ty::Fun(Box::new(Ty::Int), Box::new(Ty::Int)),
        requires: vec![],
        ensures: vec![],
    };
    let err = prove_contract(
        &partial_escape,
        &tapp(tdef("App", "consume"), tvar("a"), span_at(5, 6)),
        &data,
        &prog,
        &verified_ok,
        &l,
    )
    .expect_err("escaping partially applied contracted function must fail T082");
    assert_eq!(err.diagnostic("partial").code, codes::T082);

    let float_spec = ContractSpec {
        module: "App".into(),
        name: "scale".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![(
            "x".into(),
            Ty::Refused("`Float` is outside the domain".into()),
        )],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![],
    };
    let err = prove_contract(&float_spec, &tint(0), &data, &prog, &verified_ok, &l)
        .expect_err("Float param refused");
    assert_eq!(err.diagnostic("scale").code, codes::T082);

    let mut rec_data = Data::new();
    rec_data.insert(
        ("Prelude".into(), "List".into()),
        DataDecl {
            params: vec!["a".into()],
            constructors: vec![("Nil".into(), vec![]), ("Cons".into(), vec![])],
            recursive: true,
        },
    );
    let list_spec = ContractSpec {
        module: "App".into(),
        name: "len".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![(
            "xs".into(),
            Ty::Data {
                module: "Prelude".into(),
                name: "List".into(),
                args: vec![Ty::Int],
            },
        )],
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![],
    };
    let err = prove_contract(&list_spec, &tint(0), &rec_data, &prog, &verified_ok, &l)
        .expect_err("recursive ADT refused");
    assert_eq!(err.diagnostic("len").code, codes::T082);
}

#[test]
fn prove_budget_exhaustion_emits_t084() {
    let prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();

    // 1. Default MAX_PATHS = 1024 exceeded by 11 independent boolean branches (2^11 = 2048 paths)
    let mut params = Vec::new();
    let mut body = tint(0);
    for i in 0..11 {
        let name: Rc<str> = format!("b{i}").into();
        params.push((name.clone(), Ty::Bool));
        body = tif(t(TermKind::Var(name)), body.clone(), body);
    }
    let many_paths = ContractSpec {
        module: "App".into(),
        name: "many_paths".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params,
        result_ty: Ty::Int,
        requires: vec![],
        ensures: vec![(
            tlam("r", tcmp(IntCmpOp::Eq, tvar("r"), tint(0))),
            span_at(1, 2),
        )],
    };
    let err = prove_contract(&many_paths, &body, &data, &prog, &verified, &l)
        .expect_err("2^11 paths must exceed default MAX_PATHS");
    assert_eq!(err.diagnostic("many_paths").code, codes::T084);

    // 2. Explicit tiny step, depth, and omega budgets -> T084
    let consume = consume_spec(span_at(1, 2), span_at(3, 4), span_at(0, 9));
    let consume_body = tsub(tvar("available"), tvar("used"), span_at(5, 6));
    let valid_cert = prove_contract(&consume, &consume_body, &data, &prog, &verified, &l)
        .expect("consume certified under default budgets");

    for (label, budgets) in [
        (
            "steps=2",
            Budgets {
                max_steps: 2,
                ..Budgets::default()
            },
        ),
        (
            "depth=2",
            Budgets {
                max_depth: 2,
                ..Budgets::default()
            },
        ),
        (
            "omega=1",
            Budgets {
                omega_budget: 1,
                ..Budgets::default()
            },
        ),
    ] {
        let err = prove_contract_with_budgets(
            &consume,
            &consume_body,
            &data,
            &prog,
            &verified,
            &l,
            budgets,
        )
        .expect_err(label);
        assert_eq!(err.diagnostic("consume").code, codes::T084, "{label}");
    }

    // Kernel also enforces step/depth budgets -> T084
    let kernel_exh = check_contract_cert_with_budgets(
        &consume,
        &consume_body,
        &valid_cert,
        &data,
        &prog,
        &verified,
        &l,
        Budgets {
            max_steps: 2,
            ..Budgets::default()
        },
    )
    .expect_err("kernel step limit");
    assert_eq!(kernel_exh.diagnostic("consume").code, codes::T084);
}

#[test]
fn require_false_does_not_admit_function_to_pure_total_v1() {
    let mut prog = FakeProgram::default();
    let l = lang();
    let data = Data::new();
    let verified = VerifiedContracts::new();

    // `let f x where require false ensure (fun y -> y == 0) = x + 1`
    let f_ir = expr(ExprKind::Lambda {
        params: vec![Param {
            pat: IrPat::Bind("x".into()),
            stage: Stage::Pure,
            ty: Some(ty_name("Prelude", "Int")),
            binder_types: Vec::new(),
        }],
        body: Box::new(bin(
            BinOp::Arith(Arith::Add, Num::Int),
            local("x"),
            int_lit(1),
        )),
    });
    prog.add_value("App", "vacuous", f_ir);

    let vacuous_spec = ContractSpec {
        module: "App".into(),
        name: "vacuous".into(),
        decl_span: span_at(0, 9),
        contract_span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        result_ty: Ty::Int,
        requires: vec![(tbool(false), span_at(1, 2))],
        ensures: vec![(
            tlam("y", tcmp(IntCmpOp::Eq, tvar("y"), tint(0))),
            span_at(3, 4),
        )],
    };
    let vacuous_body = tadd(tvar("x"), tint(1), span_at(5, 6));
    let cert = prove_contract(&vacuous_spec, &vacuous_body, &data, &prog, &verified, &l)
        .expect("vacuous require false is certified in PureTotalV2");
    assert_eq!(cert.root, PathCert::Dead(LeafCert::Trivial));

    // In PureTotalV1, `vacuous` is STILL refused (T082) because its body uses `+`
    let v1_stmt = Statement {
        name: "try_use_vacuous".into(),
        span: span_at(0, 9),
        params: vec![("x".into(), Ty::Int)],
        premises: vec![],
        goal: Goal::Equal(Equation {
            ty: Ty::Int,
            left: V1Term::App(
                Rc::new(V1Term::Def(Rc::new(Def::Value {
                    module: "App".into(),
                    name: "vacuous".into(),
                }))),
                Rc::new(V1Term::Var("x".into())),
            ),
            right: V1Term::Var("x".into()),
        }),
    };
    let v1_err = prove_v1(
        &v1_stmt,
        &V1Cert {
            kind: Rule::Refl,
            span: span_at(0, 9),
        },
        &data,
        &prog,
        &[],
    )
    .expect_err("require false in PureTotalV2 must NOT admit function to PureTotalV1");
    assert!(matches!(v1_err, V1Failure::Refused(_)), "got: {v1_err:?}");
    assert_eq!(v1_err.diagnostic("try_use_vacuous").code, codes::T082);
}

fn collect_rs_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)
            .expect("readable source directory")
            .map(|e| e.expect("readable dir entry").path())
            .collect();
        entries.sort();
        for entry in entries {
            collect_rs_files(&entry, out);
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        out.push(path.to_path_buf());
    }
}

#[test]
fn verify_kernel_never_imports_solver_or_omega_decide() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/logic/contract");
    for module in ["verify", "state"] {
        let dir = root.join(module);
        let single = root.join(format!("{module}.rs"));
        let mut files = Vec::new();
        if dir.is_dir() {
            collect_rs_files(&dir, &mut files);
        } else {
            assert!(
                single.is_file(),
                "expected `{}` or `{}` to exist",
                dir.display(),
                single.display()
            );
            files.push(single);
        }
        assert!(
            !files.is_empty(),
            "expected at least one `.rs` file for `{module}`"
        );
        for file in files {
            let text = fs::read_to_string(&file).expect("readable source");
            let code: String = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let label = file.strip_prefix(&root).unwrap_or(&file).display();
            assert!(
                !code.contains("decide"),
                "{label} must never call or import omega::decide / decide_budgeted"
            );
            assert!(
                !code.contains("super::eval"),
                "{label} must remain completely independent of eval.rs"
            );
        }
    }
}
