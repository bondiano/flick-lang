//! The compile-time limits of the dialect's VM, counted on the generated
//! program: each function's locals in scope at once, its upvalues, its
//! constants and the registers of its frame. Code over a limit is refused
//! at its Flick source instead of failing to load on the host.
//!
//! The counts follow the compilers of Lua 5.1 (`lparser.c`, `lcode.c`) and
//! `LuaJIT` (`lj_parse.c`), and are never below what they count:
//!
//! - A local is in scope from after its statement to the end of its block;
//!   parameters from the start of the function; a numeric `for` adds three
//!   hidden locals and its variable, a generic one three and its two.
//! - A name a function reads from an enclosing function is an upvalue of it
//!   and of every function between, which passes it on.
//! - Constants are the distinct strings (a global's name, a field's key)
//!   and numbers of a function; Lua 5.1 also counts `nil`, `true` and `false`
//!   where an instruction reads them as constants, which is counted always.
//!   `LuaJIT` counts numbers apart from the other constants: strings, the
//!   functions defined inside and the templates of table constructors.
//! - An expression takes the registers its parts are evaluated into: a call
//!   its callee's (and on `LuaJIT` the frame link's), then one per argument
//!   after those of the arguments before; a table constructor at most fifty
//!   pending items. With the locals in scope they stay below `max_registers`.

use std::collections::BTreeSet;

use flick_ir::Span;

use super::Refusal;
use super::ast::{Block, Expr, ExprKind, Field, Function, LastKind, Name, Stmt, StmtKind};
use crate::profiles::Dialect;

/// The items of a table constructor Lua 5.1 holds in registers before it
/// stores them (`LFIELDS_PER_FLUSH`).
const PENDING_ITEMS: usize = 50;

/// What one function uses.
struct Frame<'a> {
    span: Span,
    /// Its locals in scope, innermost last.
    active: Vec<&'a str>,
    upvalues: BTreeSet<&'a str>,
    strings: BTreeSet<&'a str>,
    numbers: BTreeSet<&'a str>,
    primitives: BTreeSet<&'static str>,
    functions: usize,
    tables: usize,
    /// Whether its locals or its registers were refused already.
    refused_locals: bool,
    refused_registers: bool,
}

struct Counter<'a> {
    dialect: &'static Dialect,
    frames: Vec<Frame<'a>>,
    refusals: Vec<Refusal>,
}

/// What `chunk`, the body of a chunk's main function, has over the limits
/// of `dialect`; `span` is the source of the chunk.
pub(crate) fn check(chunk: &Block, span: Span, dialect: &'static Dialect) -> Vec<Refusal> {
    let mut counter = Counter {
        dialect,
        frames: Vec::new(),
        refusals: Vec::new(),
    };
    counter.function(&[], chunk, span);
    counter.refusals
}

