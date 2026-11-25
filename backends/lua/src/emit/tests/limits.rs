//! VM limits at the boundaries `probes/limits.lua` measured on each
//! dialect: what compiles at a limit is emitted, one past it is refused at
//! its Flick source.

use flick_ir::{ExprKind, Lit};

use super::super::Refusal;
use super::super::ast::{
    Block, Expr, ExprKind as Lua, Field, Function, LastKind, Name, Stmt, StmtKind,
};
use super::super::limits::check;
use super::{at, block, emit, ex, int, lambda, let_, local, refusals, sp, value, value_at};
use crate::profiles::{DIALECTS, Dialect};

fn dialect(tag: &str) -> &'static Dialect {
    DIALECTS.iter().find(|d| d.tag == tag).expect("a dialect")
}

fn name(text: &str) -> Name {
    Name::new(text.to_owned())
}

fn stmt(kind: StmtKind) -> Stmt {
    Stmt::new(kind, sp(0, 0))
}

fn lua(kind: Lua) -> Expr {
    Expr::new(kind, sp(0, 0))
}

fn chunk(stmts: Vec<Stmt>, tag: &str) -> Vec<Refusal> {
    let block = Block { stmts, last: None };
    check(&block, sp(0, 0), dialect(tag))
}

/// `local f` and a call of `f` with `n` arguments, as the probe makes.
fn call_of(n: usize) -> Vec<Stmt> {
    let args = (0..n).map(|_| lua(Lua::Number("1".into()))).collect();
    let call = Expr::name(&name("f"), sp(0, 0)).call(args);
    vec![
        stmt(StmtKind::Local(name("f"), None)),
        stmt(StmtKind::Call(call)),
    ]
}

#[test]
fn a_call_fits_its_frame_as_the_probe_measured() {
    // 248 arguments fail on Lua 5.1, 247 on LuaJIT with its frame link
    assert_eq!(chunk(call_of(247), "lua51"), vec![]);
    let over = chunk(call_of(248), "lua51");
    assert_eq!(over.len(), 1, "{over:#?}");
    assert!(over[0].message.contains("250 registers"), "{over:#?}");
    assert_eq!(chunk(call_of(246), "luajit"), vec![]);
    assert_eq!(chunk(call_of(247), "luajit").len(), 1);
}

#[test]
fn a_chunk_has_two_hundred_locals_and_a_function_sixty_upvalues() {
    let locals = |n: usize| -> Vec<Stmt> {
        (0..n)
            .map(|i| stmt(StmtKind::Local(name(&format!("a{i}")), None)))
            .collect()
    };
    assert_eq!(chunk(locals(200), "lua51"), vec![]);
    let over = chunk(locals(201), "luajit");
    assert!(over[0].message.contains("201 locals"), "{over:#?}");
    let captured = |n: usize| {
        let mut stmts = locals(n);
        let items = (0..n)
            .map(|i| Field::Item(Expr::name(&name(&format!("a{i}")), sp(0, 0))))
            .collect();
        let mut body = Block::default();
        body.end(LastKind::Return(lua(Lua::Table(items))), sp(0, 0));
        let f = Function {
            params: vec![],
            body,
            span: sp(5, 8),
        };
        stmts.push(stmt(StmtKind::LocalFunction(name("g"), f)));
        stmts
    };
    assert_eq!(chunk(captured(60), "lua51"), vec![]);
    let over = chunk(captured(61), "lua51");
    assert_eq!(over.len(), 1, "{over:#?}");
    assert_eq!(over[0].span, sp(5, 8));
    assert!(over[0].message.contains("61 upvalues"), "{over:#?}");
}

#[test]
fn lua51_counts_every_constant_together() {
    let numbers = |n: usize| {
        let items = (0..n)
            .map(|i| Field::Item(lua(Lua::Number(format!("{i}.5")))))
            .collect();
        vec![stmt(StmtKind::Local(
            name("t"),
            Some(lua(Lua::Table(items))),
        ))]
    };
    assert_eq!(chunk(numbers(262_143), "lua51"), vec![]);
    let over = chunk(numbers(262_144), "lua51");
    assert!(over[0].message.contains("262144 constants"), "{over:#?}");
}

#[test]
fn too_many_locals_are_refused_at_the_one_past_the_limit() {
    // the parameter is the first local of the function
    let lets = |n: u32| {
        let stmts = (0..n)
            .map(|i| {
                let value = at(ExprKind::Lit(Lit::Int(0)), 100 + i, 101 + i);
                let_(&format!("x{i}"), value)
            })
            .collect();
        vec![value("f", lambda(&["_"], block(stmts, int(0))))]
    };
    assert!(emit("lua51", lets(199)).is_ok());
    let found = refusals("lua51", lets(200));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].1, (299, 300));
    assert!(
        found[0].0.contains("201 locals in scope at once"),
        "{found:#?}"
    );
    assert!(found[0].0.contains("Lua 5.1.5 (the profile `lua-5.1`)"));
}

#[test]
fn too_many_upvalues_are_refused_at_the_function() {
    let captures = |n: usize| {
        let stmts = (0..n).map(|i| let_(&format!("x{i}"), int(0))).collect();
        let tuple = ex(ExprKind::Tuple(
            (0..n).map(|i| local(&format!("x{i}"))).collect(),
        ));
        let inner = at(lambda(&["_"], tuple).kind, 500, 510);
        vec![value("f", lambda(&["_"], block(stmts, inner)))]
    };
    assert!(emit("lua51", captures(60)).is_ok());
    let found = refusals("lua51", captures(61));
    assert_eq!(found, vec![(found[0].0.clone(), (500, 510))]);
}

#[test]
fn luajit_counts_numbers_apart() {
    let floats = |n: u32| {
        let items = (0..n)
            .map(|i| ex(ExprKind::Lit(Lit::Float(f64::from(i) + 0.5))))
            .collect();
        let list = ex(ExprKind::List(items));
        vec![value_at("big", list, 7, 9)]
    };
    assert!(emit("luajit", floats(65_536)).is_ok());
    let found = refusals("luajit", floats(65_537));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].1, (7, 9));
    assert!(
        found[0].0.contains("65537 constants of a kind"),
        "{found:#?}"
    );
    // one function of Lua 5.1 holds four times as many
    assert!(emit("lua51", floats(65_537)).is_ok());
}
