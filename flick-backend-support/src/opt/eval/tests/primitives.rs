//! Unit tests for the reviewed native primitive evaluation boundary (#805).

use std::collections::HashSet;

use flick_ir::{
    Adapter, Binding, Constructor, Copies, Expr, ExprKind, Foreign, ForeignMode, Item, ItemKind,
    LangItem, Lit, Module, Namespace, Rep, ResultAdapter, Shape,
};

use super::super::materialize::{MaterializeContext, MaterializeLimits, materialize};
use super::super::{
    DefTable, DictEnv, Env, EvalBudget, EvalConfig, EvalOutcome, ReviewedPrimitives, StopReason,
    Value, evaluate,
};
use super::{app, bin, empty_module, ex, global, int, lam, local, sp, val_item};
use crate::opt::optimize_with_eval;
use crate::{EffectBridge, Known};

pub(super) fn str_lit(s: &str) -> Expr {
    ex(ExprKind::Lit(Lit::String(s.to_owned())))
}

fn canonical_adapter(arity: usize, result_copies: bool, result_shape: Shape) -> Adapter {
    Adapter {
        receiver: false,
        cancel: false,
        optional: Vec::new(),
        variadic: false,
        result: ResultAdapter {
            nullable: false,
            unit: false,
        },
        callbacks: vec![None; arity],
        copies: Copies {
            args: vec![false; arity],
            result: result_copies,
            callbacks: vec![None; arity],
        },
        shapes: vec![Shape::Data; arity],
        result_shape,
    }
}

fn foreign_item(
    name: &str,
    from: Option<&str>,
    export: &str,
    stages: usize,
    arity: usize,
    result_copies: bool,
    result_shape: Shape,
) -> Item {
    Item {
        kind: ItemKind::Foreign(Foreign {
            name: name.to_owned(),
            source_name: None,
            mode: ForeignMode::Pure,
            binding: Binding {
                from: from.map(str::to_owned),
                name: export.to_owned(),
            },
            stages: vec![false; stages],
            dicts: 0,
            arity,
            adapter: canonical_adapter(arity, result_copies, result_shape),
            catch: None,
            type_params: Vec::new(),
            ty: None,
        }),
        span: sp(0, 1),
    }
}

fn ctor_item(module: &str, ty: &str, name: &str, arity: usize) -> Item {
    Item {
        kind: ItemKind::Constructor(Constructor {
            module: module.to_owned(),
            ty: ty.to_owned(),
            name: name.to_owned(),
            arity,
            rep: Rep::Tagged,
            dicts: 0,
            fields: Vec::new(),
            type_args: Vec::new(),
        }),
        span: sp(0, 1),
    }
}

fn full_lang(
    instances_mod: &str,
    opt_mod: &str,
    opt_ty: &str,
    some: &str,
    none: &str,
) -> Vec<LangItem> {
    [
        ("int", "Prelude", Namespace::Type, "Int"),
        ("string", "Prelude", Namespace::Type, "String"),
        ("bool", "Prelude", Namespace::Type, "Bool"),
        ("option", opt_mod, Namespace::Type, opt_ty),
        ("some", opt_mod, Namespace::Constructor, some),
        ("none", opt_mod, Namespace::Constructor, none),
        ("instances", instances_mod, Namespace::Type, instances_mod),
    ]
    .into_iter()
    .map(|(lang, module, namespace, name)| LangItem {
        lang: lang.into(),
        module: module.into(),
        namespace,
        name: name.into(),
    })
    .collect()
}

fn canonical_builtins(
    instances_mod: &str,
    opt_mod: &str,
    opt_ty: &str,
    some: &str,
    none: &str,
) -> Vec<Module> {
    let prelude = empty_module(
        opt_mod,
        vec![
            ctor_item(opt_mod, opt_ty, none, 0),
            ctor_item(opt_mod, opt_ty, some, 1),
        ],
    );
    let int_mod = empty_module(
        "Int",
        vec![foreign_item(
            "parse",
            Some("flick:rt"),
            "intParse",
            1,
            1,
            true,
            Shape::Option(Box::new(Shape::Data)),
        )],
    );
    let string_mod = empty_module(
        "String",
        vec![
            foreign_item(
                "length",
                Some("flick:String"),
                "length",
                1,
                1,
                false,
                Shape::Data,
            ),
            foreign_item(
                "slice",
                Some("flick:String"),
                "slice",
                3,
                2,
                false,
                Shape::Data,
            ),
            foreign_item(
                "starts_with",
                Some("flick:String"),
                "startsWith",
                2,
                1,
                false,
                Shape::Data,
            ),
            foreign_item(
                "contains",
                Some("flick:String"),
                "includes",
                2,
                1,
                false,
                Shape::Data,
            ),
            foreign_item(
                "ends_with",
                Some("flick:String"),
                "endsWith",
                2,
                1,
                false,
                Shape::Data,
            ),
        ],
    );
    let instances = empty_module(
        instances_mod,
        vec![
            foreign_item(
                "text_starts_with",
                Some("flick:String"),
                "startsWith",
                2,
                1,
                false,
                Shape::Data,
            ),
            foreign_item(
                "text_contains",
                Some("flick:String"),
                "includes",
                2,
                1,
                false,
                Shape::Data,
            ),
        ],
    );
    vec![prelude, int_mod, string_mod, instances]
}

