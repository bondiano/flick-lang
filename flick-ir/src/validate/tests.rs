use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use super::validate;
use crate::{
    Arm, Constructor, ConstructorDecl, Expr, ExprKind, FileId, Item, ItemKind, Lit, Module, Param,
    Pat, Program, Rep, Span, Stage, Stmt, TypeDecl, TypeInterner,
};

fn at(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn program(body: Expr) -> Program {
    let item = Item {
        kind: ItemKind::Value {
            name: "f".into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body,
        },
        span: Span::new(FileId::new(0), 0, 0),
    };
    Program {
        ir_version: crate::VERSION.into(),
        compiler: "test".into(),
        files: Vec::new(),
        modules: vec![Module {
            name: "App.Main".into(),
            items: vec![item],
            types: Vec::new(),
            labels: BTreeMap::default(),
            initialization: crate::Initialization::default(),
            records: BTreeMap::default(),
            package: None,
            exports: crate::Exports::default(),
        }],
        lang: Vec::new(),
    }
}

#[test]
fn a_local_needs_its_binder_and_a_global_its_module() {
    let files = BTreeSet::from([FileId::new(0)]);
    let external = |m: &str| m == "List";
    let x = Pat::Bind("x".into());
    let bound = at(ExprKind::Lambda {
        params: vec![Param {
            pat: x,
            stage: Stage::Pure,
            ty: None,
            binder_types: Vec::new(),
        }],
        body: Box::new(at(ExprKind::Local("x".into()))),
    });
    assert_eq!(
        validate(&program(bound), &external, &files),
        Vec::<String>::new()
    );
    let dangling = at(ExprKind::Local("y".into()));
    assert_eq!(
        validate(&program(dangling), &external, &files),
        ["`App.Main` uses the local `y`, which nothing binds there"]
    );
    let elsewhere = at(ExprKind::Global {
        module: "Nowhere".into(),
        name: "g".into(),
        type_args: Vec::new(),
    });
    assert_eq!(
        validate(&program(elsewhere), &external, &files),
        ["`App.Main` uses `Nowhere`, a module of no build"]
    );
    let unit = at(ExprKind::Lit(Lit::Unit));
    assert_eq!(
        validate(&program(unit), &external, &BTreeSet::new()).len(),
        2
    );
}

#[test]
fn a_variable_is_read_and_assigned_where_a_var_binds_it() {
    let files = BTreeSet::from([FileId::new(0)]);
    let one = || at(ExprKind::Lit(Lit::Int(1)));
    let block = |stmts, result| at(ExprKind::Block(stmts, Box::new(result)));
    let counted = block(
        vec![
            Stmt::var("n".into(), None, one()),
            Stmt::Assign("n".into(), one()),
        ],
        at(ExprKind::Var("n".into())),
    );
    assert_eq!(
        validate(&program(counted), &|_| false, &files),
        Vec::<String>::new()
    );
    let immutable = block(
        vec![
            Stmt::let_bind(Pat::Bind("n".into()), one()),
            Stmt::Assign("n".into(), one()),
        ],
        at(ExprKind::Var("n".into())),
    );
    assert_eq!(
        validate(&program(immutable), &|_| false, &files),
        [
            "`App.Main` assigns `n`, which is not a variable in scope there",
            "`App.Main` reads `n`, which is not a variable in scope there",
        ]
    );
    let outside = block(
        vec![Stmt::Expr(block(
            vec![Stmt::var("n".into(), None, one())],
            one(),
        ))],
        at(ExprKind::Var("n".into())),
    );
    assert_eq!(
        validate(&program(outside), &|_| false, &files),
        ["`App.Main` reads `n`, which is not a variable in scope there"]
    );
}

#[test]
fn an_or_pattern_binds_only_names_in_every_alternative() {
    let body = at(ExprKind::Match(
        Box::new(at(ExprKind::Lit(Lit::Unit))),
        vec![Arm {
            pat: Pat::Or(vec![Pat::Bind("x".into()), Pat::Wild]),
            binder_types: Vec::new(),
            guard: None,
            body: at(ExprKind::Local("x".into())),
        }],
    ));
    let files = BTreeSet::from([FileId::new(0)]);
    assert_eq!(
        validate(&program(body), &|_| false, &files),
        ["`App.Main` uses the local `x`, which nothing binds there"]
    );
}

#[test]
fn exits_target_the_nearest_loop() {
    let files = BTreeSet::from([FileId::new(0)]);
    let exit = || {
        at(ExprKind::LoopExit {
            id: "outer".into(),
            value: None,
        })
    };
    let looping = |id: &str, body| {
        at(ExprKind::ControlLoop {
            id: id.into(),
            condition: None,
            body: Box::new(body),
            iterable: false,
        })
    };
    assert_eq!(
        validate(&program(looping("outer", exit())), &|_| false, &files),
        Vec::<String>::new()
    );
    assert_eq!(
        validate(&program(exit()), &|_| false, &files),
        ["exit targets no loop: `outer`"]
    );
    let nested = looping("outer", looping("inner", exit()));
    assert_eq!(
        validate(&program(nested), &|_| false, &files),
        Vec::<String>::new()
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn rejects_wrong_number_of_type_arguments() {
    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());
    let var_a = t.var("a");

    // 1. Named type with wrong arity: `Box` takes 1 param, applied to 2.
    let bad_named = t.named("App.Main", "Box", vec![int.clone(), str_ty.clone()]);
    let mut p = program(Expr::with_ty(ExprKind::Lit(Lit::Unit), span, None));
    p.modules[0].types.push(TypeDecl {
        name: "Box".into(),
        params: vec!["a".into()],
        error: false,
        constructors: vec![ConstructorDecl {
            name: "Wrap".into(),
            rep: Rep::Tagged,
            type_params: vec!["a".into()],
            existentials: Vec::new(),
            constraints: Vec::new(),
            fields: vec![var_a.clone()],
            record: None,
            result_args: None,
        }],
        alias: None,
    });
    // Add polymorphic global `id : forall a. a -> a`.
    let id_ty = t.fun(var_a.clone(), Stage::Pure, var_a.clone());
    p.modules[0].items.push(Item {
        span,
        kind: ItemKind::Value {
            name: "id".into(),
            type_params: vec!["a".into()],
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: Some(id_ty),
            body: Expr::with_ty(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Bind("x".into()),
                        stage: Stage::Pure,
                        ty: Some(var_a.clone()),
                        binder_types: vec![("x".into(), var_a.clone())],
                    }],
                    body: Box::new(Expr::with_ty(
                        ExprKind::Local("x".into()),
                        span,
                        Some(var_a.clone()),
                    )),
                },
                span,
                Some(t.fun(var_a.clone(), Stage::Pure, var_a.clone())),
            ),
        },
    });
    // Use `Box` with wrong type arg count in `f`'s type annotation.
    let mut p_named = p.clone();
    if let ItemKind::Value { ty, .. } = &mut p_named.modules[0].items[0].kind {
        *ty = Some(bad_named);
    }
    let errs = validate(&p_named, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("applies type `App.Main.Box` to 2 type argument(s), expected 1")),
        "expected wrong named type arity error, got: {errs:?}"
    );

    // 2. Global `id` given 2 type args instead of 1.
    let mut p_global = p.clone();
    if let ItemKind::Value { body, .. } = &mut p_global.modules[0].items[0].kind {
        *body = Expr::with_ty(
            ExprKind::Global {
                module: "App.Main".into(),
                name: "id".into(),
                type_args: vec![int.clone(), str_ty.clone()],
            },
            span,
            None,
        );
    }
    let errs = validate(&p_global, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("global `App.Main.id` given 2 type argument(s), expected 1")),
        "expected wrong global type arg count error, got: {errs:?}"
    );

    // 3. Constructor `Wrap` given 0 type args while typed.
    let mut p_con = p.clone();
    if let ItemKind::Value { body, .. } = &mut p_con.modules[0].items[0].kind {
        *body = Expr::with_ty(
            ExprKind::Con(Constructor {
                module: "App.Main".into(),
                ty: "Box".into(),
                name: "Wrap".into(),
                arity: 1,
                rep: Rep::Tagged,
                dicts: 0,
                fields: vec![int.clone()],
                type_args: Vec::new(),
            }),
            span,
            None,
        );
    }
    let errs = validate(&p_con, &|_| false, &files);
    assert!(
        errs.iter().any(|e| e.contains("constructor `App.Main.Wrap` given 0 type argument(s), expected 1")),
        "expected wrong constructor type arg count error, got: {errs:?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn rejects_constructor_operand_replaced_with_string_retaining_int_annotation() {
    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());
    let num_box = t.named("App.Main", "NumBox", Vec::new());
    let ctor_ty = t.fun(int.clone(), Stage::Pure, num_box.clone());

    let make = |arg: Expr| {
        let con = Expr::with_ty(
            ExprKind::Con(Constructor {
                module: "App.Main".into(),
                ty: "NumBox".into(),
                name: "NumBox".into(),
                arity: 1,
                rep: Rep::Tagged,
                dicts: 0,
                fields: vec![int.clone()],
                type_args: Vec::new(),
            }),
            span,
            Some(ctor_ty.clone()),
        );
        let app = Expr::with_ty(
            ExprKind::App {
                func: Box::new(con),
                arg: Box::new(arg),
                stage: Stage::Pure,
                arg_first: false,
            },
            span,
            Some(num_box.clone()),
        );
        let mut p = program(app);
        p.modules[0].types.push(TypeDecl {
            name: "NumBox".into(),
            params: Vec::new(),
            error: false,
            constructors: vec![ConstructorDecl {
                name: "NumBox".into(),
                rep: Rep::Tagged,
                type_params: Vec::new(),
                existentials: Vec::new(),
                constraints: Vec::new(),
                fields: vec![int.clone()],
                record: None,
                result_args: None,
            }],
            alias: None,
        });
        p
    };

    // Valid Int operand passes.
    let valid = make(Expr::with_ty(
        ExprKind::Lit(Lit::Int(42)),
        span,
        Some(int.clone()),
    ));
    assert_eq!(validate(&valid, &|_| false, &files), Vec::<String>::new());

    // Transform replaces Int operand with String literal while retaining Int annotation -> rejected!
    let stale_int = make(Expr::with_ty(
        ExprKind::Lit(Lit::String("oops".into())),
        span,
        Some(int.clone()),
    ));
    let errs = validate(&stale_int, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("literal annotated as `Prelude.Int`, expected `Prelude.String`")),
        "expected literal annotation mismatch, got: {errs:?}"
    );

    // Transform replaces Int operand with String literal annotated as String -> rejected!
    let wrong_arg = make(Expr::with_ty(
        ExprKind::Lit(Lit::String("oops".into())),
        span,
        Some(str_ty.clone()),
    ));
    let errs = validate(&wrong_arg, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("has type `Prelude.String`, expected `Prelude.Int`")),
        "expected call/constructor operand mismatch, got: {errs:?}"
    );

    // Transform replaces Int operand with a String-bound local while retaining Int annotation -> rejected!
    let bad_local = make(Expr::with_ty(
        ExprKind::Local("s".into()),
        span,
        Some(int.clone()),
    ));
    let mut wrapped = bad_local;
    if let ItemKind::Value { body, .. } = &mut wrapped.modules[0].items[0].kind {
        let inner = mem::replace(body, at(ExprKind::Lit(Lit::Unit)));
        *body = Expr::with_ty(
            ExprKind::Block(
                vec![Stmt::let_typed(
                    Pat::Bind("s".into()),
                    vec![("s".into(), str_ty.clone())],
                    Expr::with_ty(
                        ExprKind::Lit(Lit::String("hi".into())),
                        span,
                        Some(str_ty.clone()),
                    ),
                )],
                Box::new(inner),
            ),
            span,
            Some(num_box),
        );
    }
    let errs = validate(&wrapped, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("local `s` has type `Prelude.Int`, expected `Prelude.String`")),
        "expected local binder mismatch, got: {errs:?}"
    );
}
