//! Unit test harness and builders for polymorphic specialization (#743).

mod cases_budget;
mod cases_semantics;
mod eval;
mod eval_ops;

use std::collections::BTreeMap;

use flick_ir::{
    ConstructorDecl, Exports, Expr, ExprKind, FileId, Initialization, IrType, Item, LangItem,
    Module, Namespace, Param, Pat, Program, Rep, Span, Stage, TypeDecl, TypeRef,
};

pub(super) fn span() -> Span {
    Span::new(FileId::new(0), 0, 0)
}

pub(super) fn named(m: &str, n: &str, args: Vec<TypeRef>) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: m.to_owned(),
        name: n.to_owned(),
        args,
    })
}

pub(super) fn bare(m: &str, n: &str) -> TypeRef {
    named(m, n, Vec::new())
}

pub(super) fn var(n: &str) -> TypeRef {
    TypeRef::new(IrType::Var(n.to_owned()))
}

pub(super) fn fun(param: TypeRef, result: TypeRef) -> TypeRef {
    TypeRef::new(IrType::Fun {
        param,
        stage: Stage::Pure,
        result,
    })
}

pub(super) fn app(f: Expr, a: Expr, ty: Option<TypeRef>) -> Expr {
    Expr::with_ty(
        ExprKind::App {
            func: Box::new(f),
            arg: Box::new(a),
            stage: Stage::Pure,
            arg_first: false,
        },
        span(),
        ty,
    )
}

pub(super) fn lam(name: &str, pty: TypeRef, body: Expr) -> Expr {
    let rty = body.ty.clone().map(|r| fun(pty.clone(), r));
    Expr::with_ty(
        ExprKind::Lambda {
            params: vec![Param {
                pat: Pat::Bind(name.to_owned()),
                stage: Stage::Pure,
                ty: Some(pty.clone()),
                binder_types: vec![(name.to_owned(), pty)],
            }],
            body: Box::new(body),
        },
        span(),
        rty,
    )
}

pub(super) fn empty_mod(name: &str, items: Vec<Item>, types: Vec<TypeDecl>) -> Module {
    Module {
        name: name.to_owned(),
        items,
        types,
        preambles: Vec::new(),
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn base_program(modules: Vec<Module>) -> Program {
    let a = var("a");
    let prelude = empty_mod(
        "Prelude",
        Vec::new(),
        vec![
            TypeDecl {
                name: "Unit".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "Bool".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "Int".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "Float".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "String".into(),
                params: Vec::new(),
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "List".into(),
                params: vec!["a".into()],
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "Option".into(),
                params: vec!["a".into()],
                error: false,
                constructors: vec![
                    ConstructorDecl {
                        name: "None".into(),
                        rep: Rep::Tagged,
                        type_params: vec!["a".into()],
                        existentials: Vec::new(),
                        constraints: Vec::new(),
                        fields: Vec::new(),
                        record: None,
                        result_args: None,
                    },
                    ConstructorDecl {
                        name: "Some".into(),
                        rep: Rep::Tagged,
                        type_params: vec!["a".into()],
                        existentials: Vec::new(),
                        constraints: Vec::new(),
                        fields: vec![a],
                        record: None,
                        result_args: None,
                    },
                ],
                alias: None,
                foreign_forms: Vec::new(),
            },
            TypeDecl {
                name: "Module".into(),
                params: vec!["sig".into()],
                error: false,
                constructors: Vec::new(),
                alias: None,
                foreign_forms: Vec::new(),
            },
        ],
    );
    let mut all = vec![prelude];
    all.extend(modules);
    let ty = |role: &str, name: &str| LangItem {
        lang: role.to_owned(),
        module: "Prelude".to_owned(),
        namespace: Namespace::Type,
        name: name.to_owned(),
    };
    Program {
        ir_version: flick_ir::VERSION.to_owned(),
        compiler: "test".to_owned(),
        files: Vec::new(),
        modules: all,
        lang: vec![
            ty("unit", "Unit"),
            ty("bool", "Bool"),
            ty("int", "Int"),
            ty("float", "Float"),
            ty("string", "String"),
            ty("list", "List"),
            ty("option", "Option"),
        ],
    }
}
