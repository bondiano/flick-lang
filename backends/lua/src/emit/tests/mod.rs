//! The emitter's tests: the printer on hand-built Lua, golden Lua for small
//! IR programs, refusals and VM limits. Each expected text follows from
//! Lua 5.1's rules and the representation `lower` documents, not from a run.

mod build;
mod foreign;
mod kernel;
mod limits;
mod lower;
mod print;
mod refuse;

use std::collections::{BTreeMap, BTreeSet};

use flick_backend_support::names::tag;
use flick_ir::{
    Arm, BinOp, Constructor, Dict, Exports, Expr, ExprKind, FileId, Initialization, IrType, Item,
    ItemKind, LangItem, Lit, Module, Namespace, Param, Pat, Program, Rep, Span, Stage, Stmt,
    TypeRef,
};

use super::ast::Block;
use super::limits::check;
use super::lower::Lowering;
use super::names::GLOBALS;
use super::print::{self as printer, Printed};
use super::{Refusal, Target};
use crate::profiles::{DIALECTS, PROFILES};

pub(super) fn sp(start: u32, end: u32) -> Span {
    Span::new(FileId::new(0), start, end)
}

pub(super) fn at(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr::new(kind, sp(start, end))
}

pub(super) fn ex(kind: ExprKind) -> Expr {
    at(kind, 0, 0)
}

pub(super) fn local(name: &str) -> Expr {
    ex(ExprKind::Local(name.into()))
}

pub(super) fn read(name: &str) -> Expr {
    ex(ExprKind::Var(name.into()))
}

pub(super) fn int(n: i32) -> Expr {
    ex(ExprKind::Lit(Lit::Int(n)))
}

pub(super) fn float(x: f64) -> Expr {
    ex(ExprKind::Lit(Lit::Float(x)))
}

pub(super) fn boolean(b: bool) -> Expr {
    ex(ExprKind::Lit(Lit::Bool(b)))
}

pub(super) fn string(s: &str) -> Expr {
    ex(ExprKind::Lit(Lit::String(s.into())))
}

pub(super) fn unit() -> Expr {
    ex(ExprKind::Lit(Lit::Unit))
}

/// `e` with the type `Prelude.<name>`.
pub(super) fn typed(e: Expr, name: &str) -> Expr {
    let ty = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: name.into(),
        args: vec![],
    });
    Expr { ty: Some(ty), ..e }
}

pub(super) fn global(module: &str, name: &str) -> Expr {
    ex(ExprKind::Global {
        module: module.into(),
        name: name.into(),
        type_args: vec![],
    })
}

pub(super) fn app(func: Expr, arg: Expr) -> Expr {
    ex(ExprKind::App {
        func: Box::new(func),
        arg: Box::new(arg),
        stage: Stage::Pure,
        arg_first: false,
    })
}

pub(super) fn param(name: &str) -> Param {
    Param {
        pat: if name == "_" {
            Pat::Wild
        } else {
            Pat::Bind(name.into())
        },
        stage: Stage::Pure,
        ty: None,
        binder_types: vec![],
    }
}

/// A pure function of the named parameters, `_` for one nobody reads.
pub(super) fn lambda(params: &[&str], body: Expr) -> Expr {
    ex(ExprKind::Lambda {
        params: params.iter().map(|p| param(p)).collect(),
        body: Box::new(body),
    })
}

pub(super) fn block(stmts: Vec<Stmt>, result: Expr) -> Expr {
    ex(ExprKind::Block(stmts, Box::new(result)))
}

pub(super) fn let_(name: &str, value: Expr) -> Stmt {
    Stmt::let_bind(Pat::Bind(name.into()), value)
}

pub(super) fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    ex(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)))
}

pub(super) fn if_(c: Expr, t: Expr, f: Expr) -> Expr {
    ex(ExprKind::If(Box::new(c), Box::new(t), Box::new(f)))
}

pub(super) fn match_(scrutinee: Expr, arms: Vec<Arm>) -> Expr {
    ex(ExprKind::Match(Box::new(scrutinee), arms))
}

pub(super) fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
        binder_types: vec![],
        guard: None,
        body,
    }
}

pub(super) fn bind(name: &str) -> Pat {
    Pat::Bind(name.into())
}

pub(super) fn method(dict: Dict, name: &str) -> Expr {
    ex(ExprKind::Method {
        dict,
        name: name.into(),
        type_args: vec![],
    })
}

/// A tagged constructor of `App.Shape`, or of `Prelude.Option`.
pub(super) fn con(module: &str, ty: &str, name: &str, arity: usize) -> Constructor {
    Constructor {
        module: module.into(),
        ty: ty.into(),
        name: name.into(),
        arity,
        rep: Rep::Tagged,
        dicts: 0,
        fields: vec![],
        type_args: vec![],
    }
}

