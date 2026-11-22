//! Negative/decline and budget-boundary unit tests for reviewed native
//! primitives (#805).

use std::rc::Rc;

use flick_ir::{Dict, ExprKind, Foreign, ForeignMode, ItemKind, LangItem, Rep, Shape, Stage};

use super::super::{
    DefTable, DictEnv, DictValue, Env, EvalBudget, EvalConfig, EvalOutcome, NativePrimitive,
    PrimitiveEval, PrimitiveOutcome, ReviewedPrimitives, StopReason, Value, evaluate,
};
use super::primitives::{default_env, js_prims, str_lit};
use super::{app, empty_module, ex, global, int, local, val_item};
use crate::opt::optimize_with_eval;
use crate::{EffectBridge, Known};

#[test]
fn declines_homonyms_alternate_adapters_missing_roles_and_bad_ctor_rep() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);

    // 1. User homonyms (non-builtin modules) decline even with identical names & foreign bindings.
    let homonym = ReviewedPrimitives::for_js(
        &builtins,
        &|_| false,
        &known,
        true,
        EffectBridge::Absent,
        &|_, _| None,
    );
    assert!(homonym.is_empty(), "non-builtin modules must decline");

    // 2. Alternate JS adapter override declines.
    let alt_adapter = ReviewedPrimitives::for_js(
        &builtins,
        &|_| true,
        &known,
        true,
        EffectBridge::Absent,
        &|_, _| Some("./custom-adapter.mjs"),
    );
    assert!(alt_adapter.is_empty(), "alternate JS adapter must decline");

    // 3. Target callback rejection declines.
    let target_declines = ReviewedPrimitives::build(&builtins, &|_| true, &known, &|_, _, _| false);
    assert!(target_declines.is_empty());

    // 4. Missing @lang roles decline affected primitives.
    let no_option: Vec<LangItem> = lang.iter().filter(|i| i.lang != "some").cloned().collect();
    let no_option_known = Known::of(&no_option);
    let prims = js_prims(&builtins, &no_option_known);
    assert_eq!(prims.arity("Int", "parse", &no_option_known), None);
    assert_eq!(prims.arity("String", "length", &no_option_known), Some(1));

    let no_instances: Vec<LangItem> = lang
        .iter()
        .filter(|i| i.lang != "instances")
        .cloned()
        .collect();
    let no_inst_known = Known::of(&no_instances);
    let prims = js_prims(&builtins, &no_inst_known);
    assert_eq!(
        prims.arity("Instances", "text_contains", &no_inst_known),
        None
    );
    assert_eq!(prims.arity("String", "contains", &no_inst_known), Some(2));

    let no_bool: Vec<LangItem> = lang.iter().filter(|i| i.lang != "bool").cloned().collect();
    let no_bool_known = Known::of(&no_bool);
    let prims = js_prims(&builtins, &no_bool_known);
    assert_eq!(prims.arity("String", "starts_with", &no_bool_known), None);
    assert_eq!(prims.arity("String", "length", &no_bool_known), Some(1));

    // 5. Wrong Option constructor representation declines Int.parse.
    let mut bad_prelude = builtins.clone();
    if let ItemKind::Constructor(c) = &mut bad_prelude[0].items[1].kind {
        c.rep = Rep::Unboxed;
    }
    let prims = js_prims(&bad_prelude, &known);
    assert_eq!(prims.arity("Int", "parse", &known), None);
}

