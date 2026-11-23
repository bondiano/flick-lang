//! Unit tests for the bounded concrete evaluator over portable Flick IR (#803).

mod budgets;
mod limits;
mod primitives;
mod primitives_decline;

use std::collections::{BTreeMap, HashSet};

use flick_ir::{
    Arith, Arm, BinOp, Constructor, Exports, Expr, ExprKind, FileId, Initialization, Item,
    ItemKind, LangItem, Lit, Module, Namespace, Num, Param, Pat, Rep, Span, Stage, Stmt,
};

use super::materialize::{MaterializeContext, MaterializeLimits, materialize};
use super::{
    DefTable, DictEnv, Env, EvalBudget, EvalConfig, EvalOutcome, NoPrimitives, StopReason, Value,
    evaluate,
};
use crate::Known;
use crate::opt::optimize_with_eval;

pub(super) fn sp(start: u32, end: u32) -> Span {
    Span::new(FileId::new(0), start, end)
}

pub(super) fn ex(kind: ExprKind) -> Expr {
    Expr::new(kind, sp(0, 1))
}

pub(super) fn ex_at(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr::new(kind, sp(start, end))
}

pub(super) fn int(n: i32) -> Expr {
    ex(ExprKind::Lit(Lit::Int(n)))
}

pub(super) fn bigint(s: &str) -> Expr {
    ex(ExprKind::Lit(Lit::BigInt(s.to_owned())))
}

pub(super) fn local(name: &str) -> Expr {
    ex(ExprKind::Local(name.to_owned()))
}

pub(super) fn global(module: &str, name: &str) -> Expr {
    ex(ExprKind::Global {
        module: module.to_owned(),
        name: name.to_owned(),
        type_args: Vec::new(),
    })
}

pub(super) fn app(func: Expr, arg: Expr) -> Expr {
    ex(ExprKind::App {
        func: Box::new(func),
        arg: Box::new(arg),
        stage: Stage::Pure,
        arg_first: false,
    })
}

pub(super) fn pipe(arg: Expr, func: Expr) -> Expr {
    ex(ExprKind::App {
        func: Box::new(func),
        arg: Box::new(arg),
        stage: Stage::Pure,
        arg_first: true,
    })
}

pub(super) fn lam(names: &[&str], body: Expr) -> Expr {
    ex(ExprKind::Lambda {
        params: names
            .iter()
            .map(|n| Param {
                pat: if *n == "_" {
                    Pat::Wild
                } else {
                    Pat::Bind((*n).to_owned())
                },
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            })
            .collect(),
        body: Box::new(body),
    })
}

pub(super) fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    ex(ExprKind::Binary(op, Box::new(l), Box::new(r)))
}

pub(super) fn ctor(module: &str, ty: &str, name: &str, arity: usize) -> Constructor {
    Constructor {
        module: module.to_owned(),
        ty: ty.to_owned(),
        name: name.to_owned(),
        arity,
        rep: Rep::Tagged,
        dicts: 0,
        fields: Vec::new(),
        type_args: Vec::new(),
    }
}

pub(super) fn empty_module(name: &str, items: Vec<Item>) -> Module {
    Module {
        name: name.to_owned(),
        types: Vec::new(),
        items,
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    }
}

