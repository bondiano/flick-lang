use std::rc::Rc;

use flick_ir::{
    Arith, Arm as IrArm, BinOp, Constructor, Dict, ExprKind, Lit, Num, Pat as IrPat, Stage, Stmt,
};

use super::super::{IntBinOp, IntCmpOp, Pat, PrimHint, Term, TermKind, extract};
use super::{FakeProgram, bin, bool_lit, expr, global, int_lit, lang, local, typed_expr};

#[test]
fn extract_accepts_pure_total_v2_int_and_bool_ops() {
    let prog = FakeProgram::default();
    let l = lang();
    let env: Vec<(Rc<str>, PrimHint)> = vec![
        ("x".into(), PrimHint::Int),
        ("y".into(), PrimHint::Int),
        ("flag".into(), PrimHint::Bool),
    ];

    // (x + y) - (-x) <= 10 && (x === y || !(flag === true))
    let add = bin(BinOp::Arith(Arith::Add, Num::Int), local("x"), local("y"));
    let neg = expr(ExprKind::Neg(Num::Int, Box::new(local("x"))));
    let sub = bin(BinOp::Arith(Arith::Sub, Num::Int), add, neg);
    let le = bin(BinOp::Le, sub, int_lit(10));
    let int_eq = bin(BinOp::Same, local("x"), local("y"));
    let bool_eq = bin(BinOp::Same, local("flag"), bool_lit(true));
    let not_eq = expr(ExprKind::Not(Box::new(bool_eq)));
    let or_e = bin(BinOp::Or, int_eq, not_eq);
    let full = bin(BinOp::And, le, or_e);

    let extracted = extract(&full, &env, &prog, &l).expect("valid PureTotalV2 term");
    let TermKind::And(left, _) = &extracted.kind else {
        panic!("expected And");
    };
    let TermKind::IntCmp(IntCmpOp::Le, sub_t, _) = &left.kind else {
        panic!("expected Le");
    };
    assert!(matches!(sub_t.kind, TermKind::IntBin(IntBinOp::Sub, ..)));

    // `if` and `let` preserve spans and propagate Int hints across `let`
    let block = expr(ExprKind::Block(
        vec![Stmt::let_bind(
            IrPat::Bind("z".into()),
            bin(BinOp::Arith(Arith::Add, Num::Int), local("x"), int_lit(1)),
        )],
        Box::new(expr(ExprKind::If(
            Box::new(bin(BinOp::Gt, local("z"), int_lit(0))),
            Box::new(local("z")),
            Box::new(int_lit(0)),
        ))),
    ));
    let extracted_block = extract(&block, &env, &prog, &l).expect("block with let + if");
    assert!(matches!(extracted_block.kind, TermKind::Let(..)));
}

#[test]
fn extract_resolves_top_level_int_constant_without_expr_ty() {
    let mut prog = FakeProgram::default();
    prog.add_value("Int", "max_value", int_lit(i32::MAX));
    let l = lang();
    let env: Vec<(Rc<str>, PrimHint)> = vec![("x".into(), PrimHint::Int)];

    // Neither side carries `Expr::ty`, and `Int.max_value` is a Global value
    let cmp = bin(BinOp::Lt, local("x"), global("Int", "max_value"));
    let t = extract(&cmp, &env, &prog, &l).expect("Int.max_value inferred as Int");
    assert!(matches!(t.kind, TermKind::IntCmp(IntCmpOp::Lt, ..)));

    // Even when `x` is not in env, `Int.max_value` alone confirms Int
    let t2 = extract(&cmp, &[], &prog, &l).expect("rhs Int.max_value confirms Int");
    assert!(matches!(t2.kind, TermKind::IntCmp(IntCmpOp::Lt, ..)));
}

#[test]
fn extract_distinguishes_compiler_tag_eq_from_user_record_field() {
    let prog = FakeProgram::default();
    let l = lang();

    // Compiler-emitted nullary ADT `Identity::Tag` has `ty == None` on `_tag` fields
    let tag_lhs = expr(ExprKind::Field(Box::new(local("a")), "_tag".into()));
    let tag_rhs = expr(ExprKind::Field(Box::new(local("b")), "_tag".into()));
    let tag_eq = bin(BinOp::NotSame, tag_lhs, tag_rhs);
    let t = extract(&tag_eq, &[], &prog, &l).expect("TagEq accepted");
    assert!(matches!(t.kind, TermKind::TagEq { eq: false, .. }));

    // User record field access `r._tag == s._tag` carrying `ty = Some(Int)` must NOT be TagEq
    let user_lhs = typed_expr(
        ExprKind::Field(Box::new(local("r")), "_tag".into()),
        "Prelude",
        "Int",
    );
    let user_rhs = typed_expr(
        ExprKind::Field(Box::new(local("s")), "_tag".into()),
        "Prelude",
        "Int",
    );
    let user_eq = bin(BinOp::Same, user_lhs, user_rhs);
    let err = extract(&user_eq, &[], &prog, &l).expect_err("record field must be refused");
    assert!(err.why.contains("record"), "got: {}", err.why);
}

