//! Regression tests for budget accounting, bounded preloading, and
//! charge-before-allocation invariants in the bounded IR evaluator (#803).

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use flick_ir::{BinOp, Dict, Expr, ExprKind, Item, ItemKind, LangItem, Lit, Namespace, Pat, Stmt};

use super::super::defs::MAX_EXTERNAL_PROBES;
use super::super::materialize::{MaterializeContext, MaterializeLimits, materialize};
use super::super::{
    DefTable, DictEnv, Env, EvalBudget, EvalConfig, EvalCounters, EvalOutcome, NoPrimitives,
    RetryTracker, StopReason, Value, evaluate, run_module_pass,
};
use super::{app, bin, empty_module, ex, global, int, lam, local, sp, val_item};
use crate::Known;
use crate::opt::devirt;

#[test]
fn failing_let_pattern_match_commits_module_step_budget() {
    // Nested 8-level tuple pattern: matching consumes > 6 steps and hits `ModuleBudget`.
    let mut pat = Pat::Bind("leaf".into());
    let mut rhs = int(1);
    for _ in 0..8 {
        pat = Pat::Tuple(vec![pat, Pat::Wild]);
        rhs = ex(ExprKind::Tuple(vec![rhs, int(0)]));
    }
    let stmts = vec![Stmt::Let(pat.clone(), rhs.clone()), Stmt::Let(pat, rhs)];
    let block = ex(ExprKind::Block(
        stmts,
        Box::new(app(global("App", "id"), int(7))),
    ));
    let mut module = empty_module(
        "App",
        vec![
            val_item("id", lam(&["x"], local("x"))),
            val_item("res", block),
        ],
    );
    let known = Known::of(&[]);
    let generics = |_: &str, _: &str| None;
    let lookup = devirt::Lookup::new(&module, &generics, &known);
    let config = EvalConfig {
        max_module_steps: 25,
        ..EvalConfig::default()
    };
    let mut module_steps_remaining = 6_u32;
    let mut retries = RetryTracker::default();
    let mut counters = EvalCounters::default();
    let changed = run_module_pass(
        &mut module,
        &|name| name == "res",
        &lookup,
        &generics,
        &known,
        &NoPrimitives,
        &config,
        &mut module_steps_remaining,
        &mut retries,
        &mut counters,
    );
    assert!(!changed, "exhausted budget must not fold trailing call");
    assert_eq!(
        module_steps_remaining, 0,
        "failing pattern match must commit consumed module steps"
    );
}

#[test]
fn def_table_caps_unavailable_polymorphic_cartesian_probes() {
    // 400 external instances × 400 polymorphic methods = 160,000 combinations,
    // none of which exist in `generics`: probes must stop at `MAX_EXTERNAL_PROBES`.
    let mut items = Vec::with_capacity(800);
    for m_idx in 0..400_usize {
        let method_name = format!("m{m_idx:03}");
        items.push(Item {
            kind: ItemKind::Value {
                name: format!("poly_{m_idx:03}"),
                dicts: vec!["d".into()],
                body: lam(
                    &["x"],
                    app(
                        ex(ExprKind::Method {
                            dict: Dict::Given("d".into()),
                            name: method_name,
                        }),
                        local("x"),
                    ),
                ),
            },
            span: sp(0, 1),
        });
    }
    for i_idx in (0..400_usize).rev() {
        let inst = format!("Inst{i_idx:03}");
        let call = app(
            ex(ExprKind::Dicts(
                Box::new(global("App", "poly_000")),
                vec![Dict::Instance {
                    module: "Ext".into(),
                    name: inst,
                    args: vec![],
                }],
            )),
            int(0),
        );
        items.push(val_item(&format!("use_{i_idx:03}"), call));
    }
    let module = empty_module("App", items);
    let known = Known::of(&[]);
    let probes = Cell::new(0_usize);
    let generics = |owner: &str, _key: &str| -> Option<(Vec<String>, Expr)> {
        if owner == "Ext" {
            probes.set(probes.get() + 1);
        }
        None
    };
    let lookup = devirt::Lookup::new(&module, &generics, &known);
    let _table = DefTable::build(&module, &lookup, &generics, &known);
    assert!(probes.get() > 0, "expected polymorphic probes to run");
    assert!(
        probes.get() <= MAX_EXTERNAL_PROBES,
        "probes {} exceeded cap {}",
        probes.get(),
        MAX_EXTERNAL_PROBES
    );
}