pub(super) fn val_item(name: &str, body: Expr) -> Item {
    Item {
        kind: ItemKind::Value {
            name: name.to_owned(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body,
        },
        span: sp(0, 1),
    }
}

fn option_lang() -> Vec<LangItem> {
    [
        ("some", "Option", "Some"),
        ("none", "Option", "None"),
        ("ok", "Result", "Ok"),
        ("err", "Result", "Err"),
    ]
    .into_iter()
    .map(|(lang, module, name)| LangItem {
        lang: lang.into(),
        module: module.into(),
        namespace: Namespace::Value,
        name: name.into(),
    })
    .collect()
}

fn build_formula_module() -> Module {
    let input_c = ctor("Sample", "Formula", "Input", 0);
    let number_c = ctor("Sample", "Formula", "Number", 1);
    let add_c = ctor("Sample", "Formula", "Add", 2);
    let times_c = ctor("Sample", "Formula", "Times", 2);
    let eval_rec = |a, b, op| {
        bin(
            BinOp::Arith(op, Num::Int),
            app(app(global("Sample", "eval"), local(a)), local("x")),
            app(app(global("Sample", "eval"), local(b)), local("x")),
        )
    };
    let eval_body = lam(
        &["formula", "x"],
        ex(ExprKind::Match(
            Box::new(local("formula")),
            vec![
                Arm {
                    pat: Pat::Con(input_c.clone(), vec![]),
                    binder_types: Vec::new(),
                    guard: None,
                    body: local("x"),
                },
                Arm {
                    pat: Pat::Con(number_c.clone(), vec![Pat::Bind("n".into())]),
                    binder_types: Vec::new(),
                    guard: None,
                    body: local("n"),
                },
                Arm {
                    pat: Pat::Con(
                        add_c.clone(),
                        vec![Pat::Bind("a".into()), Pat::Bind("b".into())],
                    ),
                    binder_types: Vec::new(),
                    guard: None,
                    body: eval_rec("a", "b", Arith::Add),
                },
                Arm {
                    pat: Pat::Con(
                        times_c.clone(),
                        vec![Pat::Bind("a".into()), Pat::Bind("b".into())],
                    ),
                    binder_types: Vec::new(),
                    guard: None,
                    body: eval_rec("a", "b", Arith::Mul),
                },
            ],
        )),
    );
    let num = |n| app(ex(ExprKind::Con(number_c.clone())), int(n));
    let tree = app(
        app(
            ex(ExprKind::Con(add_c.clone())),
            app(
                app(ex(ExprKind::Con(times_c)), ex(ExprKind::Con(input_c))),
                num(3),
            ),
        ),
        app(app(ex(ExprKind::Con(add_c)), num(2)), num(5)),
    );
    let price_body = lam(&["x"], app(app(global("Sample", "eval"), tree), local("x")));
    let total_body = ex_at(
        ExprKind::App {
            func: Box::new(global("Sample", "price")),
            arg: Box::new(int(10)),
            stage: Stage::Pure,
            arg_first: false,
        },
        10,
        20,
    );
    empty_module(
        "Sample",
        vec![
            val_item("eval", eval_body),
            val_item("price", price_body),
            val_item("total", total_body),
        ],
    )
}

#[test]
fn formula_price_10_evaluates_to_37() {
    let mut module = build_formula_module();
    let known = Known::of(&[]);
    let counters = optimize_with_eval(
        &mut module,
        &|name| name == "price" || name == "total",
        &|_, _| None,
        &|_, _| None,
        &known,
        &|_| false,
        &EvalConfig::default(),
        &NoPrimitives,
    );
    assert!(counters.success >= 1, "expected fold: {counters:?}");
    let total = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "total" => Some(body),
            _ => None,
        })
        .expect("total item");
    assert_eq!(total.kind, ExprKind::Lit(Lit::Int(37)));
    assert_eq!(total.span, sp(10, 20), "call-site span must be preserved");
}

#[test]
fn two_closures_of_same_lambda_keep_distinct_captures() {
    let mut defs = DefTable::default();
    defs.insert_value(
        "App",
        "make_adder",
        vec![],
        lam(
            &["n"],
            lam(
                &["x"],
                bin(BinOp::Arith(Arith::Add, Num::Int), local("n"), local("x")),
            ),
        ),
    );
    let expr = ex(ExprKind::Block(
        vec![
            Stmt::let_bind(
                Pat::Bind("f".into()),
                app(global("App", "make_adder"), int(10)),
            ),
            Stmt::let_bind(
                Pat::Bind("g".into()),
                app(global("App", "make_adder"), int(20)),
            ),
        ],
        Box::new(ex(ExprKind::Tuple(vec![
            app(local("f"), int(1)),
            app(local("g"), int(1)),
        ]))),
    ));
    let known = Known::of(&[]);
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let EvalOutcome::Known(Value::Tuple(items)) = evaluate(
        &expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("expected tuple of distinct answers");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], Value::Int(11)));
    assert!(matches!(items[1], Value::Int(21)));
}

