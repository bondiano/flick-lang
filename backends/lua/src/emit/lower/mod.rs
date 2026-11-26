//! The pure core of the IR as structured Lua.
//!
//! An IR expression becomes a Lua expression; what needs statements (a
//! `match`, an `if`, a block) is lowered into the statements of the block
//! being built before it, and earlier operands are first moved into locals,
//! so that evaluation stays left to right. Lua 5.1 and `LuaJIT` evaluate the
//! operands of an operator, the arguments of a call and the items of a
//! table constructor left to right, which the lowering relies on.
//!
//! Values: Unit is `true`, so no value is `nil` and a table never has a hole.
//! A tuple or list is a table of its items from 1, a record a table of its
//! labels. A tagged constructor's value is a table `{_tag = tag, …}` of its
//! fields from 1, then the dictionaries it holds; an unboxed or untagged
//! one is its field, an enum constructor its literal. An instance is a
//! table of its methods; its superclasses (`$Class`) and the methods it
//! takes from defaults are computed at each read, as the JS target's getters
//! are. A module's top-level values are fields of its table: `M` for the
//! module being lowered, `ns[name]` for another. These are private to the
//! generated code; values crossing to the host need the value ABI.
//!
//! Only what Lua computes exactly as Flick does is lowered. Effects,
//! non-pure stages, foreign functions and the operators whose numeric or
//! string semantics belong to the Lua kernel are refused with `P012` at
//! their source; nothing is ever approximated or stubbed.

mod data;
mod decide;
mod expr;
mod foreign;
mod items;
mod lang;
mod ops;
mod pattern;

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_backend_support::Known;
use flick_ir::{Constructor, Expr as IrExpr, ItemKind, Program, Span};

use super::ast::{Block, Expr, ExprKind, Field, LastKind, Name, Stmt, StmtKind};
use super::names::{MODULE, NAMESPACE, Namer};
use super::{Refusal, Target};
use crate::kernel::{self, Table};
use crate::profiles::Dialect;

/// Why the effects of the IR are refused.
const RUNTIME: &str = "effects, services and tasks need the Lua runtime, which is not implemented";

/// Where the value of an expression goes.
#[derive(Debug, Clone)]
enum Dest {
    /// Returned from the function: the expression is in tail position.
    Return,
    /// Assigned to a local declared before.
    Into(Name),
    /// Nowhere: the expression runs for its effects.
    Drop,
}

/// Lowers the items of a program, one top-level definition at a time.
pub(crate) struct Lowering<'p> {
    program: &'p Program,
    target: &'p str,
    dialect: &'static Dialect,
    known: Known<'p>,
    /// Every top-level value, foreign function or service by module and name.
    items: BTreeMap<(&'p str, &'p str), &'p ItemKind>,
    /// Every constructor by module and name.
    constructors: BTreeMap<(&'p str, &'p str), &'p Constructor>,
    /// The package of each module, which its constructors' tags name.
    packages: BTreeMap<&'p str, Option<&'p str>>,
    /// The first name of each global path a foreign function binds, which
    /// no local may shadow.
    hosts: BTreeSet<&'p str>,
    /// What the Lua kernel implements.
    kernel: Table<'static>,
    /// The built-in modules, whose JS globals of the kernel are the Lua
    /// kernel's.
    builtin: &'p BTreeSet<&'p str>,
    /// The module of the item being lowered.
    module: &'p str,
    names: Namer,
    /// The locals a `Stmt::Var` declared: reads of them are not stable.
    vars: BTreeSet<String>,
    /// The dictionaries in scope, as `Dict::Given` names them.
    givens: Vec<(String, Name)>,
    /// The block being built.
    block: Block,
    /// How many functions the code being lowered is inside.
    functions: usize,
    refusals: Vec<Refusal>,
}