#[test]
fn eval_for_env_charges_constructed_bytes_on_literal_fast_path() {
    let make_module = || {
        let block = ex(ExprKind::Block(
            vec![Stmt::Let(
                Pat::Bind("s".into()),
                ex(ExprKind::Lit(Lit::String("0123456789".into()))),
            )],
            Box::new(bin(
                BinOp::Eq,
                local("s"),
                ex(ExprKind::Lit(Lit::String("0123456789".into()))),
            )),
        ));
        empty_module("App", vec![val_item("res", block)])
    };
    let known = Known::of(&[]);
    let generics = |_: &str, _: &str| None;

    for limit in [0_u32, 4_u32] {
        let mut module = make_module();
        let lookup = devirt::Lookup::new(&module, &generics, &known);
        let config = EvalConfig {
            max_constructed_bytes: limit,
            ..EvalConfig::default()
        };
        let mut steps = config.max_module_steps;
        let mut retries = RetryTracker::default();
        let mut counters = EvalCounters::default();
        let changed = run_module_pass(
            &mut module,
            &|name| name == "res",
            &lookup,
            &generics,
            &known,
            &NoPrimitives,
            &config,
            &mut steps,
            &mut retries,
            &mut counters,
        );
        assert!(
            !changed,
            "10-byte literal must not bypass max_constructed_bytes={limit}"
        );
    }

    let mut module = make_module();
    let lookup = devirt::Lookup::new(&module, &generics, &known);
    let config = EvalConfig {
        max_constructed_bytes: 64,
        ..EvalConfig::default()
    };
    let mut steps = config.max_module_steps;
    let mut retries = RetryTracker::default();
    let mut counters = EvalCounters::default();
    let changed = run_module_pass(
        &mut module,
        &|name| name == "res",
        &lookup,
        &generics,
        &known,
        &NoPrimitives,
        &config,
        &mut steps,
        &mut retries,
        &mut counters,
    );
    assert!(changed, "sufficient byte budget should fold block");
    let ItemKind::Value { body, .. } = &module.items[0].kind else {
        panic!("expected value item");
    };
    assert_eq!(body.kind, ExprKind::Lit(Lit::Bool(true)));
}

fn lang_item(lang: &str, module: &str, name: &str) -> LangItem {
    LangItem {
        lang: lang.into(),
        module: module.into(),
        namespace: Namespace::Value,
        name: name.into(),
    }
}

#[test]
fn show_string_charges_escaped_bytes_before_allocating() {
    let lang_items = [
        lang_item("instances", "Instances", ""),
        lang_item("show_method", "Instances", "show"),
        lang_item("display_method", "Instances", "display"),
    ];
    let known = Known::of(&lang_items);
    let defs = DefTable::default();
    // 20 control characters: 20 input bytes -> 2 + 20 * 6 = 122 escaped bytes.
    let ctrl: Rc<str> = Rc::from("\u{0001}".repeat(20));
    let env = Env::default().with(Rc::from("s"), Value::String(ctrl));

    let show_call = app(
        ex(ExprKind::Method {
            dict: Dict::Instance {
                module: "Instances".into(),
                name: "Show String".into(),
                args: vec![],
            },
            name: "show".into(),
        }),
        local("s"),
    );

    // Setup charges 8 + 9 + 11 (dict) + 32 + 4 (method) = 64 bytes.
    // With 120 bytes total, 56 bytes remain — enough for 20 raw bytes, but NOT 122 escaped bytes.
    let tight = EvalConfig {
        max_constructed_bytes: 120,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&tight, tight.max_module_steps);
    let out = evaluate(
        &show_call,
        &defs,
        env.clone(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::ConstructedBytes)),
        "expected ConstructedBytes before allocating escaped string, got {out:?}"
    );
    assert_eq!(
        budget.bytes_remaining, 56,
        "bytes_remaining must not be partially debited on rejection"
    );

    // With 200 bytes total (64 + 122 = 186 <= 200), succeeds with exact byte accounting.
    let room = EvalConfig {
        max_constructed_bytes: 200,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&room, room.max_module_steps);
    let EvalOutcome::Known(Value::String(rendered)) = evaluate(
        &show_call,
        &defs,
        env.clone(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("expected escaped string result");
    };
    assert_eq!(rendered.len(), 122);
    assert_eq!(budget.bytes_remaining, 14);

    // `Display String` reuses the existing `Rc<str>` without charging extra string bytes.
    let display_call = app(
        ex(ExprKind::Method {
            dict: Dict::Instance {
                module: "Instances".into(),
                name: "Display String".into(),
                args: vec![],
            },
            name: "display".into(),
        }),
        local("s"),
    );
    let disp_cfg = EvalConfig {
        // 8 + 9 + 14 (dict) + 32 + 7 (method) = 70 bytes; 0 bytes for returning `s`.
        max_constructed_bytes: 70,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&disp_cfg, disp_cfg.max_module_steps);
    let EvalOutcome::Known(Value::String(shown)) = evaluate(
        &display_call,
        &defs,
        env,
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    ) else {
        panic!("expected Display String to succeed without reallocating");
    };
    assert_eq!(shown.len(), 20);
    assert_eq!(budget.bytes_remaining, 0);
}

