//! Typed IR acceptance tests (#741): constructor field types, polymorphic
//! binders, instantiation type arguments (including `List.map` and GADTs),
//! type-preserving optimization, wire interning/sharing, and negative
//! validation checks before layout.

use std::collections::BTreeSet;

use flick_ir::{Expr, ExprKind, IrType, ItemKind, Lit, Program, Stmt, TypeRef};

use super::lower_text;
use crate::ir::lang_items;
use crate::opt::optimize;
use crate::types;

const TYPED_PROGRAM: &str = "\
module App exposing (..)

type Shape =
  | Circle Float
  | Rect Float Float

type Tree a =
  | Leaf a
  | Node (Tree a) (Tree a)

type BoxRec =
  | BoxRec { x : Float; y : String }

type Expr a =
  | IntLit Int : Expr Int
  | Lift a : Expr a
  | Apply (Expr (b -> a)) (Expr b) : Expr a

type ShowBox =
  | Show b => ShowBox b : ShowBox

poly : (a -> b) -> List a -> b -> (List b, Int)
let poly f xs fallback =
  let mapped = List.map f xs
  let mut count = 0
  let picked =
    match mapped with
    | [head, ..._] ->
      count = count + 1
      head
    | [] -> fallback
  let _ = picked
  (mapped, count)

use_all : Unit -> (Shape, Tree Int, BoxRec, List String, Expr String, ShowBox)
let use_all () =
  let s = Rect 3.0 4.0
  let t = Node (Leaf 1) (Leaf 2)
  let r = BoxRec { x = 1.5; y = \"ok\" }
  let shown = List.map show [1, 2]
  let applied : Expr String = Apply (Lift show) (IntLit 42)
  let boxed = ShowBox 42
  (s, t, r, shown, applied, boxed)
";

fn strs(ts: &[TypeRef]) -> Vec<String> {
    ts.iter().map(ToString::to_string).collect()
}

fn validate_module(module: &flick_ir::Module) -> Vec<String> {
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    let mut files = BTreeSet::new();
    module.clone().each_span_mut(&mut |s| {
        files.insert(s.file);
    });
    let program = Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![module.clone()],
        lang: lang_items(),
    };
    flick_ir::validate(&program, &builtin, &files)
}

fn find_value<'a>(
    module: &'a flick_ir::Module,
    wanted: &str,
) -> (&'a [String], Option<&'a TypeRef>, &'a Expr) {
    module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value {
                name,
                type_params,
                ty,
                body,
                ..
            } if name == wanted => Some((type_params.as_slice(), ty.as_ref(), body)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing value `{wanted}`"))
}

fn collect_exprs<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    out.push(expr);
    expr.each_child(&mut |child| collect_exprs(child, out));
}

