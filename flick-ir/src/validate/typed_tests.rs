//! Negative tests for typed IR validation soundness: constructor patterns,
//! `Dicts` annotations, and erased literals in bindings/blocks/if/match.

use std::collections::{BTreeMap, BTreeSet};

use super::validate;
use crate::{
    Arm, Constructor, ConstructorDecl, Dict, Expr, ExprKind, FileId, Item, ItemKind, Lit, Module,
    Param, Pat, Program, Rep, Span, Stage, Stmt, TypeDecl, TypeInterner,
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
#[allow(clippy::too_many_lines)]
fn rejects_constructor_pattern_type_mismatch() {
    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let float = t.named("Prelude", "Float", Vec::new());
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());
    let var_a = t.var("a");

    let decls = vec![
        TypeDecl {
            name: "Shape".into(),
            params: Vec::new(),
            error: false,
            constructors: vec![ConstructorDecl {
                name: "Circle".into(),
                rep: Rep::Tagged,
                type_params: Vec::new(),
                existentials: Vec::new(),
                constraints: Vec::new(),
                fields: vec![float.clone()],
                record: None,
                result_args: None,
            }],
            alias: None,
        },
        TypeDecl {
            name: "Box".into(),
            params: vec!["a".into()],
            error: false,
            constructors: vec![ConstructorDecl {
                name: "Wrap".into(),
                rep: Rep::Tagged,
                type_params: vec!["a".into()],
                existentials: Vec::new(),
                constraints: Vec::new(),
                fields: vec![var_a],
                record: None,
                result_args: None,
            }],
            alias: None,
        },
    ];

    // 1. Matching `Shape.Circle r` against a `Prelude.String` scrutinee.
    let match_shape = Expr::with_ty(
        ExprKind::Match(
            Box::new(Expr::with_ty(
                ExprKind::Lit(Lit::String("oops".into())),
                span,
                Some(str_ty.clone()),
            )),
            vec![Arm {
                pat: Pat::Con(
                    Constructor {
                        module: "App.Main".into(),
                        ty: "Shape".into(),
                        name: "Circle".into(),
                        arity: 1,
                        rep: Rep::Tagged,
                        dicts: 0,
                        fields: vec![float],
                        type_args: Vec::new(),
                    },
                    vec![Pat::Bind("r".into())],
                ),
                binder_types: Vec::new(),
                guard: None,
                body: at(ExprKind::Lit(Lit::Unit)),
            }],
        ),
        span,
        None,
    );
    let mut p = program(match_shape);
    p.modules[0].types = decls.clone();
    let errs = validate(&p, &|_| false, &files);
    assert!(
        errs.iter()
            .any(|e| e.contains("constructor pattern `App.Main.Circle`")),
        "expected Shape.Circle vs String mismatch, got: {errs:?}"
    );

    // 2. Matching `Box.Wrap` with erased type_args against a `Prelude.String` scrutinee.
    let match_erased = Expr::with_ty(
        ExprKind::Match(
            Box::new(Expr::with_ty(
                ExprKind::Lit(Lit::String("oops".into())),
                span,
                Some(str_ty.clone()),
            )),
            vec![Arm {
                pat: Pat::Con(
                    Constructor {
                        module: "App.Main".into(),
                        ty: "Box".into(),
                        name: "Wrap".into(),
                        arity: 1,
                        rep: Rep::Tagged,
                        dicts: 0,
                        fields: Vec::new(),
                        type_args: Vec::new(),
                    },
                    vec![Pat::Wild],
                ),
                binder_types: Vec::new(),
                guard: None,
                body: at(ExprKind::Lit(Lit::Unit)),
            }],
        ),
        span,
        None,
    );
    let mut p2 = program(match_erased);
    p2.modules[0].types = decls.clone();
    let errs2 = validate(&p2, &|_| false, &files);
    assert!(
        errs2
            .iter()
            .any(|e| e.contains("does not match scrutinee type `Prelude.String`")),
        "expected erased Box.Wrap vs String mismatch, got: {errs2:?}"
    );

    // 3. Matching `Box.Wrap` instantiated at `Int` against a `Box String` scrutinee.
    let box_str = t.named("App.Main", "Box", vec![str_ty]);
    let match_arg = Expr::with_ty(
        ExprKind::Match(
            Box::new(Expr::with_ty(
                ExprKind::Local("b".into()),
                span,
                Some(box_str.clone()),
            )),
            vec![Arm {
                pat: Pat::Con(
                    Constructor {
                        module: "App.Main".into(),
                        ty: "Box".into(),
                        name: "Wrap".into(),
                        arity: 1,
                        rep: Rep::Tagged,
                        dicts: 0,
                        fields: vec![int.clone()],
                        type_args: vec![int],
                    },
                    vec![Pat::Wild],
                ),
                binder_types: Vec::new(),
                guard: None,
                body: at(ExprKind::Lit(Lit::Unit)),
            }],
        ),
        span,
        None,
    );
    let lam = Expr::new(
        ExprKind::Lambda {
            params: vec![Param {
                pat: Pat::Bind("b".into()),
                stage: Stage::Pure,
                ty: Some(box_str.clone()),
                binder_types: vec![("b".into(), box_str)],
            }],
            body: Box::new(match_arg),
        },
        span,
    );
    let mut p3 = program(lam);
    p3.modules[0].types = decls;
    let errs3 = validate(&p3, &|_| false, &files);
    assert!(
        errs3
            .iter()
            .any(|e| e.contains("constructor pattern `App.Main.Wrap`")),
        "expected Box Int vs Box String mismatch, got: {errs3:?}"
    );
}

