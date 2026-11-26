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

#[test]
fn the_kernel_defines_each_binding_its_table_implements() {
    let sources: Vec<&str> = kernel_sources().collect();
    let defined = |name: &str| {
        sources.iter().any(|s| {
            s.contains(&format!("function K.{name}("))
                || s.lines().any(|l| l.starts_with(&format!("K.{name} =")))
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
