//! Budget, growth, retry, primitive-hook, capture-avoidance, and inner-fold tests (#803).

use std::collections::HashSet;
use std::rc::Rc;

use flick_ir::{Arith, BinOp, Dict, ExprKind, Item, ItemKind, Lit, Num, Stage, Stmt};

use super::super::materialize::{
    MaterializeContext, MaterializeLimits, check_profitability, expr_cost, materialize,
};
use super::super::{
    DefTable, DictEnv, DictValue, Env, EvalBudget, EvalConfig, EvalOutcome, NoPrimitives,
    PrimitiveEval, PrimitiveOutcome, StopReason, Value, evaluate,
};
use super::{app, bin, empty_module, ex, ex_at, global, int, lam, local, sp, val_item};
use crate::Known;
use crate::opt::{Generic, optimize_with_eval};

struct StubPrim;

impl PrimitiveEval for StubPrim {
    fn arity(&self, module: &str, name: &str, _lang: &Known<'_>) -> Option<usize> {
        (module == "String" && name == "utf16_len").then_some(1)
    }

    fn eval<'a>(
        &self,
        _module: &str,
        _name: &str,
        _dicts: &[DictValue<'a>],
        args: &[Value<'a>],
        _lang: &Known<'_>,
        budget: &mut EvalBudget,
    ) -> PrimitiveOutcome<'a> {
        if budget.charge_steps(1).is_err() {
            return PrimitiveOutcome::Stop(StopReason::StepBudget);
        }
        match &args[0] {
            Value::String(s) => PrimitiveOutcome::Known(Value::Int(
                i32::try_from(s.encode_utf16().count()).unwrap_or(0),
            )),
            Value::Utf16(u) => {
                PrimitiveOutcome::Known(Value::Int(i32::try_from(u.len()).unwrap_or(0)))
            }
            _ => PrimitiveOutcome::Stop(StopReason::UnknownOp),
        }
    }
}

#[test]
fn deep_recursion_and_deep_terms_hit_deterministic_frame_depth_without_stack_overflow() {
    let mut defs = DefTable::default();
    defs.insert_value(
        "App",
        "deep",
        vec![],
        lam(
            &["n"],
            ex(ExprKind::If(
                Box::new(bin(BinOp::Eq, local("n"), int(0))),
                Box::new(int(0)),
                Box::new(bin(
                    BinOp::Arith(Arith::Add, Num::Int),
                    int(1),
                    app(
                        global("App", "deep"),
                        bin(BinOp::Arith(Arith::Sub, Num::Int), local("n"), int(1)),
                    ),
                )),
            )),
        ),
    );
    let known = Known::of(&[]);
    let config = EvalConfig::default();
    let call_deep = app(global("App", "deep"), int(500));
    let mut budget = EvalBudget::new(&config, config.max_module_steps);
    let out = evaluate(
        &call_deep,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::FrameDepth)),
        "expected deterministic FrameDepth, got {out:?}"
    );

    let mut nested = int(1);
    for _ in 0..200 {
        nested = bin(BinOp::Arith(Arith::Add, Num::Int), int(1), nested);
    }
    let mut budget = EvalBudget::new(&config, config.max_module_steps);
    let out = evaluate(
        &nested,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::FrameDepth)),
        "expected deterministic FrameDepth on deep AST, got {out:?}"
    );
}