impl<'a> Counter<'a> {
    fn frame(&mut self) -> &mut Frame<'a> {
        self.frames.last_mut().expect("a function")
    }

    fn refuse(&mut self, span: Span, message: String) {
        self.refusals.push(Refusal { message, span });
    }

    fn function(&mut self, params: &'a [Name], body: &'a Block, span: Span) {
        self.frames.push(Frame {
            span,
            active: Vec::new(),
            upvalues: BTreeSet::new(),
            strings: BTreeSet::new(),
            numbers: BTreeSet::new(),
            primitives: BTreeSet::new(),
            functions: 0,
            tables: 0,
            refused_locals: false,
            refused_registers: false,
        });
        for p in params {
            self.declare(p.as_str(), span);
        }
        self.block(body);
        let frame = self.frames.pop().expect("a function");
        self.limits(&frame);
    }

    /// The limits of a whole function: upvalues and constants.
    fn limits(&mut self, frame: &Frame<'_>) {
        let d = self.dialect;
        if frame.upvalues.len() > d.max_upvalues {
            self.refuse(
                frame.span,
                format!(
                    "this code needs a Lua function of {} upvalues, which {} (the profile `{}`) \
                     does not load: it allows {}",
                    frame.upvalues.len(),
                    d.title,
                    d.id,
                    d.max_upvalues
                ),
            );
        }
        let (numbers, others) = (
            frame.numbers.len(),
            frame.strings.len() + frame.functions + frame.tables,
        );
        let over = if d.constants_by_kind {
            numbers.max(others)
        } else {
            (numbers + frame.strings.len() + frame.primitives.len()).max(frame.functions)
        };
        if over > d.max_constants {
            let kind = if d.constants_by_kind {
                " of a kind"
            } else {
                ""
            };
            self.refuse(
                frame.span,
                format!(
                    "this code needs a Lua function of {over} constants{kind}, which {} (the \
                     profile `{}`) does not load: it allows {}",
                    d.title, d.id, d.max_constants
                ),
            );
        }
    }

    /// A new local in scope.
    fn declare(&mut self, name: &'a str, span: Span) {
        let d = self.dialect;
        let frame = self.frame();
        frame.active.push(name);
        let count = frame.active.len();
        if count > d.max_locals && !frame.refused_locals {
            frame.refused_locals = true;
            self.refuse(
                span,
                format!(
                    "this code needs {count} locals in scope at once in one Lua function, which \
                     {} (the profile `{}`) does not load: it allows {}",
                    d.title, d.id, d.max_locals
                ),
            );
        }
    }

    /// The registers `need` holds on top of the locals in scope.
    fn registers(&mut self, need: usize, span: Span) {
        let d = self.dialect;
        let frame = self.frame();
        let total = frame.active.len() + need;
        if total >= d.max_registers && !frame.refused_registers {
            frame.refused_registers = true;
            self.refuse(
                span,
                format!(
                    "this code needs {total} registers in one Lua function, which {} (the \
                     profile `{}`) does not load: a frame holds fewer than {}, and a call takes \
                     one per argument",
                    d.title, d.id, d.max_registers
                ),
            );
        }
    }

    fn block(&mut self, block: &'a Block) {
        let mark = self.frame().active.len();
        self.stmts(block);
        self.frame().active.truncate(mark);
    }

    /// The statements of `block` and what ends it, its locals left in scope.
    fn stmts(&mut self, block: &'a Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
        if let Some(last) = &block.last
            && let LastKind::Return(e) = &last.kind
        {
            self.eval(e, last.span);
        }
    }

    /// `e` evaluated at `span`: its registers, then what it reads.
    fn eval(&mut self, e: &'a Expr, span: Span) {
        let need = self.regs(e);
        self.registers(need, span);
        self.expr(e);
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        let span = stmt.span;
        match &stmt.kind {
            StmtKind::Local(name, value) => {
                if let Some(value) = value {
                    self.eval(value, span);
                }
                self.declare(name.as_str(), span);
            }
            StmtKind::LocalFunction(name, f) => {
                self.declare(name.as_str(), span);
                self.frame().functions += 1;
                self.function(&f.params, &f.body, f.span);
            }
            StmtKind::Assign(target, value) => {
                let need = self.regs(target) + self.regs(value);
                self.registers(need, span);
                self.expr(target);
                self.expr(value);
            }
            StmtKind::Call(call) => self.eval(call, span),
            StmtKind::Do(block) => self.block(block),
            StmtKind::While(c, block) => {
                self.eval(c, span);
                self.block(block);
            }
            StmtKind::Repeat(block, c) => {
                // the condition sees the locals of the block
                let mark = self.frame().active.len();
                self.stmts(block);
                self.eval(c, span);
                self.frame().active.truncate(mark);
            }
            StmtKind::If(arms, otherwise) => {
                for (c, block) in arms {
                    self.eval(c, span);
                    self.block(block);
                }
                if let Some(block) = otherwise {
                    self.block(block);
                }
            }
            StmtKind::For(name, start, stop, block) => {
                // the start, the limit and the step, each in a register
                let need = self.regs(start).max(1 + self.regs(stop)).max(3);
                self.registers(need, span);
                self.expr(start);
                self.expr(stop);
                self.scoped(
                    &["(for index)", "(for limit)", "(for step)", name.as_str()],
                    block,
                    span,
                );
            }
            StmtKind::Pairs(key, value, table, block) => {
                // `pairs(table)` returns three values into three registers
                let need = (self.dialect.call_slots + self.regs(table)).max(3);
                self.registers(need, span);
                self.global("pairs");
                self.expr(table);
                let names = [
                    "(for generator)",
                    "(for state)",
                    "(for control)",
                    key.as_str(),
                    value.as_str(),
                ];
                self.scoped(&names, block, span);
            }
        }
    }

    /// `block` with the loop's `names` in scope.
    fn scoped(&mut self, names: &[&'a str], block: &'a Block, span: Span) {
        let mark = self.frame().active.len();
        for name in names {
            self.declare(name, span);
        }
        self.block(block);
        self.frame().active.truncate(mark);
    }

    /// A global read: its name is a constant.
    fn global(&mut self, name: &'a str) {
        self.frame().strings.insert(name);
    }

    /// What `e` reads and the constants it has.
    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Nil => {
                self.frame().primitives.insert("nil");
            }
            ExprKind::Bool(b) => {
                self.frame()
                    .primitives
                    .insert(if *b { "true" } else { "false" });
            }
            ExprKind::Number(n) => {
                self.frame().numbers.insert(n);
            }
            ExprKind::Str(s) => {
                self.frame().strings.insert(s);
            }
            ExprKind::Name(name) => self.read(name.as_str()),
            ExprKind::Index(object, key) => {
                self.expr(object);
                self.expr(key);
            }
            ExprKind::Call(callee, args, _) => {
                self.expr(callee);
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Function(f) => {
                self.frame().functions += 1;
                let Function { params, body, span } = &**f;
                self.function(params, body, *span);
            }
            ExprKind::Table(fields) => {
                self.frame().tables += 1;
                for field in fields {
                    match field {
                        Field::Item(v) => self.expr(v),
                        Field::Pair(k, v) => {
                            self.expr(k);
                            self.expr(v);
                        }
                    }
                }
            }
            ExprKind::Binary(_, l, r) => {
                self.expr(l);
                self.expr(r);
            }
            ExprKind::Unary(_, inner) => self.expr(inner),
        }
    }

    /// A name read: a local, an upvalue of each function up to the one
    /// that declares it, or a global.
    fn read(&mut self, name: &'a str) {
        let top = self.frames.len() - 1;
        let Some(owner) = (0..=top)
            .rev()
            .find(|&i| self.frames[i].active.contains(&name))
        else {
            self.global(name);
            return;
        };
        for frame in &mut self.frames[owner + 1..] {
            frame.upvalues.insert(name);
        }
    }

    /// The registers evaluating `e` takes, its result's among them.
    fn regs(&self, e: &Expr) -> usize {
        match &e.kind {
            ExprKind::Nil
            | ExprKind::Bool(_)
            | ExprKind::Number(_)
            | ExprKind::Str(_)
            | ExprKind::Name(_)
            | ExprKind::Function(_) => 1,
            ExprKind::Index(object, key) => self.regs(object).max(1 + self.regs(key)),
            ExprKind::Call(callee, args, _) => {
                let slots = self.dialect.call_slots;
                let mut need = self.regs(callee).max(slots + args.len());
                for (i, arg) in args.iter().enumerate() {
                    need = need.max(slots + i + self.regs(arg));
                }
                need
            }
            ExprKind::Table(fields) => {
                let mut need = 1;
                for (i, field) in fields.iter().enumerate() {
                    let held = match field {
                        Field::Item(v) => i % PENDING_ITEMS + self.regs(v),
                        Field::Pair(k, v) => self.regs(k) + self.regs(v),
                    };
                    need = need.max(1 + held);
                }
                need
            }
            ExprKind::Binary(_, l, r) => self.regs(l).max(1 + self.regs(r)),
            ExprKind::Unary(_, inner) => self.regs(inner),
        }
    }
}