pub(super) fn default_env() -> (Vec<LangItem>, Vec<Module>) {
    (
        full_lang("Instances", "Prelude", "Option", "Some", "None"),
        canonical_builtins("Instances", "Prelude", "Option", "Some", "None"),
    )
}

pub(super) fn js_prims(modules: &[Module], known: &Known<'_>) -> ReviewedPrimitives {
    ReviewedPrimitives::for_js(
        modules,
        &|_| true,
        known,
        true,
        EffectBridge::Absent,
        &|_, _| None,
    )
}

#[test]
fn int_parse_valid_cases_cover_whitespace_bounds_and_negative_zero() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();

    let valid: &[(&str, i32)] = &[
        ("0", 0),
        ("-0", 0),
        ("+0", 0),
        ("36", 36),
        ("+36", 36),
        ("-36", -36),
        ("0000000000000000000000042", 42),
        ("-000000000000000000000000", 0),
        ("-2147483648", i32::MIN),
        ("2147483647", i32::MAX),
        ("+2147483647", i32::MAX),
        ("\u{FEFF}36", 36),
        ("36\u{FEFF}", 36),
        ("\u{FEFF}36\u{00A0}", 36),
        ("\u{00A0}-0\u{FEFF}", 0),
        (" \t\n\u{000B}\u{000C}\r 36 \r\u{000C}\u{000B}\n\t ", 36),
        (
            "\u{1680}\u{2000}\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}36\u{3000}",
            36,
        ),
    ];
    for &(input, want) in valid {
        let expr = app(global("Int", "parse"), str_lit(input));
        let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
        let EvalOutcome::Known(Value::Con { ctor, args, dicts }) = evaluate(
            &expr,
            &defs,
            Env::default(),
            DictEnv::default(),
            &known,
            &prims,
            &mut budget,
        ) else {
            panic!("expected Some({want}) for {input:?}");
        };
        assert_eq!(ctor.module, "Prelude");
        assert_eq!(ctor.ty, "Option");
        assert_eq!(ctor.name, "Some");
        assert!(dicts.is_empty());
        assert_eq!(args.len(), 1);
        assert!(
            matches!(args[0], Value::Int(n) if n == want),
            "{input:?}: {args:?}"
        );
    }
}

#[test]
fn int_parse_invalid_overflow_and_utf16_inputs_return_none() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();

    let invalid: &[&str] = &[
        "",
        "   ",
        "+",
        "-",
        "+-1",
        "--1",
        "3 6",
        "3\t6",
        "3\u{00A0}6",
        "3\u{FEFF}6",
        "+ 36",
        "- 36",
        "\u{0085}36",
        "36\u{0085}",
        "\u{180E}36",
        "\u{200B}36",
        "1.5",
        "0x10",
        "1_000",
        "1e2",
        "ada",
        "-2147483649",
        "2147483648",
        "+2147483648",
        "99999999999999999999",
    ];
    for &input in invalid {
        let expr = app(global("Int", "parse"), str_lit(input));
        let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
        let EvalOutcome::Known(Value::Con { ctor, args, dicts }) = evaluate(
            &expr,
            &defs,
            Env::default(),
            DictEnv::default(),
            &known,
            &prims,
            &mut budget,
        ) else {
            panic!("expected None for {input:?}");
        };
        assert_eq!(ctor.name, "None", "{input:?}");
        assert!(args.is_empty() && dicts.is_empty());
    }

    // Internal UTF-16 values passed to Int.parse: lone surrogate -> None.
    let lone_parse = app(
        global("Int", "parse"),
        app(
            app(app(global("String", "slice"), int(1)), int(2)),
            str_lit("a🌍b"),
        ),
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &lone_parse,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Known(Value::Con { ref ctor, .. }) if ctor.name == "None"));
}