#[test]
#[allow(clippy::too_many_lines)]
fn typed_ir_carries_decl_fields_binders_and_type_args() {
    let module = lower_text("App", TYPED_PROGRAM).expect("lowers");
    let problems = validate_module(&module);
    assert!(problems.is_empty(), "validation failed: {problems:?}");

    // 1. Type declarations: Shape, Tree a, BoxRec, Expr a, ShowBox
    let shape = module
        .types
        .iter()
        .find(|t| t.name == "Shape")
        .expect("Shape decl");
    assert!(shape.params.is_empty());
    let circle = shape
        .constructors
        .iter()
        .find(|c| c.name == "Circle")
        .expect("Circle");
    assert_eq!(
        circle
            .fields
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Prelude.Float"]
    );
    let rect = shape
        .constructors
        .iter()
        .find(|c| c.name == "Rect")
        .expect("Rect");
    assert_eq!(
        rect.fields
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["Prelude.Float", "Prelude.Float"]
    );

    let tree = module
        .types
        .iter()
        .find(|t| t.name == "Tree")
        .expect("Tree decl");
    assert_eq!(tree.params, vec!["a"]);
    let leaf = tree
        .constructors
        .iter()
        .find(|c| c.name == "Leaf")
        .expect("Leaf");
    assert_eq!(
        leaf.fields
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    let node = tree
        .constructors
        .iter()
        .find(|c| c.name == "Node")
        .expect("Node");
    assert_eq!(
        node.fields
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["App.Tree a", "App.Tree a"]
    );

    let box_rec = module
        .types
        .iter()
        .find(|t| t.name == "BoxRec")
        .expect("BoxRec decl");
    let box_rec_ctor = &box_rec.constructors[0];
    assert_eq!(
        box_rec_ctor.record,
        Some(vec!["x".to_owned(), "y".to_owned()])
    );
    assert_eq!(
        strs(&box_rec_ctor.fields),
        vec!["{x : Prelude.Float, y : Prelude.String}"]
    );

    // GADT declarations: Expr a (indexed + existential) and ShowBox (existential + dictionary)
    let expr_decl = module
        .types
        .iter()
        .find(|t| t.name == "Expr")
        .expect("Expr decl");
    assert_eq!(expr_decl.params, vec!["a"]);
    let int_lit = expr_decl
        .constructors
        .iter()
        .find(|c| c.name == "IntLit")
        .expect("IntLit");
    assert_eq!(strs(&int_lit.fields), vec!["Prelude.Int"]);
    assert_eq!(
        int_lit.result_args.as_deref().map(strs),
        Some(vec!["Prelude.Int".to_owned()])
    );
    let apply = expr_decl
        .constructors
        .iter()
        .find(|c| c.name == "Apply")
        .expect("Apply");
    assert_eq!(apply.existentials, vec!["b"]);
    assert_eq!(strs(&apply.fields), vec!["App.Expr (b -> a)", "App.Expr b"]);
    assert_eq!(
        apply.result_args.as_deref().map(strs),
        Some(vec!["a".to_owned()])
    );

    let show_box = module
        .types
        .iter()
        .find(|t| t.name == "ShowBox")
        .expect("ShowBox decl");
    let show_box_ctor = &show_box.constructors[0];
    assert_eq!(show_box_ctor.existentials, vec!["b"]);
    assert_eq!(show_box_ctor.constraints.len(), 1);
    assert_eq!(show_box_ctor.constraints[0].class, "Show");
    assert_eq!(strs(&show_box_ctor.fields), vec!["b"]);

    // 2. Polymorphic function binders (`let`, param, pattern var, `Var`) and `List.map` type_args
    let (poly_params, poly_ty, poly_body) = find_value(&module, "poly");
    assert_eq!(poly_params, &["a", "b"]);
    assert_eq!(
        poly_ty.map(ToString::to_string).as_deref(),
        Some("(a -> b) -> Prelude.List a -> b -> (Prelude.List b, Prelude.Int)")
    );
    let mut exprs = Vec::new();
    collect_exprs(poly_body, &mut exprs);

    let param_tys: Vec<String> = exprs
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::Lambda { params, .. } => Some(params),
            _ => None,
        })
        .flatten()
        .filter_map(|p| p.ty.as_ref().map(ToString::to_string))
        .collect();
    assert_eq!(param_tys, vec!["a -> b", "Prelude.List a", "b"]);

    let mut let_binders = Vec::new();
    let mut var_binders = Vec::new();
    let mut arm_binders = Vec::new();
    let mut map_type_args = Vec::new();
    for e in &exprs {
        match &e.kind {
            ExprKind::Block(stmts, _) => {
                for s in stmts {
                    match s {
                        Stmt::Let { binder_types, .. } => {
                            for (n, t) in binder_types {
                                let_binders.push((n.as_str(), t.to_string()));
                            }
                        }
                        Stmt::Var { name, ty, .. } => {
                            var_binders.push((
                                name.as_str(),
                                ty.as_ref().map(ToString::to_string).unwrap_or_default(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::Match(_, arms) => {
                for arm in arms {
                    for (n, t) in &arm.binder_types {
                        arm_binders.push((n.as_str(), t.to_string()));
                    }
                }
            }
            ExprKind::Global {
                module,
                name,
                type_args,
            } if module == "List" && name == "map" => {
                map_type_args.push(
                    type_args
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                );
            }
            _ => {}
        }
    }
    assert!(
        let_binders.contains(&("mapped", "Prelude.List b".to_owned())),
        "{let_binders:?}"
    );
    assert!(
        let_binders.contains(&("picked", "b".to_owned())),
        "{let_binders:?}"
    );
    assert_eq!(var_binders, vec![("count", "Prelude.Int".to_owned())]);
    assert!(
        arm_binders.contains(&("head", "b".to_owned())),
        "{arm_binders:?}"
    );
    assert_eq!(map_type_args, vec![vec!["a".to_owned(), "b".to_owned()]]);

    // 3. Concrete calls: `List.map`, `Tree.Leaf`/`Node`, and GADT constructors `Apply` & `ShowBox`
    let (_, _, use_all_body) = find_value(&module, "use_all");
    let mut use_exprs = Vec::new();
    collect_exprs(use_all_body, &mut use_exprs);

    let concrete_map = use_exprs
        .iter()
        .find_map(|e| match &e.kind {
            ExprKind::Global {
                module,
                name,
                type_args,
            } if module == "List" && name == "map" => Some(
                type_args
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("List.map call");
    assert_eq!(concrete_map, vec!["Prelude.Int", "Prelude.String"]);

    let apply_use = use_exprs
        .iter()
        .find_map(|e| match &e.kind {
            ExprKind::Con(c) if c.name == "Apply" => Some(c),
            _ => None,
        })
        .expect("Apply con use");
    assert_eq!(apply_use.type_args.len(), 2);
    assert_eq!(
        strs(&apply_use.fields),
        vec![
            "App.Expr (Prelude.Int -> Prelude.String)",
            "App.Expr Prelude.Int"
        ]
    );

    let show_box_use = use_exprs
        .iter()
        .find_map(|e| match &e.kind {
            ExprKind::Con(c) if c.name == "ShowBox" => Some(c),
            _ => None,
        })
        .expect("ShowBox con use");
    assert_eq!(show_box_use.dicts, 1);
    assert_eq!(strs(&show_box_use.type_args), vec!["Prelude.Int"]);
    assert_eq!(strs(&show_box_use.fields), vec!["Prelude.Int"]);

    // 4. Wire CBOR round-trip preserves shared `Arc` interning
    let program = Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![module.clone()],
        lang: lang_items(),
    };
    let decoded = Program::from_cbor(&program.to_cbor()).expect("cbor round-trip");
    assert_eq!(decoded, program);
    let shape_dec = decoded.modules[0]
        .types
        .iter()
        .find(|t| t.name == "Shape")
        .expect("Shape");
    let f1 = &shape_dec.constructors[0].fields[0];
    let f2 = &shape_dec.constructors[1].fields[0];
    assert!(
        f1.ptr_eq(f2),
        "wire interning must share identical IrType nodes behind Arc"
    );

    // 5. Type-preserving optimization preserves valid types
    let mut optimized = module;
    optimize(&mut optimized, &|_| true, &|_, _| None, &|_, _| None);
    let opt_problems = validate_module(&optimized);
    assert!(
        opt_problems.is_empty(),
        "post-opt validation failed: {opt_problems:?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn validation_rejects_bad_type_args_and_inconsistent_annotations_but_allows_erased() {
    let base = lower_text("App", TYPED_PROGRAM).expect("lowers");
    assert!(validate_module(&base).is_empty());

    // 1. Corrupt an `Int` constructor operand (`Leaf 1`) with a `String` while keeping its `Int` annotation
    let mut bad_keep_int = base.clone();
    let mut corrupted = false;
    for item in &mut bad_keep_int.items {
        if let ItemKind::Value { name, body, .. } = &mut item.kind
            && name == "use_all"
        {
            corrupt_con_int_operand(body, false, &mut corrupted);
        }
    }
    assert!(corrupted, "did not locate Leaf Int operand");
    let problems = validate_module(&bad_keep_int);
    assert!(
        !problems.is_empty(),
        "replacing Int constructor operand with String keeping Int annotation must fail validation"
    );

    // 2. Even if its annotation is updated to `String`, `Leaf` expects `Int`
    let mut bad_update_str = base.clone();
    let mut corrupted = false;
    for item in &mut bad_update_str.items {
        if let ItemKind::Value { name, body, .. } = &mut item.kind
            && name == "use_all"
        {
            corrupt_con_int_operand(body, true, &mut corrupted);
        }
    }
    assert!(corrupted, "did not locate Leaf Int operand");
    let problems = validate_module(&bad_update_str);
    assert!(
        !problems.is_empty(),
        "replacing Int constructor operand with String annotated String must still fail against constructor field type"
    );

    // 3. Wrong number of type arguments on a polymorphic constructor (`Leaf`)
    let mut bad_con_args = base.clone();
    let mut mutated = false;
    for item in &mut bad_con_args.items {
        if let ItemKind::Value { name, body, .. } = &mut item.kind
            && name == "use_all"
        {
            mutate_first_con_args(body, "Leaf", &mut mutated);
        }
    }
    assert!(mutated, "did not locate Leaf");
    let problems = validate_module(&bad_con_args);
    assert!(
        !problems.is_empty(),
        "wrong number of constructor type_args must fail validation"
    );

    // 4. Wrong number of type arguments on a lowered polymorphic global (`poly`)
    let mut bad_global_args = base.clone();
    let mut mutated_global = false;
    for item in &mut bad_global_args.items {
        if let ItemKind::Value { name, body, .. } = &mut item.kind
            && name == "use_all"
        {
            let span = body.span;
            body.kind = ExprKind::Global {
                module: "App".into(),
                name: "poly".into(),
                type_args: vec![TypeRef::new(IrType::Named {
                    module: "Prelude".into(),
                    name: "Int".into(),
                    args: Vec::new(),
                })],
            };
            body.ty = None;
            body.span = span;
            mutated_global = true;
        }
    }
    assert!(mutated_global);
    assert!(
        validate_module(&bad_global_args)
            .iter()
            .any(|e| e.contains("global `App.poly` given 1 type argument(s), expected 2")),
        "wrong number of global type_args must fail validation"
    );

    // 5. Wrong number of named type arguments in a TypeDecl (`App.Tree a` -> `App.Tree a a`)
    let mut bad_named_args = base.clone();
    if let Some(decl) = bad_named_args.types.iter_mut().find(|t| t.name == "Tree")
        && let Some(node) = decl.constructors.iter_mut().find(|c| c.name == "Node")
    {
        let a = TypeRef::new(IrType::Var("a".into()));
        node.fields[0] = TypeRef::new(IrType::Named {
            module: "App".into(),
            name: "Tree".into(),
            args: vec![a.clone(), a],
        });
    }
    assert!(
        !validate_module(&bad_named_args).is_empty(),
        "wrong number of named type arguments must fail validation"
    );

    // 6. Erased representation path remains valid
    let mut erased = base;
    for item in &mut erased.items {
        if let ItemKind::Value { body, ty, .. } = &mut item.kind {
            *ty = None;
            erase_expr(body);
        }
    }
    let problems = validate_module(&erased);
    assert!(
        problems.is_empty(),
        "erased IR path must remain valid: {problems:?}"
    );
}

fn corrupt_con_int_operand(expr: &mut Expr, update_ty: bool, done: &mut bool) {
    if *done {
        return;
    }
    if let ExprKind::App { func, arg, .. } = &mut expr.kind
        && matches!(&func.kind, ExprKind::Con(c) if c.name == "Leaf")
        && matches!(&arg.kind, ExprKind::Lit(Lit::Int(_)))
    {
        arg.kind = ExprKind::Lit(Lit::String("oops".into()));
        if update_ty {
            arg.ty = Some(TypeRef::new(IrType::Named {
                module: "Prelude".into(),
                name: "String".into(),
                args: Vec::new(),
            }));
        }
        *done = true;
        return;
    }
    expr.each_child_mut(&mut |c| corrupt_con_int_operand(c, update_ty, done));
}

fn mutate_first_con_args(expr: &mut Expr, target: &str, done: &mut bool) {
    if *done {
        return;
    }
    if let ExprKind::Con(c) = &mut expr.kind
        && c.name == target
    {
        c.type_args.push(TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "String".into(),
            args: Vec::new(),
        }));
        *done = true;
        return;
    }
    expr.each_child_mut(&mut |c| mutate_first_con_args(c, target, done));
}

fn erase_expr(expr: &mut Expr) {
    expr.ty = None;
    match &mut expr.kind {
        ExprKind::Con(c) => {
            c.fields.clear();
            c.type_args.clear();
        }
        ExprKind::Global { type_args, .. } | ExprKind::Method { type_args, .. } => {
            type_args.clear();
        }
        ExprKind::Lambda { params, .. } => {
            for p in params {
                p.ty = None;
                p.binder_types.clear();
            }
        }
        ExprKind::Block(stmts, _) => {
            for stmt in stmts {
                match stmt {
                    Stmt::Let { binder_types, .. } => binder_types.clear(),
                    Stmt::Var { ty, .. } => *ty = None,
                    Stmt::Fun {
                        type_params,
                        dict_constraints,
                        ty,
                        ..
                    } => {
                        type_params.clear();
                        dict_constraints.clear();
                        *ty = None;
                    }
                    _ => {}
                }
            }
        }
        ExprKind::Match(_, arms) => {
            for arm in arms {
                arm.binder_types.clear();
            }
        }
        ExprKind::Each { binder_types, .. } => binder_types.clear(),
        _ => {}
    }
    expr.each_child_mut(&mut erase_expr);
}