#[test]
fn declines_mutated_foreign_declarations_dynamic_dicts_and_unknown_operands() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);

    // 6. Mutating any Foreign field declines that declaration.
    let mutations: Vec<fn(&mut Foreign)> = vec![
        |f| f.source_name = Some("orig".into()),
        |f| f.mode = ForeignMode::Sync,
        |f| f.mode = ForeignMode::Async,
        |f| f.mode = ForeignMode::Effect,
        |f| f.dicts = 1,
        |f| f.arity = 0,
        |f| f.arity += 1,
        |f| f.stages[0] = true,
        |f| f.stages.push(false),
        |f| f.binding.from = None,
        |f| f.binding.from = Some("./evil.mjs".into()),
        |f| f.binding.name.push('X'),
        |f| f.catch = Some(int(0)),
        |f| f.adapter.receiver = true,
        |f| f.adapter.cancel = true,
        |f| f.adapter.optional.push(0),
        |f| f.adapter.variadic = true,
        |f| f.adapter.result.nullable = true,
        |f| f.adapter.result.unit = true,
        |f| f.adapter.callbacks[0] = Some(1),
        |f| f.adapter.copies.args[0] = true,
        |f| f.adapter.copies.result = !f.adapter.copies.result,
        |f| f.adapter.copies.callbacks[0] = Some((vec![], false)),
        |f| f.adapter.shapes[0] = Shape::Layer,
        |f| f.adapter.result_shape = Shape::Layer,
    ];
    for (idx, mutate) in mutations.into_iter().enumerate() {
        let mut mutated = builtins.clone();
        let ItemKind::Foreign(f) = &mut mutated[1].items[0].kind else {
            panic!("Int.parse foreign");
        };
        mutate(f);
        let prims = js_prims(&mutated, &known);
        assert_eq!(
            prims.arity("Int", "parse", &known),
            None,
            "Int.parse mutation {idx} must decline"
        );
    }

    // 7. Dynamic dictionaries and spurious dictionaries decline.
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();
    let dynamic_dict_call = app(
        ex(ExprKind::Dicts(
            Box::new(global("Int", "parse")),
            vec![Dict::Given("d0".into())],
        )),
        str_lit("42"),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &dynamic_dict_call,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Stopped(StopReason::DynamicRead)));

    let spurious_dict = DictValue::Instance {
        module: Rc::from("Instances"),
        name: Rc::from("Show Int"),
        args: Rc::new(Vec::new()),
    };
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = prims.eval(
        "Int",
        "parse",
        &[spurious_dict],
        &[Value::String(Rc::from("42"))],
        &known,
        &mut budget,
    );
    assert!(matches!(out, PrimitiveOutcome::Stop(StopReason::UnknownOp)));

    // 8. Unknown operands decline without folding.
    let unknown_arg = app(global("Int", "parse"), local("x"));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &unknown_arg,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Stopped(StopReason::DynamicRead)));

    // 9. Wrong value type passed directly to eval declines.
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = prims.eval(
        "String",
        "length",
        &[],
        &[Value::Int(1)],
        &known,
        &mut budget,
    );
    assert!(matches!(out, PrimitiveOutcome::Stop(StopReason::UnknownOp)));
}

#[test]
fn budget_exhaustion_preserves_call_and_never_turns_into_none() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();

    // Tight step budget on Int.parse of a long string stops with StepBudget, NEVER Known(None).
    let long_input = " ".repeat(512) + "42";
    let parse_call = app(global("Int", "parse"), str_lit(&long_input));
    let tight_steps = EvalConfig {
        max_trial_steps: 10,
        ..EvalConfig::default()
    };
    let mut budget = EvalBudget::new(&tight_steps, 10_000);
    let out = evaluate(
        &parse_call,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Stopped(StopReason::StepBudget)),
        "budget exhaustion must stay Stopped(StepBudget), never Known(None): {out:?}"
    );

    // Tight byte budget on Int.parse stops with ConstructedBytes, NEVER Known(None).
    let mut tight_bytes = EvalBudget::new(
        &EvalConfig {
            max_constructed_bytes: 4,
            ..EvalConfig::default()
        },
        10_000,
    );
    let out = prims.eval(
        "Int",
        "parse",
        &[],
        &[Value::String(Rc::from("42"))],
        &known,
        &mut tight_bytes,
    );
    assert!(
        matches!(out, PrimitiveOutcome::Stop(StopReason::ConstructedBytes)),
        "byte limit must stay Stop(ConstructedBytes): {out:?}"
    );

    // Tight byte budget on String.slice stops before allocation and preserves call in module pass.
    let mut module = empty_module(
        "App",
        vec![val_item(
            "sliced",
            app(
                app(app(global("String", "slice"), int(0)), int(200)),
                str_lit(&"x".repeat(250)),
            ),
        )],
    );
    let counters = optimize_with_eval(
        &mut module,
        &|_| true,
        &|_, _| None,
        &|_, _| None,
        &known,
        &|m| matches!(m, "Prelude" | "Int" | "String" | "Instances"),
        &EvalConfig {
            max_constructed_bytes: 300,
            ..EvalConfig::default()
        },
        &prims,
    );
    assert_eq!(counters.success, 0, "{counters:?}");
    assert!(counters.budget >= 1, "{counters:?}");
    let ItemKind::Value { body, .. } = &module.items[0].kind else {
        panic!("value item");
    };
    assert!(
        matches!(
            body.kind,
            ExprKind::App {
                stage: Stage::Pure,
                ..
            }
        ),
        "budget-rejected call must remain residual: {body:?}"
    );

    // Verify NativePrimitive metadata helpers directly.
    assert_eq!(NativePrimitive::IntParse.arity(), 1);
    assert_eq!(NativePrimitive::StringSlice.arity(), 3);
    assert_eq!(
        NativePrimitive::StringStartsWith.binding(),
        ("flick:String", "startsWith")
    );
}
