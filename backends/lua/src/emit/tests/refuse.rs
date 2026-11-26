//! What the emitter refuses, at the Flick source of each: effects,
//! non-pure stages, kernel bindings and the operators whose exact
//! semantics are the kernel's. Nothing is approximated.

use flick_backend::Severity;
use flick_ir::{
    Adapter, Arith, BinOp, Binding, Dict, ExprKind, Foreign, ForeignMode, InstanceInfo, Item,
    ItemKind, Lit, Num, Param, Pat, Stage,
};

use super::{
    app, arm, at, emit, ex, float, global, int, lambda, local, method, refusals, sp, value,
};

fn message_at(found: &[(String, (u32, u32))], text: &str, span: (u32, u32)) {
    assert!(
        found.iter().any(|(m, s)| m.contains(text) && *s == span),
        "no refusal containing {text:?} at {span:?}: {found:#?}"
    );
}

#[test]
fn an_effect_is_refused_where_it_is() {
    let perform = at(
        ExprKind::Perform {
            effect: "Log".into(),
            op: "log".into(),
            stages: vec![Stage::Runtime],
        },
        10,
        20,
    );
    let items = || vec![value("main", lambda(&["_"], perform.clone()))];
    let found = refusals("lua51", items());
    assert_eq!(found.len(), 1, "{found:#?}");
    message_at(
        &found,
        "is not available on the lua/love target yet: effects, services and tasks need the Lua \
         runtime",
        (10, 20),
    );
    let refused = emit("lua51", items()).expect_err("refused");
    let diagnostic = refused[0].clone().diagnostic();
    assert_eq!(diagnostic.code, "P012");
    assert_eq!(diagnostic.severity, Severity::Error);
    let span = diagnostic.span.expect("a span");
    assert_eq!((span.file, span.start, span.end), (0, 10, 20));
}

#[test]
fn a_non_pure_stage_is_refused() {
    let runtime = at(
        ExprKind::Lambda {
            params: vec![Param {
                stage: Stage::Runtime,
                ..super::param("x")
            }],
            body: Box::new(local("x")),
        },
        3,
        9,
    );
    let call = at(
        ExprKind::App {
            func: Box::new(local("f")),
            arg: Box::new(int(1)),
            stage: Stage::Sync,
            arg_first: false,
        },
        11,
        14,
    );
    let found = refusals(
        "lua51",
        vec![value("f", runtime), value("g", lambda(&["f"], call))],
    );
    message_at(&found, "a function of a non-pure stage", (3, 9));
    message_at(&found, "a call of a non-pure stage", (11, 14));
}

#[test]
fn every_refusal_of_a_run_is_reported_in_order() {
    let add = at(
        ExprKind::Binary(
            BinOp::Arith(Arith::Add, Num::Int8),
            Box::new(int(1)),
            Box::new(int(2)),
        ),
        1,
        5,
    );
    let rem = at(
        ExprKind::Binary(
            BinOp::Arith(Arith::Rem, Num::Float),
            Box::new(float(1.0)),
            Box::new(float(2.0)),
        ),
        6,
        9,
    );
    let unknown = at(
        ExprKind::Binary(BinOp::Lt, Box::new(local("p")), Box::new(local("q"))),
        10,
        12,
    );
    let tuple = ex(ExprKind::Tuple(vec![add, rem, unknown]));
    let found = refusals("lua51", vec![value("main", lambda(&["p", "q"], tuple))]);
    let spans: Vec<(u32, u32)> = found.iter().map(|(_, s)| *s).collect();
    assert_eq!(spans, vec![(1, 5), (6, 9), (10, 12)], "{found:#?}");
    message_at(&found, "`+` of integers", (1, 5));
    message_at(&found, "`%` of floats", (6, 9));
    message_at(&found, "the operator `<`", (10, 12));
    assert!(
        found
            .iter()
            .all(|(m, _)| m.contains("the Lua kernel does not compute it exactly yet"))
    );
}

