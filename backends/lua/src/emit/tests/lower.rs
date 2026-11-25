//! Golden Lua of the pure core. A tag is spelled by the shared helper; the
//! text around it is what Lua's rules and the documented representation
//! give.

use flick_ir::{BinOp, Dict, ExprKind, InstanceInfo, Item, ItemKind, Pat, Stmt};

use super::{
    app, arm, at, binary, bind, block, boolean, con, emit, ex, float, global, if_, int, lambda,
    let_, local, lua, match_, method, none, read, some, sp, string, tag_of, typed, unit, value,
    value_at,
};

/// `text` with each `<Name>` the tag of that constructor.
fn tagged(text: &str) -> String {
    let shape = |name: &str, arity| con("App", "Shape", name, arity);
    text.replace("<Some>", &tag_of(&some()))
        .replace("<None>", &tag_of(&none()))
        .replace("<Circle>", &tag_of(&shape("Circle", 1)))
        .replace("<Square>", &tag_of(&shape("Square", 1)))
        .replace("<Point>", &tag_of(&shape("Point", 0)))
}

fn con_expr(c: flick_ir::Constructor) -> flick_ir::Expr {
    ex(ExprKind::Con(c))
}

/// `match o with | Some x -> x | None -> fallback`.
fn option_match(o: &str, fallback: flick_ir::Expr) -> flick_ir::Expr {
    match_(
        local(o),
        vec![
            arm(Pat::Con(some(), vec![bind("x")]), local("x")),
            arm(Pat::Con(none(), vec![]), fallback),
        ],
    )
}

