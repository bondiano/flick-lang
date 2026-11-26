//! Calls of host globals: a string, a number or a callback passed, a
//! string taken back or nothing; the rest of the value ABI refused.

use flick_ir::{
    Adapter, Binding, ExprKind, Foreign, ForeignMode, IrType, Item, ItemKind, Stage, TypeRef,
};

use super::{app, at, global, lambda, local, lua, refusals, sp, string, value};

fn named(name: &str) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: name.into(),
        args: vec![],
    })
}

fn arrow(param: TypeRef, result: TypeRef) -> TypeRef {
    TypeRef::new(IrType::Fun {
        param,
        stage: Stage::Pure,
        result,
    })
}

/// `foreign pure name from global "path"` of type `ty` and `arity`.
fn host(name: &str, path: &str, ty: TypeRef, arity: usize, start: u32) -> Item {
    Item {
        kind: ItemKind::Foreign(Foreign {
            name: name.into(),
            source_name: None,
            mode: ForeignMode::Pure,
            binding: Binding::Global { name: path.into() },
            stages: vec![false; arity.max(1)],
            dicts: 0,
            arity,
            adapter: Adapter::default(),
            catch: None,
            type_params: vec![],
            ty: Some(ty),
        }),
        span: sp(start, start + 1),
    }
}

fn say() -> Item {
    host("say", "print", arrow(named("String"), named("Unit")), 1, 0)
}

fn gsub() -> Item {
    let s = || named("String");
    let ty = arrow(s(), arrow(s(), arrow(arrow(s(), s()), s())));
    host("gsub", "string.gsub", ty, 3, 2)
}

#[test]
fn a_host_function_is_called_with_its_arguments_and_one_result() {
    let replaced = app(
        app(app(global("App", "gsub"), string("abc")), string("%w")),
        lambda(&["c"], local("c")),
    );
    let items = vec![
        say(),
        gsub(),
        value("shown", app(global("App", "say"), string("hi"))),
        value("doubled", app(global("App", "say"), replaced)),
        // a local named as the host global is renamed, the global read
        value(
            "echo",
            lambda(&["print"], app(global("App", "say"), local("print"))),
        ),
    ];
    // `string.gsub` gives the string and a count: `print` takes the string
    let expected = "\
M.say = function(x)
  print(x)
  return true
end
M.gsub = function(x)
  return function(x_2)
    return function(x_3)
      return (string.gsub(x, x_2, x_3))
    end
  end
end
do
  print(\"hi\")
  M.shown = true
end
do
  print((string.gsub(\"abc\", \"%w\", function(c)
    return c
  end)))
  M.doubled = true
end
M.echo = function(print_2)
  print(print_2)
  return true
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn what_crosses_beyond_scalars_needs_the_value_abi() {
    let option = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![named("String")],
    });
    let mut sync = say();
    if let ItemKind::Foreign(f) = &mut sync.kind {
        f.mode = ForeignMode::Sync;
        f.name = "log".into();
    }
    let use_ = |name: &str, start| at(global("App", name).kind, start, start + 3);
    let items = vec![
        host("find", "string.find", arrow(named("String"), option), 1, 10),
        host(
            "clock",
            "os.clock",
            arrow(named("Unit"), named("Int")),
            0,
            20,
        ),
        sync,
        value("a", app(use_("find", 30), string("x"))),
        value(
            "b",
            app(
                use_("clock", 40),
                at(ExprKind::Lit(flick_ir::Lit::Unit), 0, 0),
            ),
        ),
        value("c", app(use_("log", 50), string("x"))),
    ];
    let mut pi = host("pi", "math.pi", named("Float"), 0, 60);
    if let ItemKind::Foreign(f) = &mut pi.kind {
        f.stages = vec![];
    }
    let mut items = items;
    items.push(pi);
    let found = refusals("lua51", items);
    assert!(
        found
            .iter()
            .any(|(m, s)| m.contains("a foreign value, not a function") && *s == (60, 61)),
        "{found:#?}"
    );
    let at_use = |start| found.iter().find(|(_, s)| *s == (start, start + 3));
    let message = |start| at_use(start).map_or("", |(m, _)| m.as_str());
    assert!(message(30).contains("a result other than"), "{found:#?}");
    assert!(message(40).contains("a result other than"), "{found:#?}");
    assert!(message(50).contains("that is not `pure`"), "{found:#?}");
}