#[test]
fn extract_refuses_unsupported_literals_arith_and_comparisons() {
    let prog = FakeProgram::default();
    let l = lang();

    for op in [Arith::Mul, Arith::Div, Arith::Rem, Arith::Pow] {
        let e = bin(BinOp::Arith(op, Num::Int), int_lit(2), int_lit(3));
        assert!(extract(&e, &[], &prog, &l).is_err(), "must refuse {op:?}");
    }

    for lit in [
        Lit::Float(1.5),
        Lit::BigInt("1".into()),
        Lit::String("a".into()),
    ] {
        assert!(extract(&expr(ExprKind::Lit(lit)), &[], &prog, &l).is_err());
    }
    let float_add = bin(
        BinOp::Arith(Arith::Add, Num::Float),
        expr(ExprKind::Lit(Lit::Float(1.0))),
        expr(ExprKind::Lit(Lit::Float(2.0))),
    );
    assert!(extract(&float_add, &[], &prog, &l).is_err());
    let float_neg = expr(ExprKind::Neg(Num::Float, Box::new(local("x"))));
    assert!(extract(&float_neg, &[], &prog, &l).is_err());

    let float_env: Vec<(Rc<str>, PrimHint)> = vec![("a".into(), PrimHint::Float)];
    let str_env: Vec<(Rc<str>, PrimHint)> = vec![("s".into(), PrimHint::String)];
    let lt_ab = bin(BinOp::Lt, local("a"), local("b"));
    assert!(extract(&lt_ab, &float_env, &prog, &l).is_err());
    assert!(extract(&lt_ab, &[], &prog, &l).is_err());
    let same_st = bin(BinOp::Same, local("s"), local("t"));
    assert!(extract(&same_st, &str_env, &prog, &l).is_err());
    assert!(extract(&bin(BinOp::Same, local("u"), local("v")), &[], &prog, &l).is_err());

    assert!(extract(&bin(BinOp::Eq, local("x"), local("y")), &[], &prog, &l).is_err());
    assert!(extract(&bin(BinOp::NotEq, local("x"), local("y")), &[], &prog, &l).is_err());
}

#[test]
fn extract_refuses_dicts_effects_guards_and_local_recursion() {
    let prog = FakeProgram::default();
    let l = lang();

    let dicts = expr(ExprKind::Dicts(
        Box::new(global("M", "f")),
        vec![Dict::Given("Ord a".into())],
    ));
    assert!(extract(&dicts, &[], &prog, &l).is_err());

    let runtime_app = expr(ExprKind::App {
        func: Box::new(local("f")),
        arg: Box::new(int_lit(1)),
        stage: Stage::Runtime,
        arg_first: false,
    });
    assert!(extract(&runtime_app, &[], &prog, &l).is_err());

    let guarded_match = expr(ExprKind::Match(
        Box::new(local("x")),
        vec![IrArm {
            pat: IrPat::Wild,
            binder_types: Vec::new(),
            guard: Some(bool_lit(true)),
            body: int_lit(0),
        }],
    ));
    assert!(extract(&guarded_match, &[], &prog, &l).is_err());

    let local_rec = expr(ExprKind::Block(
        vec![Stmt::Fun {
            name: "loop_fn".into(),
            rec: true,
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            value: int_lit(0),
        }],
        Box::new(int_lit(0)),
    ));
    assert!(extract(&local_rec, &[], &prog, &l).is_err());
}

#[test]
fn extract_handles_match_int_bool_con_and_tuple_patterns() {
    let prog = FakeProgram::default();
    let l = lang();
    let env: Vec<(Rc<str>, PrimHint)> = vec![("x".into(), PrimHint::Int)];
    let ctor = Constructor {
        module: "Prelude".into(),
        ty: "Option".into(),
        name: "Some".into(),
        arity: 1,
        rep: flick_ir::Rep::Tagged,
        dicts: 0,
        fields: Vec::new(),
        type_args: Vec::new(),
    };
    let m = expr(ExprKind::Match(
        Box::new(local("x")),
        vec![
            IrArm {
                pat: IrPat::Lit(Lit::Int(0)),
                binder_types: Vec::new(),
                guard: None,
                body: int_lit(1),
            },
            IrArm {
                pat: IrPat::Bind("n".into()),
                binder_types: Vec::new(),
                guard: None,
                body: expr(ExprKind::If(
                    Box::new(bin(BinOp::Gt, local("n"), int_lit(0))),
                    Box::new(local("n")),
                    Box::new(int_lit(0)),
                )),
            },
        ],
    ));
    let Term {
        kind: TermKind::Match(_, arms),
        ..
    } = extract(&m, &env, &prog, &l).expect("match on Int")
    else {
        panic!("expected match");
    };
    assert_eq!(arms[0].pat, Pat::Int(0));
    assert_eq!(arms[1].pat, Pat::Bind("n".into()));

    let con_m = expr(ExprKind::Match(
        Box::new(local("opt")),
        vec![IrArm {
            pat: IrPat::Con(ctor, vec![IrPat::Bind("v".into())]),
            binder_types: Vec::new(),
            guard: None,
            body: local("v"),
        }],
    ));
    assert!(extract(&con_m, &[], &prog, &l).is_ok());
}