#[test]
fn string_length_and_lone_surrogate_materialization_boundary() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();

    let earth = "a🌍b";
    let slice = |a, b, s| app(app(app(global("String", "slice"), int(a)), int(b)), s);

    let len_expr = app(global("String", "length"), str_lit(earth));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &len_expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(matches!(out, EvalOutcome::Known(Value::Int(4))), "{out:?}");

    // Standalone String.slice 1 2 "a🌍b" evaluates to internal UTF-16 [0xD83C]
    // and declines materialization (NEVER U+FFFD).
    let lone_expr = slice(1, 2, str_lit(earth));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let EvalOutcome::Known(lone_val) = evaluate(
        &lone_expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    ) else {
        panic!("expected internal UTF-16 value");
    };
    assert!(
        matches!(&lone_val, Value::Utf16(u) if u.as_slice() == [0xD83C]),
        "got {lone_val:?}"
    );
    let visible = HashSet::new();
    let mat_ctx = MaterializeContext {
        current_module: "App",
        lang: &known,
        visible_external_ctors: &visible,
    };
    let mat = materialize(
        &lone_val,
        sp(0, 1),
        &mat_ctx,
        MaterializeLimits::from_config(&EvalConfig::default()),
    );
    assert!(
        matches!(mat, Err(StopReason::Unmaterializable)),
        "lone surrogate must decline materialization, got {mat:?}"
    );
}

#[test]
fn string_slice_and_rejoin_fold_in_module_optimizer_while_lone_slice_stays_residual() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let earth = "a🌍b";
    let slice = |a, b, s| app(app(app(global("String", "slice"), int(a)), int(b)), s);
    let lone_expr = slice(1, 2, str_lit(earth));

    let mut module = empty_module(
        "App",
        vec![
            val_item("len_earth", app(global("String", "length"), str_lit(earth))),
            val_item(
                "len_lone",
                app(global("String", "length"), lone_expr.clone()),
            ),
            val_item("standalone_lone", lone_expr),
            val_item("valid_slice", slice(1, 3, str_lit(earth))),
            val_item("neg_slice", slice(-1, 4, str_lit(earth))),
            val_item("min_max_slice", slice(i32::MIN, i32::MAX, str_lit(earth))),
            val_item("empty_oob", slice(9, 12, str_lit(earth))),
            val_item("empty_rev", slice(3, 1, str_lit(earth))),
            val_item(
                "rejoined",
                bin(
                    flick_ir::BinOp::Concat,
                    slice(0, 2, str_lit(earth)),
                    slice(2, 4, str_lit(earth)),
                ),
            ),
            val_item("reslice_valid", slice(0, 1, slice(0, 2, str_lit(earth)))),
        ],
    );
    let counters = optimize_with_eval(
        &mut module,
        &|_| true,
        &|_, _| None,
        &|_, _| None,
        &known,
        &|m| matches!(m, "Prelude" | "Int" | "String" | "Instances"),
        &EvalConfig::default(),
        &prims,
    );
    assert!(counters.success >= 9, "{counters:?}");
    assert!(counters.unmaterializable >= 1, "{counters:?}");

    let body = |name: &str| {
        module
            .items
            .iter()
            .find_map(|i| match &i.kind {
                ItemKind::Value { name: n, body, .. } if n == name => Some(body),
                _ => None,
            })
            .expect("item")
    };
    assert_eq!(body("len_earth").kind, ExprKind::Lit(Lit::Int(4)));
    assert_eq!(body("len_lone").kind, ExprKind::Lit(Lit::Int(1)));
    assert!(
        matches!(body("standalone_lone").kind, ExprKind::App { .. }),
        "standalone lone-surrogate slice must stay residual: {:?}",
        body("standalone_lone")
    );
    assert_eq!(
        body("valid_slice").kind,
        ExprKind::Lit(Lit::String("🌍".into()))
    );
    assert_eq!(
        body("neg_slice").kind,
        ExprKind::Lit(Lit::String("b".into()))
    );
    assert_eq!(
        body("min_max_slice").kind,
        ExprKind::Lit(Lit::String(earth.into()))
    );
    assert_eq!(
        body("empty_oob").kind,
        ExprKind::Lit(Lit::String(String::new()))
    );
    assert_eq!(
        body("empty_rev").kind,
        ExprKind::Lit(Lit::String(String::new()))
    );
    assert_eq!(
        body("rejoined").kind,
        ExprKind::Lit(Lit::String(earth.into()))
    );
    assert_eq!(
        body("reslice_valid").kind,
        ExprKind::Lit(Lit::String("a".into()))
    );
}