#[test]
fn tiny_call_producing_large_data_declines_on_bytes_and_growth() {
    let mut defs = DefTable::default();
    defs.insert_value(
        "App",
        "grow",
        vec![],
        lam(
            &["n", "s"],
            ex(ExprKind::If(
                Box::new(bin(BinOp::Le, local("n"), int(0))),
                Box::new(local("s")),
                Box::new(app(
                    app(
                        global("App", "grow"),
                        bin(BinOp::Arith(Arith::Sub, Num::Int), local("n"), int(1)),
                    ),
                    bin(BinOp::Concat, local("s"), local("s")),
                )),
            )),
        ),
    );
    let known = Known::of(&[]);
    let config = EvalConfig::default();
    let huge = app(
        app(global("App", "grow"), int(14)),
        ex(ExprKind::Lit(Lit::String("x".into()))),
    );
    let mut budget = EvalBudget::new(&config, config.max_module_steps);
    let out = evaluate(
        &huge,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::ConstructedBytes)),
        "expected ConstructedBytes, got {out:?}"
    );

    let medium = app(
        app(global("App", "grow"), int(9)),
        ex(ExprKind::Lit(Lit::String("x".into()))),
    );
    let mut budget = EvalBudget::new(&config, config.max_module_steps);
    let EvalOutcome::Known(val) = evaluate(
        &medium,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("expected 512-byte string to evaluate");
    };
    let visible = HashSet::new();
    let mat_ctx = MaterializeContext {
        current_module: "App",
        lang: &known,
        visible_external_ctors: &visible,
    };
    let mat = materialize(
        &val,
        sp(0, 1),
        &mat_ctx,
        MaterializeLimits::from_config(&config),
    )
    .expect("512 bytes <= max_materialized_bytes");
    assert_eq!(
        check_profitability(expr_cost(&medium), &mat, 0, &config),
        Err(StopReason::ResidualGrowth)
    );
}

#[test]
fn primitive_hook_consumes_unmaterializable_utf16_internally() {
    let known = Known::of(&[]);
    let config = EvalConfig::default();
    let visible = HashSet::new();
    let mat_ctx = MaterializeContext {
        current_module: "App",
        lang: &known,
        visible_external_ctors: &visible,
    };
    let lone = Value::Utf16(Rc::new(vec![0xD800_u16]));
    assert!(matches!(
        materialize(
            &lone,
            sp(0, 1),
            &mat_ctx,
            MaterializeLimits::from_config(&config),
        ),
        Err(StopReason::Unmaterializable)
    ));
    let call_prim = app(global("String", "utf16_len"), local("lone"));
    let env = Env::default().with(Rc::from("lone"), lone);
    let defs = DefTable::default();
    let mut budget = EvalBudget::new(&config, config.max_module_steps);
    let out = evaluate(
        &call_prim,
        &defs,
        env,
        DictEnv::default(),
        &known,
        &StubPrim,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Known(Value::Int(1))), "{out:?}");
}

#[test]
fn outer_refusal_keeps_independent_inner_fold() {
    // Recursive `sum_to` is not inlined by `inline::inline`, so `eval` folds `sum_to 6 -> 21`
    // while keeping the outer opaque call and preserving `inner_call`'s exact span.
    let sum_to_body = lam(
        &["n"],
        ex(ExprKind::If(
            Box::new(bin(BinOp::Le, local("n"), int(0))),
            Box::new(int(0)),
            Box::new(bin(
                BinOp::Arith(Arith::Add, Num::Int),
                local("n"),
                app(
                    global("App", "sum_to"),
                    bin(BinOp::Arith(Arith::Sub, Num::Int), local("n"), int(1)),
                ),
            )),
        )),
    );
    let inner_call = ex_at(
        ExprKind::App {
            func: Box::new(global("App", "sum_to")),
            arg: Box::new(int(6)),
            stage: Stage::Pure,
            arg_first: false,
        },
        20,
        30,
    );
    let outer_call = ex_at(
        ExprKind::App {
            func: Box::new(global("Ext", "opaque")),
            arg: Box::new(inner_call),
            stage: Stage::Pure,
            arg_first: false,
        },
        10,
        31,
    );
    let mut module = empty_module(
        "App",
        vec![val_item("sum_to", sum_to_body), val_item("res", outer_call)],
    );
    let known = Known::of(&[]);
    let counters = optimize_with_eval(
        &mut module,
        &|name| name == "res",
        &|_, _| None,
        &|_, _| None,
        &known,
        &|_| false,
        &EvalConfig::default(),
        &NoPrimitives,
    );
    let res = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "res" => Some(body),
            _ => None,
        })
        .expect("res");
    let ExprKind::App { func, arg, .. } = &res.kind else {
        panic!("outer opaque call must stay: {res:?}");
    };
    assert!(
        matches!(&func.kind, ExprKind::Global { module, name, .. } if module == "Ext" && name == "opaque")
    );
    assert_eq!(arg.kind, ExprKind::Lit(Lit::Int(21)));
    assert_eq!(arg.span, sp(20, 30));
    assert!(counters.success >= 1);
    assert!(counters.unknown_op >= 1);
}