pub(super) fn some() -> Constructor {
    con("Prelude", "Option", "Some", 1)
}

pub(super) fn none() -> Constructor {
    con("Prelude", "Option", "None", 0)
}

/// The tag of `c`'s values, as both targets spell it.
pub(super) fn tag_of(c: &Constructor) -> String {
    tag(c, None)
}

pub(super) fn value(name: &str, body: Expr) -> Item {
    value_at(name, body, 0, 0)
}

pub(super) fn value_at(name: &str, body: Expr, start: u32, end: u32) -> Item {
    Item {
        kind: ItemKind::Value {
            name: name.into(),
            type_params: vec![],
            dicts: vec![],
            dict_constraints: vec![],
            ty: None,
            body,
        },
        span: sp(start, end),
    }
}

fn module(name: &str, items: Vec<Item>) -> Module {
    Module {
        name: name.into(),
        items,
        types: vec![],
        preambles: vec![],
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    }
}

/// `Prelude`, with `Option` and the scalar types as lang items, and `App`
/// of `items`.
pub(super) fn program(items: Vec<Item>) -> Program {
    let constructor = |c: Constructor| Item {
        kind: ItemKind::Constructor(c),
        span: sp(0, 0),
    };
    let prelude = module("Prelude", vec![constructor(none()), constructor(some())]);
    let lang = |lang: &str, namespace, name: &str| LangItem {
        lang: lang.into(),
        module: "Prelude".into(),
        namespace,
        name: name.into(),
    };
    Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: "test".into(),
        files: vec!["App.fl".into()],
        modules: vec![prelude, module("App", items)],
        lang: vec![
            lang("bool", Namespace::Type, "Bool"),
            lang("float", Namespace::Type, "Float"),
            lang("int", Namespace::Type, "Int"),
            lang("none", Namespace::Constructor, "None"),
            lang("option", Namespace::Type, "Option"),
            lang("some", Namespace::Constructor, "Some"),
            lang("string", Namespace::Type, "String"),
            lang("unit", Namespace::Type, "Unit"),
        ],
    }
}

/// The definitions of the items `items` of module `module`, in that order,
/// as the text of one chunk without its prologue; or everything in them
/// the target refuses.
fn chunk(
    program: &Program,
    module: usize,
    items: &[usize],
    target: &Target<'_>,
) -> Result<Printed, Vec<Refusal>> {
    let builtin = BTreeSet::new();
    let mut lowering = Lowering::new(program, target, &builtin);
    let mut block = Block::default();
    for &item in items {
        for stmt in lowering.item(module, item) {
            block.push(stmt);
        }
    }
    let refused = lowering.refusals();
    if !refused.is_empty() {
        return Err(refused.to_vec());
    }
    let span = (items.first()).map_or(Span::new(FileId::new(0), 0, 0), |&i| {
        program.modules[module].items[i].span
    });
    let over = check(&block, span, target.dialect);
    if !over.is_empty() {
        return Err(over);
    }
    Ok(printer::chunk(&block))
}

/// Every item of `App` as one chunk for the dialect `dialect`.
pub(super) fn emit(dialect: &str, items: Vec<Item>) -> Result<Printed, Vec<Refusal>> {
    let program = program(items);
    let target = Target {
        name: "lua/love",
        dialect: DIALECTS
            .iter()
            .find(|d| d.tag == dialect)
            .expect("a dialect"),
    };
    let all: Vec<usize> = (0..program.modules[1].items.len()).collect();
    chunk(&program, 1, &all, &target)
}

/// The Lua 5.1 text of `items`, which nothing in refuses.
pub(super) fn lua(items: Vec<Item>) -> String {
    match emit("lua51", items) {
        Ok(printed) => printed.code,
        Err(refused) => panic!("refused: {refused:#?}"),
    }
}

/// What the dialect refuses of `items`: each message and its span.
pub(super) fn refusals(dialect: &str, items: Vec<Item>) -> Vec<(String, (u32, u32))> {
    match emit(dialect, items) {
        Ok(printed) => panic!("not refused:\n{}", printed.code),
        Err(refused) => refused
            .into_iter()
            .map(|r| (r.message, (r.span.start, r.span.end)))
            .collect(),
    }
}

#[test]
fn every_profile_defines_the_globals_generated_code_calls() {
    for p in PROFILES {
        for global in GLOBALS {
            assert!(p.defines(global), "{} has no `{global}`", p.id);
        }
    }
}