#[test]
fn values_and_curried_functions() {
    let items = vec![
        value("answer", int(42)),
        value("id", lambda(&["x"], local("x"))),
        value("apply", lambda(&["f", "x"], app(local("f"), local("x")))),
    ];
    // a call of a Flick function is not adjusted: `return f(x)` is a tail call
    let expected = "\
M.answer = 42
M.id = function(x)
  return x
end
M.apply = function(f)
  return function(x)
    return f(x)
  end
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn each_line_maps_to_the_node_it_starts_with() {
    let body = at(ExprKind::Local("x".into()), 5, 6);
    let items = vec![value_at("id", lambda(&["x"], body), 0, 10)];
    let printed = emit("lua51", items).expect("lowered");
    assert_eq!(printed.code, "M.id = function(x)\n  return x\nend\n");
    assert_eq!(printed.lines, vec![(0, sp(0, 10)), (1, sp(5, 6))]);
}

#[test]
fn branches_deliver_their_values_where_they_go() {
    let body = block(
        vec![let_("y", if_(local("b"), int(1), int(2)))],
        if_(local("b"), local("y"), int(3)),
    );
    let mutable = block(
        vec![
            Stmt::var("acc".into(), None, int(0)),
            Stmt::Assign("acc".into(), int(1)),
        ],
        read("acc"),
    );
    let items = vec![
        value("pick", lambda(&["b"], body)),
        value("count", lambda(&["_"], mutable)),
    ];
    let expected = "\
M.pick = function(b)
  local y
  if b then
    y = 1
  else
    y = 2
  end
  if b then
    return y
  else
    return 3
  end
end
M.count = function(_)
  local acc = 0
  acc = 1
  return acc
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn a_match_tests_each_tag_once_and_returns_from_its_arms() {
    let items = vec![value(
        "getOr",
        lambda(&["d", "o"], option_match("o", local("d"))),
    )];
    let expected = tagged(
        r#"M.getOr = function(d)
  return function(o)
    if o._tag == "<Some>" then
      local x = o[1]
      return x
    end
    if o._tag == "<None>" then
      return d
    end
  end
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn a_match_for_its_value_is_a_block_its_arms_leave() {
    let body = block(vec![let_("n", option_match("o", int(0)))], local("n"));
    let items = vec![value("size", lambda(&["o"], body))];
    let expected = tagged(
        r#"M.size = function(o)
  local n
  repeat
    if o._tag == "<Some>" then
      local x = o[1]
      n = x
      break
    end
    if o._tag == "<None>" then
      n = 0
      break
    end
  until true
  return n
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn alternatives_that_bind_names_are_tried_in_order() {
    let shape = |name: &str, arity| con("App", "Shape", name, arity);
    let either = Pat::Or(vec![
        Pat::Con(shape("Circle", 1), vec![bind("r")]),
        Pat::Con(shape("Square", 1), vec![bind("r")]),
    ]);
    let arms = vec![
        arm(either, local("r")),
        arm(Pat::Con(shape("Point", 0), vec![]), float(0.0)),
    ];
    let items = vec![value("size", lambda(&["s"], match_(local("s"), arms)))];
    // the graph would write the first arm twice: each arm is tried in turn
    let expected = tagged(
        r#"M.size = function(s)
  local r
  local m = false
  if s._tag == "<Circle>" then
    r = s[1]
    m = true
  end
  if not m then
    if s._tag == "<Square>" then
      r = s[1]
      m = true
    end
  end
  if m then
    return r
  end
  if s._tag == "<Point>" then
    return 0
  end
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn a_guard_runs_once_its_pattern_has_bound_its_names() {
    let guarded = flick_ir::Arm {
        guard: Some(local("x")),
        ..arm(bind("x"), int(1))
    };
    let body = match_(local("b"), vec![guarded, arm(Pat::Wild, int(2))]);
    let items = vec![value("g", lambda(&["b"], body))];
    let expected = "\
M.g = function(b)
  local x = b
  if x then
    return 1
  end
  return 2
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn the_rest_of_a_list_is_a_copy() {
    let arms = vec![
        arm(
            Pat::List(vec![bind("x")], Some(Box::new(bind("rest")))),
            local("rest"),
        ),
        arm(Pat::List(vec![], None), local("xs")),
    ];
    let items = vec![value("tail", lambda(&["xs"], match_(local("xs"), arms)))];
    let expected = "\
M.tail = function(xs)
  if #xs >= 1 then
    local x = xs[1]
    local rest = {}
    for i = 2, #xs do
      rest[i - 1] = xs[i]
    end
    return rest
  end
  if #xs == 0 then
    return xs
  end
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn data_is_built_in_tables() {
    let record = ex(ExprKind::Record(vec![
        ("x".into(), local("a")),
        ("y".into(), local("b")),
    ]));
    let list = ex(ExprKind::List(vec![local("a"), local("b")]));
    let tuple = ex(ExprKind::Tuple(vec![local("a"), list, record]));
    let update = ex(ExprKind::Update(
        Box::new(local("p")),
        vec![("x".into(), float(1.5))],
    ));
    let field = ex(ExprKind::Field(Box::new(local("p")), "x".into()));
    let items = vec![
        value("mk", lambda(&["a", "b"], tuple)),
        value("move", lambda(&["p"], update)),
        value("getX", lambda(&["p"], field)),
    ];
    let expected = "\
M.mk = function(a)
  return function(b)
    return {a, {a, b}, {x = a, y = b}}
  end
end
M.move = function(p)
  local r = {}
  for k, x in pairs(p) do
    r[k] = x
  end
  r.x = 1.5
  return r
end
M.getX = function(p)
  return p.x
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn constructors_build_their_values() {
    let circle = con("App", "Shape", "Circle", 1);
    let point = con("App", "Shape", "Point", 0);
    let red = flick_ir::Constructor {
        rep: flick_ir::Rep::Literal(flick_ir::EnumLit::String("red".into())),
        ..con("App", "Color", "Red", 0)
    };
    let wrap = flick_ir::Constructor {
        rep: flick_ir::Rep::Unboxed,
        ..con("App", "Wrap", "Wrap", 1)
    };
    let items = vec![
        value("c", app(con_expr(circle.clone()), float(2.0))),
        value("circle", con_expr(circle.clone())),
        value("origin", con_expr(point)),
        value("red", con_expr(red)),
        value("w", app(con_expr(wrap), int(5))),
        Item {
            kind: ItemKind::Constructor(circle),
            span: sp(0, 0),
        },
    ];
    let expected = tagged(
        r#"M.c = {_tag = "<Circle>", 2}
M.circle = function(f)
  return {_tag = "<Circle>", f}
end
M.origin = {_tag = "<Point>"}
M.red = "red"
M.w = 5
M.Circle = function(f)
  return {_tag = "<Circle>", f}
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn unwrap_returns_a_none_from_the_function() {
    let unwrap = ex(ExprKind::Unwrap(Box::new(local("o"))));
    let items = vec![value("first", lambda(&["o"], unwrap))];
    let expected = tagged(
        r#"M.first = function(o)
  if o._tag == "<None>" then
    return o
  end
  return o[1]
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn loops_run_while_their_condition_holds() {
    let spin = ex(ExprKind::Loop {
        condition: Box::new(read("b")),
        body: Box::new(block(
            vec![Stmt::Assign("b".into(), boolean(false))],
            unit(),
        )),
    });
    let spin = block(
        vec![Stmt::var("b".into(), None, boolean(true)), Stmt::Expr(spin)],
        unit(),
    );
    // `loop { break 7 }`: the shared lowering's flags, no `break` across blocks
    let seven = ex(ExprKind::ControlLoop {
        id: "l".into(),
        condition: None,
        body: Box::new(ex(ExprKind::LoopExit {
            id: "l".into(),
            value: Some(Box::new(int(7))),
        })),
        iterable: false,
    });
    let items = vec![
        value("spin", lambda(&["_"], spin)),
        value("seven", lambda(&["_"], seven)),
    ];
    let expected = "\
M.spin = function(_)
  local b = true
  while b do
    b = false
  end
  return true
end
M.seven = function(_)
  local l = true
  local l_result = true
  local l_step = true
  while l do
    l_step = true
    local l_test_1 = true
    if l == true and l_step == true then
      if l_test_1 then
        l_result = 7
        l = false
        l_step = false
      else
        l = false
      end
    end
  end
  return l_result
end
";
    assert_eq!(lua(items), expected);
}

fn instance(name: &str, context: &[&str], supers: Vec<(String, Dict)>) -> InstanceInfo {
    InstanceInfo {
        module: "App".into(),
        name: name.into(),
        class: ("Prelude".into(), "Show".into()),
        span: (0, 0),
        context: context.iter().map(|&c| c.into()).collect(),
        supers,
        derived: None,
        defaults: vec![],
        generic: None,
        enumerates: None,
    }
}

#[test]
fn an_instance_reads_its_superclasses_and_defaults_when_asked() {
    let with_default = InstanceInfo {
        defaults: vec![("showList".into(), 0, vec![])],
        ..instance("Show Foo", &[], vec![])
    };
    let eq = Dict::Instance {
        module: "App".into(),
        name: "Eq Box".into(),
        args: vec![],
    };
    let given = Dict::Given("Show a".into());
    let shown = lambda(&["x"], app(method(given, "show"), local("x")));
    let items = vec![
        Item {
            kind: ItemKind::Instance {
                info: Box::new(with_default),
                methods: vec![("show".into(), lambda(&["x"], string("foo")))],
            },
            span: sp(0, 0),
        },
        Item {
            kind: ItemKind::Instance {
                info: Box::new(instance("Show Box", &["Show a"], vec![("Eq".into(), eq)])),
                methods: vec![("show".into(), shown)],
            },
            span: sp(0, 0),
        },
    ];
    let expected = r#"M["$i_Show_Foo"] = setmetatable({show = function(x)
  return "foo"
end}, {__index = function(dict, key)
  if key == "showList" then
    return ns.Prelude["$default_Show_showList"](dict)
  end
end})
M["$i_Show_Box"] = function(dict)
  return setmetatable({show = function(x)
    return dict.show(x)
  end}, {__index = function(dict_2, key)
    if key == "$Eq" then
      return M["$i_Eq_Box"]
    end
  end})
end
"#;
    assert_eq!(lua(items), expected);
}

#[test]
fn dictionaries_are_passed_at_once() {
    let mut show_it = value(
        "showIt",
        lambda(
            &["x"],
            app(method(Dict::Given("Show a".into()), "show"), local("x")),
        ),
    );
    if let ItemKind::Value { dicts, .. } = &mut show_it.kind {
        dicts.push("Show a".into());
    }
    let foo = Dict::Instance {
        module: "App".into(),
        name: "Show Foo".into(),
        args: vec![],
    };
    let call = app(
        ex(ExprKind::Dicts(
            Box::new(global("App", "showIt")),
            vec![foo],
        )),
        int(1),
    );
    let items = vec![show_it, value("main", call)];
    let expected = r#"M.showIt = function(dict)
  return function(x)
    return dict.show(x)
  end
end
M.main = M.showIt(M["$i_Show_Foo"])(1)
"#;
    assert_eq!(lua(items), expected);
}

#[test]
fn a_recursive_local_function_sees_itself() {
    let go = lambda(
        &["b"],
        if_(local("b"), app(local("go"), boolean(false)), int(0)),
    );
    let fun = Stmt::Fun {
        name: "go".into(),
        rec: true,
        type_params: vec![],
        dicts: vec![],
        dict_constraints: vec![],
        ty: None,
        value: go,
    };
    let body = block(vec![fun], app(local("go"), boolean(true)));
    let items = vec![value("f", lambda(&["_"], body))];
    let expected = "\
M.f = function(_)
  local function go(b)
    if b then
      return go(false)
    else
      return 0
    end
  end
  return go(true)
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn a_top_level_destructure_assigns_the_modules_fields() {
    let pattern = Pat::Tuple(vec![bind("a"), bind("b")]);
    let body = ex(ExprKind::Tuple(vec![int(1), int(2)]));
    let items = vec![Item {
        kind: ItemKind::Destructure {
            pattern,
            binder_types: vec![],
            body,
        },
        span: sp(0, 0),
    }];
    // the local its value needs stays out of the chunk's scope
    let expected = "\
do
  local v = {1, 2}
  M.a = v[1]
  M.b = v[2]
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn the_right_operand_of_or_runs_only_when_needed() {
    let either = binary(
        BinOp::Or,
        local("a"),
        if_(local("c"), boolean(false), boolean(true)),
    );
    let items = vec![value("h", lambda(&["a", "c"], either))];
    let expected = "\
M.h = function(a)
  return function(c)
    local b = a
    if not b then
      local r
      if c then
        r = false
      else
        r = true
      end
      b = r
    end
    return b
  end
end
";
    assert_eq!(lua(items), expected);
}

#[test]
fn operands_before_statements_are_evaluated_first() {
    let pair = ex(ExprKind::Tuple(vec![
        app(local("f"), int(1)),
        option_match("o", int(0)),
    ]));
    let items = vec![value("k", lambda(&["f", "o"], pair))];
    let expected = tagged(
        r#"M.k = function(f)
  return function(o)
    local t = f(1)
    local r
    repeat
      if o._tag == "<Some>" then
        local x = o[1]
        r = x
        break
      end
      if o._tag == "<None>" then
        r = 0
        break
      end
    until true
    return {t, r}
  end
end
"#,
    );
    assert_eq!(lua(items), expected);
}

#[test]
fn exact_operators_are_lua_operators() {
    let float_op = |arith, l, r| binary(BinOp::Arith(arith, flick_ir::Num::Float), l, r);
    let sum = float_op(
        flick_ir::Arith::Mul,
        float_op(flick_ir::Arith::Add, local("a"), local("b")),
        local("c"),
    );
    let compared = ex(ExprKind::Not(Box::new(binary(
        BinOp::Lt,
        typed(local("a"), "Int"),
        local("b"),
    ))));
    let same = binary(BinOp::Eq, string("x"), local("s"));
    let neg = ex(ExprKind::Neg(flick_ir::Num::Float, Box::new(local("a"))));
    let floats = ex(ExprKind::Tuple(vec![
        float(f64::INFINITY),
        float(f64::NEG_INFINITY),
        float(f64::NAN),
        float(0.1),
        float(1e300),
        float(-2.5),
        neg,
    ]));
    let items = vec![
        value("sum", lambda(&["a", "b", "c"], sum)),
        value("lt", lambda(&["a", "b"], compared)),
        value("same", lambda(&["s"], same)),
        value("floats", lambda(&["a"], floats)),
    ];
    let expected = r#"M.sum = function(a)
  return function(b)
    return function(c)
      return (a + b) * c
    end
  end
end
M.lt = function(a)
  return function(b)
    return not (a < b)
  end
end
M.same = function(s)
  return "x" == s
end
M.floats = function(a)
  return {1 / 0, -1 / 0, 0 / 0, 0.1, 1e300, -2.5, -a}
end
"#;
    assert_eq!(lua(items), expected);
}

#[test]
fn emission_is_deterministic() {
    let items = || {
        vec![value(
            "getOr",
            lambda(&["d", "o"], option_match("o", local("d"))),
        )]
    };
    assert_eq!(lua(items()), lua(items()));
}