#[test]
fn resolved_dictionary_consumed_internally_without_materializing() {
    let known = Known::of(&[]);
    let mut defs = DefTable::default();
    defs.insert_value(
        "App",
        "use_dict",
        vec!["d".into()],
        lam(
            &["x"],
            app(
                ex(ExprKind::Method {
                    dict: Dict::Given("d".into()),
                    name: "inc".into(),
                    type_args: Vec::new(),
                }),
                local("x"),
            ),
        ),
    );
    defs.insert_method(
        "App",
        "Inc Int",
        "inc",
        vec![],
        lam(
            &["n"],
            bin(BinOp::Arith(Arith::Add, Num::Int), local("n"), int(1)),
        ),
    );
    let dict_call = app(
        ex(ExprKind::Dicts(
            Box::new(global("App", "use_dict")),
            vec![Dict::Instance {
                module: "App".into(),
                name: "Inc Int".into(),
                args: vec![],
            }],
        )),
        int(41),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &dict_call,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Known(Value::Int(42))), "{out:?}");
}

#[test]
fn capture_avoidance_and_shadowing_in_nested_blocks_and_rec_groups() {
    let known = Known::of(&[]);
    let rec_block = ex(ExprKind::Block(
        vec![
            Stmt::Fun {
                name: "even".into(),
                rec: true,
                type_params: Vec::new(),
                dicts: vec![],
                dict_constraints: Vec::new(),
                ty: None,
                value: lam(
                    &["n"],
                    ex(ExprKind::If(
                        Box::new(bin(BinOp::Eq, local("n"), int(0))),
                        Box::new(ex(ExprKind::Lit(Lit::Bool(true)))),
                        Box::new(app(
                            local("odd"),
                            bin(BinOp::Arith(Arith::Sub, Num::Int), local("n"), int(1)),
                        )),
                    )),
                ),
            },
            Stmt::Fun {
                name: "odd".into(),
                rec: true,
                type_params: Vec::new(),
                dicts: vec![],
                dict_constraints: Vec::new(),
                ty: None,
                value: lam(
                    &["n"],
                    ex(ExprKind::If(
                        Box::new(bin(BinOp::Eq, local("n"), int(0))),
                        Box::new(ex(ExprKind::Lit(Lit::Bool(false)))),
                        Box::new(app(
                            local("even"),
                            bin(BinOp::Arith(Arith::Sub, Num::Int), local("n"), int(1)),
                        )),
                    )),
                ),
            },
        ],
        Box::new(app(local("even"), int(6))),
    ));

    // Direct evaluator check with an outer shadowed binding `odd = 99` in Env.
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let shadowed_env = Env::default().with(Rc::from("odd"), Value::Int(99));
    let defs = DefTable::default();
    let out = evaluate(
        &rec_block,
        &defs,
        shadowed_env,
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Known(Value::Bool(true))),
        "{out:?}"
    );

    // Full optimizer pass: mutually recursive local group folds without capturing outer locals.
    let mut module = empty_module("App", vec![val_item("res", rec_block)]);
    let counters = optimize_with_eval(
        &mut module,
        &|name| name == "res",
        &|_, _| None,
        &|_, _| None,
        &known,
        &|_| false,
        &EvalConfig::default(),
        &NoPrimitives,
    );
    assert!(counters.success >= 1, "expected fold: {counters:?}");
    let res = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "res" => Some(body),
            _ => None,
        })
        .expect("res");
    assert_eq!(res.kind, ExprKind::Lit(Lit::Bool(true)));
}