#[test]
fn staged_trap_stops_at_stage_one_before_next_arg_and_respects_arg_first() {
    let mut defs = DefTable::default();
    defs.insert_value(
        "App",
        "staged",
        vec![],
        lam(
            &["x"],
            ex(ExprKind::Block(
                vec![Stmt::let_bind(
                    Pat::Bind("checked".into()),
                    bin(
                        BinOp::Arith(Arith::Pow, Num::BigInt),
                        bigint("2"),
                        local("x"),
                    ),
                )],
                Box::new(lam(
                    &["y"],
                    bin(
                        BinOp::Arith(Arith::Add, Num::BigInt),
                        local("checked"),
                        local("y"),
                    ),
                )),
            )),
        ),
    );
    let known = Known::of(&[]);

    // Ordinary application: `staged (-1n) (opaque ())` stops with PossibleTrap at stage 1
    // BEFORE evaluating `opaque ()` (which would stop with UnknownOp).
    let ordinary = app(
        app(global("App", "staged"), bigint("-1")),
        app(global("Ext", "opaque"), ex(ExprKind::Lit(Lit::Unit))),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &ordinary,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::PossibleTrap)),
        "expected stage-1 PossibleTrap before arg 2, got {out:?}"
    );

    // Argument-first pipe: `opaque () |> staged (-1n)` evaluates `opaque ()` FIRST -> UnknownOp.
    let piped = pipe(
        app(global("Ext", "opaque"), ex(ExprKind::Lit(Lit::Unit))),
        app(global("App", "staged"), bigint("-1")),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &piped,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::UnknownOp)),
        "expected arg-first UnknownOp before stage 1, got {out:?}"
    );

    // Valid staged call: `5n |> staged 3n` -> 2^3 + 5 = 13n.
    let valid_pipe = pipe(bigint("5"), app(global("App", "staged"), bigint("3")));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &valid_pipe,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Known(Value::BigInt(13))),
        "{out:?}"
    );
}

#[test]
fn discarded_throwing_arg_is_preserved_and_dead_branch_unsupported_is_skipped() {
    let mut defs = DefTable::default();
    defs.insert_value("App", "const_42", vec![], lam(&["_"], int(42)));
    let known = Known::of(&[]);

    let throwing = app(
        global("App", "const_42"),
        bin(
            BinOp::Arith(Arith::Pow, Num::BigInt),
            bigint("2"),
            bigint("-1"),
        ),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &throwing,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(matches!(
        out,
        EvalOutcome::Stopped(StopReason::PossibleTrap)
    ));

    let dead_branch = ex(ExprKind::If(
        Box::new(ex(ExprKind::Lit(Lit::Bool(true)))),
        Box::new(int(7)),
        Box::new(app(global("Ext", "unsupported"), int(0))),
    ));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &dead_branch,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Known(Value::Int(7))));
}

#[test]
fn known_none_and_err_are_valid_results_distinct_from_stopped() {
    let table = option_lang();
    let known = Known::of(&table);
    let none_c = ctor("Option", "Option", "None", 0);
    let err_c = ctor("Result", "Result", "Err", 1);

    let mut defs = DefTable::default();
    let none_expr = ex(ExprKind::Con(none_c.clone()));
    let err_expr = app(
        ex(ExprKind::Con(err_c)),
        ex(ExprKind::Lit(Lit::String("bad".into()))),
    );

    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let EvalOutcome::Known(val_none) = evaluate(
        &none_expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("None must be Known");
    };
    let visible = HashSet::new();
    let mat_ctx = MaterializeContext {
        current_module: "App",
        lang: &known,
        visible_external_ctors: &visible,
    };
    let limits = MaterializeLimits::from_config(&EvalConfig::default());
    let mat_none = materialize(&val_none, sp(1, 2), &mat_ctx, limits).expect("None materializes");
    assert_eq!(mat_none.expr.kind, ExprKind::Con(none_c));

    let EvalOutcome::Known(val_err) = evaluate(
        &err_expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("Err must be Known");
    };
    let mat_err = materialize(&val_err, sp(3, 4), &mat_ctx, limits).expect("Err materializes");
    assert!(matches!(mat_err.expr.kind, ExprKind::App { .. }));

    defs.insert_value(
        "App",
        "early",
        vec![],
        lam(
            &["_"],
            ex(ExprKind::Block(
                vec![Stmt::let_bind(
                    Pat::Bind("x".into()),
                    ex(ExprKind::Unwrap(Box::new(none_expr))),
                )],
                Box::new(local("x")),
            )),
        ),
    );
    let call_early = app(global("App", "early"), ex(ExprKind::Lit(Lit::Unit)));
    let out_early = evaluate(
        &call_early,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out_early, EvalOutcome::Known(Value::Con { ref ctor, .. }) if ctor.name == "None"),
        "{out_early:?}"
    );
}