#[test]
fn show_arg_folds_with_renamed_instances_and_option_roles() {
    // Also exercise renamed `@lang instances` and renamed `@lang option`/`some`/`none`.
    let lang = full_lang("CustomInst", "CoreOpt", "Maybe", "Just", "Nothing");
    let builtins = canonical_builtins("CustomInst", "CoreOpt", "Maybe", "Just", "Nothing");
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);

    let mut defs = DefTable::default();
    let starts = |p: &str| {
        app(
            app(global("CustomInst", "text_starts_with"), str_lit(p)),
            local("text"),
        )
    };
    let or = |a, b| bin(flick_ir::BinOp::Or, a, b);
    let opens = or(or(or(starts("["), starts("(")), starts("{")), starts("\""));
    let show_arg_body = lam(
        &["text"],
        ex(ExprKind::If(
            Box::new(or(
                opens,
                ex(ExprKind::Not(Box::new(app(
                    app(global("CustomInst", "text_contains"), str_lit(" ")),
                    local("text"),
                )))),
            )),
            Box::new(local("text")),
            Box::new(bin(
                flick_ir::BinOp::Concat,
                bin(flick_ir::BinOp::Concat, str_lit("("), local("text")),
                str_lit(")"),
            )),
        )),
    );
    defs.insert_value("CustomInst", "show_arg", vec![], show_arg_body);

    for (input, want) in [
        ("36", "36"),
        ("a b", "(a b)"),
        ("[1, 2]", "[1, 2]"),
        ("(a b)", "(a b)"),
        ("{ a = 1 }", "{ a = 1 }"),
        ("\"an age is not negative\"", "\"an age is not negative\""),
    ] {
        let expr = app(global("CustomInst", "show_arg"), str_lit(input));
        let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
        let EvalOutcome::Known(Value::String(got)) = evaluate(
            &expr,
            &defs,
            Env::default(),
            DictEnv::default(),
            &known,
            &prims,
            &mut budget,
        ) else {
            panic!("show_arg failed for {input:?}");
        };
        assert_eq!(&*got, want);
    }

    // Renamed Option constructors work for Int.parse.
    let parse_seven = app(global("Int", "parse"), str_lit("7"));
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let EvalOutcome::Known(Value::Con { ctor, .. }) = evaluate(
        &parse_seven,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    ) else {
        panic!("expected Just 7");
    };
    assert_eq!(
        (ctor.module.as_str(), ctor.ty.as_str(), ctor.name.as_str()),
        ("CoreOpt", "Maybe", "Just")
    );
}

#[test]
fn string_predicates_handle_well_formed_and_decline_lone_surrogate_needle() {
    let (lang, builtins) = default_env();
    let known = Known::of(&lang);
    let prims = js_prims(&builtins, &known);
    let defs = DefTable::default();

    // Well-formed needle against lone-surrogate haystack is target-exact.
    let lone_prefix = app(
        app(app(global("String", "slice"), int(0)), int(2)),
        str_lit("a🌍b"),
    );
    let starts_expr = app(
        app(global("String", "starts_with"), str_lit("a")),
        lone_prefix,
    );
    let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
    let out = evaluate(
        &starts_expr,
        &defs,
        Env::default(),
        DictEnv::default(),
        &known,
        &prims,
        &mut budget,
    );
    assert!(
        matches!(out, EvalOutcome::Known(Value::Bool(true))),
        "{out:?}"
    );

    // Lone-surrogate needle declines to avoid JS UTF-16 vs Go WTF-8 divergence.
    let lone_needle = app(
        app(app(global("String", "slice"), int(1)), int(2)),
        str_lit("a🌍b"),
    );
    for op in ["contains", "starts_with", "ends_with"] {
        let expr = app(
            app(global("String", op), lone_needle.clone()),
            str_lit("a🌍b"),
        );
        let mut budget = EvalBudget::new(&EvalConfig::default(), 20_000);
        let out = evaluate(
            &expr,
            &defs,
            Env::default(),
            DictEnv::default(),
            &known,
            &prims,
            &mut budget,
        );
        assert!(
            matches!(out, EvalOutcome::Stopped(StopReason::UnknownOp)),
            "{op} with lone-surrogate needle must decline, got {out:?}"
        );
    }
}