#[test]
fn a_kernel_binding_is_refused_at_its_use() {
    let foreign = Item {
        kind: ItemKind::Foreign(Foreign {
            name: "now".into(),
            source_name: None,
            mode: ForeignMode::Sync,
            binding: Binding::Module {
                from: "flick:Clock".into(),
                name: "now".into(),
            },
            stages: vec![false],
            dicts: 0,
            arity: 0,
            adapter: Adapter::default(),
            catch: None,
            type_params: vec![],
            ty: None,
        }),
        span: sp(40, 50),
    };
    let use_ = at(
        ExprKind::Global {
            module: "App".into(),
            name: "now".into(),
            type_args: vec![],
        },
        30,
        33,
    );
    let found = refusals("lua51", vec![value("main", use_), foreign]);
    message_at(
        &found,
        "`App.now` is not available on the lua/love target yet: the Lua kernel does not \
         implement it",
        (30, 33),
    );
    message_at(&found, "the foreign function `App.now`", (40, 50));
}

#[test]
fn a_negative_zero_literal_keeps_its_sign_only_on_luajit() {
    let zero = || vec![value("z", at(ExprKind::Lit(Lit::Float(-0.0)), 2, 6))];
    let found = refusals("lua51", zero());
    message_at(
        &found,
        "the literal `-0.0` is not available on the lua/love target with the dialect `lua51` yet",
        (2, 6),
    );
    let printed = emit("luajit", zero()).expect("lowered on LuaJIT");
    assert_eq!(printed.code, "M.z = -0\n");
}

#[test]
fn wide_literals_and_float_patterns_are_the_kernels() {
    let wide = at(ExprKind::Lit(Lit::Int64(1)), 1, 3);
    let pattern = at(
        ExprKind::Match(
            Box::new(local("x")),
            vec![
                arm(Pat::Lit(Lit::Float(1.5)), int(1)),
                arm(Pat::Wild, int(2)),
            ],
        ),
        5,
        9,
    );
    let found = refusals(
        "lua51",
        vec![value("w", wide), value("p", lambda(&["x"], pattern))],
    );
    message_at(&found, "a 64-bit or big integer literal", (1, 3));
    message_at(&found, "a float literal pattern", (5, 9));
    // `**` of `Float` has its own special cases
    let pow = at(
        ExprKind::Binary(
            BinOp::Arith(Arith::Pow, Num::Float),
            Box::new(float(1.0)),
            Box::new(local("y")),
        ),
        4,
        8,
    );
    let found = refusals("lua51", vec![value("e", lambda(&["y"], pow))]);
    message_at(&found, "`**` of floats", (4, 8));
}

#[test]
fn a_derived_instance_without_methods_needs_the_runtime() {
    let info = InstanceInfo {
        module: "App".into(),
        name: "Eq Foo".into(),
        class: ("Prelude".into(), "Eq".into()),
        span: (0, 0),
        context: vec![],
        supers: vec![],
        derived: Some((vec![], None)),
        defaults: vec![],
        generic: None,
        enumerates: None,
    };
    let item = Item {
        kind: ItemKind::Instance {
            info: Box::new(info),
            methods: vec![],
        },
        span: sp(7, 19),
    };
    let found = refusals("lua51", vec![item]);
    message_at(&found, "the derived instance `Eq Foo`", (7, 19));
}

#[test]
fn malformed_ir_is_refused_not_emitted() {
    let missing = method(Dict::Given("Show a".into()), "show");
    let exit = ex(ExprKind::LoopExit {
        id: "l".into(),
        value: None,
    });
    let generator = at(
        ExprKind::Generate {
            body: Box::new(int(1)),
            strict: true,
            sync: false,
            limit: None,
        },
        3,
        4,
    );
    let found = refusals(
        "lua51",
        vec![
            value("m", app(missing, int(1))),
            value("x", exit),
            value("g", generator),
            value("q", global("App", "m")),
        ],
    );
    assert!(
        found
            .iter()
            .any(|(m, _)| m.contains("no dictionary `Show a`"))
    );
    assert!(found.iter().any(|(m, _)| m.contains("a loop exit outside")));
    message_at(&found, "a generator", (3, 4));
    assert!(found.iter().all(|(m, _)| !m.is_empty()));
    assert_eq!(found.len(), 3, "{found:#?}");
}