#[test]
fn rec_fun_group_charges_steps_and_bytes_before_allocating() {
    let stmts: Vec<Stmt> = (0..100_usize)
        .map(|i| Stmt::Fun {
            name: format!("f{i:02}"),
            rec: true,
            dicts: vec![],
            value: lam(&["x"], local("x")),
        })
        .collect();
    let block = ex(ExprKind::Block(stmts, Box::new(int(0))));
    let defs = DefTable::default();
    let known = Known::of(&[]);

    // Tight step budget must fail per-member during scan.
    let step_cfg = EvalConfig {
        max_trial_steps: 25,
        max_constructed_bytes: 100_000,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&step_cfg, step_cfg.max_module_steps);
    let out = evaluate(
        &block,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::StepBudget)),
        "expected StepBudget while scanning rec group, got {out:?}"
    );

    // Tight byte budget must fail before allocating the group.
    let byte_cfg = EvalConfig {
        max_trial_steps: 2_000,
        max_constructed_bytes: 100,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&byte_cfg, byte_cfg.max_module_steps);
    let out = evaluate(
        &block,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::ConstructedBytes)),
        "expected ConstructedBytes before allocating rec group, got {out:?}"
    );
}

#[test]
fn step_dicts_and_resolve_dict_charge_constructed_bytes() {
    let lang_items = [
        lang_item("numeric", "Numeric", "Numeric"),
        lang_item("int", "Numeric", "Int"),
        lang_item("add", "Numeric", "add"),
    ];
    let known = Known::of(&lang_items);
    let defs = DefTable::default();

    let super_add = app(
        app(
            ex(ExprKind::Method {
                dict: Dict::Super {
                    dict: Box::new(Dict::Instance {
                        module: "Numeric".into(),
                        name: "Integer Int".into(),
                        args: vec![],
                    }),
                    class: "Numeric.Numeric".into(),
                },
                name: "add".into(),
            }),
            int(2),
        ),
        int(3),
    );

    let tight = EvalConfig {
        max_constructed_bytes: 20,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&tight, tight.max_module_steps);
    let out = evaluate(
        &super_add,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::ConstructedBytes)),
        "expected ConstructedBytes when resolving dict, got {out:?}"
    );

    let ok_cfg = EvalConfig::default();
    let mut budget = EvalBudget::new(&ok_cfg, ok_cfg.max_module_steps);
    let out = evaluate(
        &super_add,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Known(Value::Int(5))),
        "expected Super(Integer Int, Numeric).add 2 3 -> 5, got {out:?}"
    );

    // `ExprKind::Dicts` vector allocation charges before resolving dictionaries.
    let dict_app = ex(ExprKind::Dicts(
        Box::new(global("App", "f")),
        vec![
            Dict::Instance {
                module: "Ext".into(),
                name: "A".into(),
                args: vec![],
            },
            Dict::Instance {
                module: "Ext".into(),
                name: "B".into(),
                args: vec![],
            },
        ],
    ));
    let zero_bytes = EvalConfig {
        max_constructed_bytes: 8,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&zero_bytes, zero_bytes.max_module_steps);
    let out = evaluate(
        &dict_app,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &NoPrimitives,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::ConstructedBytes)),
        "expected ConstructedBytes on step_dicts allocation, got {out:?}"
    );
}

#[test]
fn materialize_utf16_checks_limits_before_allocating_and_preserves_surrogate_error() {
    let known = Known::of(&[]);
    let visible = HashSet::new();
    let ctx = MaterializeContext {
        current_module: "App",
        lang: &known,
        visible_external_ctors: &visible,
    };

    // Lone surrogate always reports `Unmaterializable` even under zero limits.
    let lone = Value::Utf16(Rc::from([0xD800_u16]));
    assert_eq!(
        materialize(
            &lone,
            sp(0, 1),
            &ctx,
            MaterializeLimits {
                max_nodes: 0,
                max_bytes: 0,
                max_depth: 16,
            },
        )
        .err(),
        Some(StopReason::Unmaterializable)
    );

    // Surrogate pair U+1F600 ("😀") decodes to 4 UTF-8 bytes.
    let emoji = Value::Utf16(Rc::from([0xD83D_u16, 0xDE00_u16]));
    assert_eq!(
        materialize(
            &emoji,
            sp(0, 1),
            &ctx,
            MaterializeLimits {
                max_nodes: 4,
                max_bytes: 3,
                max_depth: 16,
            },
        )
        .err(),
        Some(StopReason::ResidualGrowth)
    );
    assert_eq!(
        materialize(
            &emoji,
            sp(0, 1),
            &ctx,
            MaterializeLimits {
                max_nodes: 0,
                max_bytes: 100,
                max_depth: 16,
            },
        )
        .err(),
        Some(StopReason::ResidualGrowth)
    );
    let mat = materialize(
        &emoji,
        sp(2, 5),
        &ctx,
        MaterializeLimits {
            max_nodes: 1,
            max_bytes: 4,
            max_depth: 16,
        },
    )
    .expect("exact 4-byte budget should materialize emoji");
    assert_eq!(mat.nodes, 1);
    assert_eq!(mat.bytes, 4);
    assert_eq!(mat.expr.span, sp(2, 5));
    assert_eq!(mat.expr.kind, ExprKind::Lit(Lit::String("😀".into())));
}