impl<'p> Lowering<'p> {
    pub(crate) fn new(
        program: &'p Program,
        target: &Target<'p>,
        builtin: &'p BTreeSet<&'p str>,
    ) -> Lowering<'p> {
        let mut items = BTreeMap::new();
        let mut constructors = BTreeMap::new();
        let mut packages = BTreeMap::new();
        let mut hosts = BTreeSet::new();
        for module in &program.modules {
            let m = module.name.as_str();
            packages.insert(m, module.package.as_deref());
            for item in &module.items {
                match &item.kind {
                    ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => {
                        items.insert((m, name.as_str()), &item.kind);
                    }
                    ItemKind::Foreign(f) => {
                        items.insert((m, f.name.as_str()), &item.kind);
                        if let Some(path) = f.binding.global_name() {
                            hosts.insert(path.split('.').next().unwrap_or(path));
                        }
                    }
                    ItemKind::Constructor(c) => {
                        constructors.insert((m, c.name.as_str()), c);
                    }
                    ItemKind::Destructure { .. } | ItemKind::Instance { .. } => {}
                }
            }
        }
        Lowering {
            program,
            target: target.name,
            dialect: target.dialect,
            known: Known::of(&program.lang),
            items,
            constructors,
            packages,
            hosts,
            kernel: kernel::table(),
            builtin,
            module: "",
            names: Namer::new(),
            vars: BTreeSet::new(),
            givens: Vec::new(),
            block: Block::default(),
            functions: 0,
            refusals: Vec::new(),
        }
    }

    /// The statements that define item `item` of module `module`.
    pub(crate) fn item(&mut self, module: usize, item: usize) -> Vec<Stmt> {
        let program = self.program;
        let m = &program.modules[module];
        self.module = &m.name;
        self.names = Namer::new();
        for host in &self.hosts {
            self.names.reserve(host);
        }
        self.vars.clear();
        self.givens.clear();
        self.functions = 0;
        let stmts = self.within(|g| g.item_kind(&m.items[item].kind, m.items[item].span));
        stmts.stmts
    }

    /// What was refused so far, in the order found, each once.
    pub(crate) fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }

    /// Refuses what the target cannot run, at `span`; the placeholder the
    /// lowering goes on with, so that one run reports every refusal.
    fn refuse(&mut self, span: Span, message: String) -> Expr {
        if !self
            .refusals
            .iter()
            .any(|r| r.span == span && r.message == message)
        {
            self.refusals.push(Refusal { message, span });
        }
        Expr::new(ExprKind::Nil, span)
    }

    /// Refuses `what`, which the target does not have yet because of `why`.
    fn unsupported(&mut self, span: Span, what: &str, why: &str) -> Expr {
        let message = format!(
            "{what} is not available on the {} target yet: {why}",
            self.target
        );
        self.refuse(span, message)
    }

    /// The statements `f` lowers, in a block of their own.
    fn within(&mut self, f: impl FnOnce(&mut Self)) -> Block {
        let saved = mem::take(&mut self.block);
        f(self);
        mem::replace(&mut self.block, saved)
    }

    /// The statements and value `f` lowers, in a block of their own.
    fn within_value(&mut self, f: impl FnOnce(&mut Self) -> Expr) -> (Block, Expr) {
        let saved = mem::take(&mut self.block);
        let value = f(self);
        (mem::replace(&mut self.block, saved), value)
    }

    /// A function body: what `f` lowers inside one more function.
    fn function_body(&mut self, f: impl FnOnce(&mut Self)) -> Block {
        self.functions += 1;
        let body = self.within(f);
        self.functions -= 1;
        body
    }

    fn push(&mut self, kind: StmtKind, span: Span) {
        self.block.push(Stmt::new(kind, span));
    }

    /// The statements of `block`, and what ends it, in the current block.
    fn inline(&mut self, block: Block) {
        self.block.append(block);
    }

    /// A local holding `value`.
    fn local(&mut self, hint: &str, value: Expr) -> Name {
        let name = self.names.fresh(hint);
        let span = value.span;
        self.push(StmtKind::Local(name.clone(), Some(value)), span);
        name
    }

    /// `value` as a name the code can read again: itself when it is a
    /// local that does not change, otherwise a new local holding it.
    fn subject(&mut self, value: Expr) -> Name {
        match &value.kind {
            ExprKind::Name(name) if !self.vars.contains(name.as_str()) => name.clone(),
            _ => self.local("v", value),
        }
    }

    /// Whether evaluating `e` later gives what evaluating it now does: it
    /// reads no variable and calls nothing.
    fn stable(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Nil
            | ExprKind::Bool(_)
            | ExprKind::Number(_)
            | ExprKind::Str(_)
            | ExprKind::Function(_) => true,
            ExprKind::Name(name) => !self.vars.contains(name.as_str()),
            ExprKind::Index(object, key) => self.stable(object) && self.stable(key),
            ExprKind::Binary(_, l, r) => self.stable(l) && self.stable(r),
            ExprKind::Unary(_, inner) => self.stable(inner),
            ExprKind::Table(fields) => fields.iter().all(|f| match f {
                Field::Item(v) => self.stable(v),
                Field::Pair(k, v) => self.stable(k) && self.stable(v),
            }),
            ExprKind::Call(..) => false,
        }
    }

    /// `exprs`, evaluated in order: an operand whose code needs statements
    /// moves the operands before it into locals first.
    fn ordered(&mut self, exprs: &[&IrExpr]) -> Vec<Expr> {
        let mut out: Vec<Expr> = Vec::with_capacity(exprs.len());
        for e in exprs {
            let mark = self.block.stmts.len();
            let value = self.expr(e);
            if self.block.stmts.len() > mark {
                let mut spills = Vec::new();
                for prev in &mut out {
                    if !self.stable(prev) {
                        let name = self.names.fresh("t");
                        let span = prev.span;
                        let held = mem::replace(prev, Expr::name(&name, span));
                        spills.push(Stmt::new(StmtKind::Local(name, Some(held)), span));
                    }
                }
                self.block.stmts.splice(mark..mark, spills);
            }
            out.push(value);
        }
        out
    }

    /// Delivers `value` to `dest`.
    fn deliver(&mut self, value: Expr, dest: &Dest) {
        let span = value.span;
        match dest {
            Dest::Return => self.block.end(LastKind::Return(value), span),
            Dest::Into(name) => self.push(StmtKind::Assign(Expr::name(name, span), value), span),
            Dest::Drop => self.drop_value(value),
        }
    }

    /// Runs `value` for its effects: a call as a statement, a value that
    /// calls nothing not at all, anything else in a local of its own scope.
    fn drop_value(&mut self, value: Expr) {
        let span = value.span;
        if matches!(value.kind, ExprKind::Call(..)) {
            self.push(StmtKind::Call(value), span);
        } else if !self.stable(&value) {
            let name = Namer::unused();
            let mut block = Block::default();
            block.push(Stmt::new(StmtKind::Local(name, Some(value)), span));
            self.push(StmtKind::Do(block), span);
        }
    }

    /// The table of `module`'s top-level values.
    fn table(&self, module: &str, span: Span) -> Expr {
        if module == self.module {
            Expr::name(&Name::new(MODULE.to_owned()), span)
        } else {
            Expr::name(&Name::new(NAMESPACE.to_owned()), span).index(Expr::str(module, span))
        }
    }

    /// Unit, as a value.
    fn unit(span: Span) -> Expr {
        Expr::new(ExprKind::Bool(true), span)
    }
}