#[test]
fn repeated_failing_trials_hit_retry_cap_and_zero_budget_preserves_ir() {
    let spin_body = lam(
        &["n"],
        app(
            global("App", "spin"),
            bin(BinOp::Arith(Arith::Add, Num::Int), local("n"), int(1)),
        ),
    );
    let call_spin = ex_at(
        ExprKind::App {
            func: Box::new(global("App", "spin")),
            arg: Box::new(int(0)),
            stage: Stage::Pure,
            arg_first: false,
        },
        5,
        15,
    );
    let original_module = empty_module(
        "App",
        vec![
            val_item("spin", spin_body),
            val_item("looping", call_spin.clone()),
        ],
    );
    let known = Known::of(&[]);

    // Default budget: stops on StepBudget in round 1, skips via RetryCap in round 2.
    let mut m1 = original_module.clone();
    let c1 = optimize_with_eval(
        &mut m1,
        &|name| name == "looping",
        &|_, _| None,
        &|_, _| None,
        &known,
        &|_| false,
        &EvalConfig {
            max_trial_steps: 50,
            ..EvalConfig::default()
        },
        &NoPrimitives,
    );
    assert_eq!(c1.budget, 1, "should fail once in round 1: {c1:?}");
    assert_eq!(
        c1.retries_skipped, 1,
        "should skip unchanged failing call in round 2: {c1:?}"
    );
    assert_eq!(m1.items[1].kind, original_module.items[1].kind);

    // Forced zero budget changes neither diagnostics nor program IR.
    let mut m2 = original_module.clone();
    let c2 = optimize_with_eval(
        &mut m2,
        &|name| name == "looping",
        &|_, _| None,
        &|_, _| None,
        &known,
        &|_| false,
        &EvalConfig {
            max_module_steps: 0,
            ..EvalConfig::default()
        },
        &NoPrimitives,
    );
    assert_eq!(c2.success, 0);
    assert_eq!(m2.items[1].kind, original_module.items[1].kind);
}

#[test]
fn external_polymorphic_preload_is_deterministic_at_cap() {
    use crate::opt::devirt;

    // Reference 300 external instances in reversed insertion order alongside a
    // polymorphic `Dict::Given` method call: `DefTable::build` must always load
    // the exact same lexicographically first `MAX_EXTERNAL_DEFS` (256) entries.
    let poly_fn = Item {
        kind: ItemKind::Value {
            name: "poly".into(),
            type_params: Vec::new(),
            dicts: vec!["d".into()],
            dict_constraints: Vec::new(),
            ty: None,
            body: lam(
                &["x"],
                app(
                    ex(ExprKind::Method {
                        dict: Dict::Given("d".into()),
                        name: "m".into(),
                        type_args: Vec::new(),
                    }),
                    local("x"),
                ),
            ),
        },
        span: sp(0, 1),
    };
    let mut items = vec![poly_fn];
    for idx in (0..300_usize).rev() {
        let inst = format!("Inst{idx:03}");
        let call = app(
            ex(ExprKind::Dicts(
                Box::new(global("App", "poly")),
                vec![Dict::Instance {
                    module: "Ext".into(),
                    name: inst,
                    args: vec![],
                }],
            )),
            int(0),
        );
        items.push(val_item(&format!("use_{idx:03}"), call));
    }
    let module = empty_module("App", items);
    let known = Known::of(&[]);
    let generics = |owner: &str, key: &str| -> Option<Generic> {
        (owner == "Ext" && key.ends_with("#m")).then(|| Generic {
            type_params: Vec::new(),
            dicts: vec![],
            dict_constraints: Vec::new(),
            body: lam(&["n"], local("n")),
        })
    };
    let lookup = devirt::Lookup::new(&module, &generics, &known);

    let first = DefTable::build(&module, &lookup, &generics, &known);
    let second = DefTable::build(&module, &lookup, &generics, &known);
    for idx in 0..300_usize {
        let inst = format!("Inst{idx:03}");
        let loaded_1 = first.method("Ext", &inst, "m").is_some();
        let loaded_2 = second.method("Ext", &inst, "m").is_some();
        assert_eq!(loaded_1, loaded_2, "determinism mismatch at {inst}");
        assert_eq!(loaded_1, idx < 256, "unexpected preload boundary at {inst}");
    }
}