#[test]
fn rejects_dicts_annotation_mismatch() {
    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());

    for inner_ty in [Some(int), None] {
        let dicts = Expr::with_ty(
            ExprKind::Dicts(
                Box::new(Expr::with_ty(ExprKind::Lit(Lit::Int(1)), span, inner_ty)),
                vec![Dict::Given("d".into())],
            ),
            span,
            Some(str_ty.clone()),
        );
        let mut p = program(dicts);
        if let ItemKind::Value { dicts, .. } = &mut p.modules[0].items[0].kind {
            dicts.push("d".into());
        }
        let errs = validate(&p, &|_| false, &files);
        assert!(
            errs.iter().any(|e| e.contains("dictionary application")),
            "expected Dicts type mismatch, got: {errs:?}"
        );
    }
}

#[test]
fn rejects_erased_literal_type_mismatch_in_bindings_blocks_if_and_match() {
    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let oops = || Expr::new(ExprKind::Lit(Lit::String("oops".into())), span);

    // 1. Stmt::Let binder expects Int, value is untyped String literal.
    let bad_let = Expr::new(
        ExprKind::Block(
            vec![Stmt::let_typed(
                Pat::Bind("x".into()),
                vec![("x".into(), int.clone())],
                oops(),
            )],
            Box::new(at(ExprKind::Lit(Lit::Unit))),
        ),
        span,
    );
    assert!(!validate(&program(bad_let), &|_| false, &files).is_empty());

    // 2. Stmt::Var expects Int, initializer is untyped String literal.
    let bad_var = Expr::new(
        ExprKind::Block(
            vec![Stmt::var("v".into(), Some(int.clone()), oops())],
            Box::new(at(ExprKind::Lit(Lit::Unit))),
        ),
        span,
    );
    assert!(!validate(&program(bad_var), &|_| false, &files).is_empty());

    // 3. Block expects Int, result is untyped String literal.
    let bad_block = Expr::with_ty(
        ExprKind::Block(Vec::new(), Box::new(oops())),
        span,
        Some(int.clone()),
    );
    assert!(!validate(&program(bad_block), &|_| false, &files).is_empty());

    // 4. If expects Int, else branch is untyped String literal.
    let bad_if = Expr::with_ty(
        ExprKind::If(
            Box::new(at(ExprKind::Lit(Lit::Bool(true)))),
            Box::new(at(ExprKind::Lit(Lit::Int(1)))),
            Box::new(oops()),
        ),
        span,
        Some(int.clone()),
    );
    assert!(!validate(&program(bad_if), &|_| false, &files).is_empty());

    // 5. Match expects Int, arm body is untyped String literal.
    let bad_match = Expr::with_ty(
        ExprKind::Match(
            Box::new(at(ExprKind::Lit(Lit::Unit))),
            vec![Arm {
                pat: Pat::Wild,
                binder_types: Vec::new(),
                guard: None,
                body: oops(),
            }],
        ),
        span,
        Some(int),
    );
    assert!(!validate(&program(bad_match), &|_| false, &files).is_empty());
}

#[test]
fn record_field_agrees_only_with_matching_single_field_closed_record() {
    use crate::{Lang, LangItem, Namespace};

    let files = BTreeSet::from([FileId::new(0)]);
    let span = Span::new(FileId::new(0), 0, 0);
    let mut t = TypeInterner::new();
    let int = t.named("Prelude", "Int", Vec::new());
    let str_ty = t.named("Prelude", "String", Vec::new());
    let field_int = t.named("Record", "Field", vec![int.clone()]);
    let rec_int = t.record(vec![("x".into(), int.clone())], None);
    let rec_str = t.record(vec![("x".into(), str_ty)], None);
    let rec_two = t.record(
        vec![("x".into(), int.clone()), ("y".into(), int.clone())],
        None,
    );
    let rec_open = t.record(vec![("x".into(), int)], Some("r".into()));

    let mut check = |declared: crate::TypeRef, actual: crate::TypeRef| {
        let mut p = program(Expr::with_ty(
            ExprKind::Local("x".into()),
            span,
            Some(actual.clone()),
        ));
        p.lang.push(LangItem {
            lang: Lang::Field.name().into(),
            module: "Record".into(),
            namespace: Namespace::Type,
            name: "Field".into(),
        });
        let ItemKind::Value { ty, body, .. } = &mut p.modules[0].items[0].kind else {
            unreachable!()
        };
        *ty = Some(t.fun(declared.clone(), Stage::Pure, declared));
        let lam_ty = t.fun(actual.clone(), Stage::Pure, actual.clone());
        *body = Expr::with_ty(
            ExprKind::Lambda {
                params: vec![Param {
                    pat: Pat::Bind("x".into()),
                    stage: Stage::Pure,
                    ty: Some(actual.clone()),
                    binder_types: vec![("x".into(), actual.clone())],
                }],
                body: Box::new(Expr::with_ty(
                    ExprKind::Local("x".into()),
                    span,
                    Some(actual),
                )),
            },
            span,
            Some(lam_ty),
        );
        validate(&p, &|_| false, &files)
    };

    assert!(check(field_int.clone(), rec_int.clone()).is_empty());
    assert!(check(rec_int, field_int.clone()).is_empty());
    assert!(!check(field_int.clone(), rec_str).is_empty());
    assert!(!check(field_int.clone(), rec_two).is_empty());
    assert!(!check(field_int, rec_open).is_empty());
}
