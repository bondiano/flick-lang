//! What the Lua kernel computes: `Int` arithmetic, `==` of structured
//! values, the order and `++` of strings and `++` of lists are calls of
//! `K`, as is each implemented binding of `kernel.txt`, with every argument
//! at once.

use flick_ir::{
    Adapter, Arith, BinOp, Binding, Foreign, ForeignMode, IrType, Item, ItemKind, Num, Stage,
    TypeRef,
};

use super::{app, binary, global, int, lambda, local, lua, sp, string, typed, value};
use crate::emit::kernel_sources;
use crate::kernel::table;

#[test]
fn the_operators_the_kernel_owns_are_its_calls() {
    let op = |op, l, r| lua(vec![value("v", lambda(&["a", "b"], binary(op, l, r)))]);
    let (a, b) = (|| local("a"), || local("b"));
    for (arith, name) in [
        (Arith::Add, "add32"),
        (Arith::Sub, "sub32"),
        (Arith::Mul, "mul32"),
        (Arith::Div, "div32"),
        (Arith::Rem, "rem32"),
        (Arith::Pow, "pow32"),
    ] {
        let code = op(BinOp::Arith(arith, Num::Int), a(), b());
        assert!(code.contains(&format!("return K.{name}(a, b)")), "{code}");
    }
    let code = op(BinOp::Eq, a(), b());
    assert!(code.contains("return K.equals(a, b)"), "{code}");
    let code = op(BinOp::NotEq, typed(a(), "Float"), b());
    assert!(code.contains("return not K.equals(a, b)"), "{code}");
    let code = op(BinOp::Concat, a(), string("!"));
    assert!(code.contains("return K.concat(a, \"!\")"), "{code}");
    let code = op(BinOp::ListConcat, a(), b());
    assert!(code.contains("return K.list_concat(a, b)"), "{code}");
    let code = op(BinOp::Lt, typed(a(), "String"), b());
    assert!(
        code.contains("return K.compare_strings(a, b) < 0"),
        "{code}"
    );
    // what Lua computes exactly stays Lua's
    let code = op(BinOp::Lt, typed(a(), "Int"), int(1));
    assert!(code.contains("return a < 1"), "{code}");
}

fn named(name: &str) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: name.into(),
        args: vec![],
    })
}

/// `foreign pure get from "flick:rt" "listGet"`, `arity 1` as JS curries it.
fn list_get() -> Item {
    let arrow = |param, result| {
        TypeRef::new(IrType::Fun {
            param,
            stage: Stage::Pure,
            result,
        })
    };
    let ty = arrow(named("Int"), arrow(named("List"), named("Option")));
    Item {
        kind: ItemKind::Foreign(Foreign {
            name: "get".into(),
            source_name: None,
            mode: ForeignMode::Pure,
            binding: Binding::Module {
                from: "flick:rt".into(),
                name: "listGet".into(),
            },
            stages: vec![false; 2],
            dicts: 0,
            arity: 1,
            adapter: Adapter::default(),
            catch: None,
            type_params: vec![],
            ty: Some(ty),
        }),
        span: sp(0, 1),
    }
}

#[test]
fn an_implemented_binding_takes_every_argument_at_once() {
    let call = app(app(global("App", "get"), int(0)), local("xs"));
    let code = lua(vec![list_get(), value("first", lambda(&["xs"], call))]);
    assert!(code.contains("K.listGet(0, xs)"), "{code}");
    // as a value, a curried function of its arguments
    let code = lua(vec![list_get(), value("at", global("App", "get"))]);
    assert!(code.contains("K.listGet("), "{code}");
}

/// An effect test is the kernel's `testEffect` of its name alone: the runner
/// reports it skipped, so its task, here an order the kernel does not
/// compute, is neither generated nor refused.
#[test]
fn an_effect_test_is_its_name_alone() {
    let mut item = list_get();
    if let ItemKind::Foreign(f) = &mut item.kind {
        f.name = "effect_test".into();
        f.binding = Binding::Module {
            from: "flick:rt".into(),
            name: "testEffect".into(),
        };
        f.arity = 2;
    }
    let task = lambda(&["a", "b"], binary(BinOp::Lt, local("a"), local("b")));
    let call = app(app(global("App", "effect_test"), string("x")), task);
    let code = lua(vec![item, value("t", call)]);
    assert!(code.contains("K.testEffect(\"x\")"), "{code}");
    assert!(!code.contains(" < "), "{code}");
}

/// `foreign pure get from "flick:rt" "recordGet"`, of the constraint
/// `HasField l r a`.
fn record_get() -> Item {
    let mut item = list_get();
    if let ItemKind::Foreign(f) = &mut item.kind {
        f.name = "field".into();
        f.binding = Binding::Module {
            from: "flick:rt".into(),
            name: "recordGet".into(),
        };
        f.dicts = 1;
    }
    item
}

/// The instance of a class for a record or a tuple is the kernel's, over
/// the instances of its fields.
#[test]
fn a_structural_instance_is_the_kernels_over_its_fields() {
    let dict = flick_ir::Dict::Structural {
        class: "Show".into(),
        fields: vec![
            flick_ir::Dict::Given("Show a".into()),
            flick_ir::Dict::Given("Show b".into()),
        ],
    };
    let shown = flick_ir::Expr::new(
        flick_ir::ExprKind::Method {
            dict,
            name: "show".into(),
            type_args: vec![],
        },
        sp(0, 1),
    );
    let body = flick_ir::Expr::new(
        flick_ir::ExprKind::Abstract(vec!["Show a".into(), "Show b".into()], Box::new(shown)),
        sp(0, 1),
    );
    let code = lua(vec![value("show_pair", body)]);
    assert!(code.contains("K.structural(\"Show\", {"), "{code}");
    assert!(code.contains("}).show"), "{code}");
}

#[test]
fn a_kernel_function_takes_its_dictionaries_first() {
    let dict = flick_ir::Dict::Given("HasField".into());
    let call = app(
        app(
            flick_ir::Expr::new(
                flick_ir::ExprKind::Dicts(Box::new(global("App", "field")), vec![dict.clone()]),
                sp(0, 1),
            ),
            local("label"),
        ),
        local("r"),
    );
    let body = flick_ir::Expr::new(
        flick_ir::ExprKind::Abstract(vec!["HasField".into()], Box::new(call)),
        sp(0, 1),
    );
    let code = lua(vec![
        record_get(),
        value("read", lambda(&["label", "r"], body)),
    ]);
    assert!(code.contains("K.recordGet("), "{code}");
    assert!(code.contains(", label, r)"), "{code}");
    // as a value, a function of the dictionaries, then of each argument
    let code = lua(vec![record_get(), value("f", global("App", "field"))]);
    assert!(code.contains("M.field = function(dict)"), "{code}");
}

#[test]
fn the_kernel_defines_each_binding_its_table_implements() {
    let sources: Vec<&str> = kernel_sources().collect();
    let defined = |name: &str| {
        sources.iter().any(|s| match name.split_once('.') {
            // `K.Math = { floor = math.floor, … }`
            Some((root, leaf)) => s.lines().any(|l| {
                l.starts_with(&format!("K.{root} = {{")) && l.contains(&format!(" {leaf} = "))
            }),
            None => {
                s.contains(&format!("function K.{name}("))
                    || s.lines().any(|l| l.starts_with(&format!("K.{name} =")))
            }
        })
    };
    let implemented = table().implemented();
    assert!(implemented.len() >= 17, "{implemented:?}");
    for name in implemented {
        assert!(
            defined(name),
            "kernel.txt implements `{name}`, which no kernel chunk defines"
        );
    }
}
