//! Lua text from the structured program, with the Flick span of each line.
//!
//! The text is a `flick-pretty` document printed once: a list that does not
//! fit on its line breaks one item per line, and blocks are indented by two
//! spaces. A cursor before every statement and expression reports where it
//! lands; the first node a line starts with gives that line its span. Lua
//! reports a runtime error by line, so lines are what a map needs.
//!
//! Parentheses follow Lua 5.1's operator priorities (`lparser.c`): an
//! operand is in parentheses only where it would otherwise parse
//! differently, and an object called or indexed only when it is not a name,
//! an index or a call. A chain of `and` or of `or` needs none, whichever way
//! it nests: both are associative.

use std::fmt::Write as _;

use flick_ir::Span;
use flick_pretty::{Document, DocumentArena, Position, docvec};

use super::ast::{
    BinOp, Block, Expr, ExprKind, Field, Function, LastKind, Results, Stmt, StmtKind, UnOp,
};
use super::names::is_identifier;

/// The column lists break at.
const WIDTH: usize = 100;

/// The priority of unary operators; a binary operator binds its operands
/// with a left and a right priority.
const UNARY: u8 = 8;

/// Lua text and, for each line a node starts, 0-based, the span of the
/// first one.
#[derive(Debug, Clone, PartialEq)]
pub struct Printed {
    pub code: String,
    pub lines: Vec<(u32, Span)>,
}

/// `block` as the text of a chunk.
pub fn chunk(block: &Block) -> Printed {
    let arena = DocumentArena::new();
    let mut p = Printer {
        a: &arena,
        spans: Vec::new(),
    };
    let doc = p.stmts(block);
    let mut code = String::new();
    let mut lines: Vec<(u32, Span)> = Vec::new();
    let spans = p.spans;
    let mut observe = |mark: usize, at: Position| {
        let line = u32::try_from(at.line).unwrap_or(u32::MAX);
        if lines.last().is_none_or(|&(l, _)| l < line) {
            lines.push((line, spans[mark]));
        }
    };
    // writing into a `String` cannot fail
    let _ =
        docvec![&arena, doc, arena.line()].pretty_print_observed(WIDTH, &mut code, &mut observe);
    Printed { code, lines }
}

struct Printer<'d> {
    a: &'d DocumentArena<'d>,
    /// The span of each cursor, by its mark.
    spans: Vec<Span>,
}

/// The left and right priorities of a binary operator.
fn priority(op: BinOp) -> (u8, u8) {
    match op {
        BinOp::Or => (1, 1),
        BinOp::And => (2, 2),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => (3, 3),
        BinOp::Concat => (5, 4),
        BinOp::Add | BinOp::Sub => (6, 6),
        BinOp::Mul | BinOp::Div | BinOp::Mod => (7, 7),
        BinOp::Pow => (10, 9),
    }
}

fn symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "or",
        BinOp::And => "and",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::Ne => "~=",
        BinOp::Concat => "..",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "^",
    }
}

/// Whether `e` can be called or indexed as it is written.
fn is_prefix(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Call(..)
    )
}

/// Whether a statement's text starts with `(`, which Lua would read as a
/// call of the statement before it.
fn starts_with_paren(stmt: &Stmt) -> bool {
    let StmtKind::Call(e) = &stmt.kind else {
        return false;
    };
    let mut root = e;
    while let ExprKind::Call(callee, ..) | ExprKind::Index(callee, _) = &root.kind {
        if !is_prefix(callee) {
            return true;
        }
        root = callee;
    }
    false
}

/// A Lua string literal of the bytes of `text`: printable ASCII as it is,
/// other bytes of UTF-8 too, and control characters as escapes.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // three digits, so a digit after it is not read as part of it
            c if c.is_ascii_control() => {
                let _ = write!(out, "\\{:03}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl<'d> Printer<'d> {
    fn mark(&mut self, span: Span) -> Document<'d> {
        self.spans.push(span);
        self.a.cursor(self.spans.len() - 1)
    }

    fn text(&self, text: &str) -> Document<'d> {
        self.a.text(self.a.alloc_str(text))
    }

    /// The statements of a block, one per line.
    fn stmts(&mut self, block: &Block) -> Document<'d> {
        let a = self.a;
        let mut docs = Vec::with_capacity(block.stmts.len() + 1);
        for (i, stmt) in block.stmts.iter().enumerate() {
            let mut doc = self.stmt(stmt);
            if block.stmts.get(i + 1).is_some_and(starts_with_paren) {
                doc = docvec![a, doc, ";"];
            }
            docs.push(doc);
        }
        if let Some(last) = &block.last {
            let mark = self.mark(last.span);
            let doc = match &last.kind {
                LastKind::Return(e) => docvec![a, mark, "return ", self.one(e)],
                LastKind::Break => docvec![a, mark, "break"],
            };
            docs.push(doc);
        }
        a.join(docs, a.line())
    }

    /// `head`, the block indented, and `end`: on one line when empty.
    fn body(&mut self, head: Document<'d>, block: &Block, end: &'d str) -> Document<'d> {
        let a = self.a;
        if block.is_empty() {
            return docvec![a, head, " ", end];
        }
        let inner = self.stmts(block);
        docvec![
            a,
            head,
            docvec![a, a.line(), inner].nest(a, 2),
            a.line(),
            end
        ]
    }

    fn stmt(&mut self, stmt: &Stmt) -> Document<'d> {
        let a = self.a;
        let mark = self.mark(stmt.span);
        let doc = match &stmt.kind {
            StmtKind::Args(names) => {
                let names = names.iter().map(|n| self.text(n.as_str()));
                docvec![a, "local ", a.join(names, a.text(", ")), " = ..."]
            }
            StmtKind::Local(name, None) => docvec![a, "local ", self.text(name.as_str())],
            StmtKind::Local(name, Some(value)) => {
                docvec![
                    a,
                    "local ",
                    self.text(name.as_str()),
                    " = ",
                    self.one(value)
                ]
            }
            StmtKind::LocalFunction(name, f) => {
                let head = docvec![
                    a,
                    "local function ",
                    self.text(name.as_str()),
                    self.params(f)
                ];
                self.body(head, &f.body, "end")
            }
            StmtKind::Assign(target, value) => {
                docvec![a, self.prefix(target), " = ", self.one(value)]
            }
            StmtKind::Call(call) => self.expr(call),
            StmtKind::Do(block) => self.body(a.text("do"), block, "end"),
            StmtKind::While(c, block) => {
                let head = docvec![a, "while ", self.expr(c), " do"];
                self.body(head, block, "end")
            }
            StmtKind::Repeat(block, c) => {
                let until = docvec![a, "until ", self.expr(c)];
                let inner = self.stmts(block);
                docvec![
                    a,
                    "repeat",
                    docvec![a, a.line(), inner].nest(a, 2),
                    a.line(),
                    until
                ]
            }
            StmtKind::If(arms, otherwise) => self.if_stmt(arms, otherwise.as_ref()),
            StmtKind::For(name, start, stop, block) => {
                let head = docvec![
                    a,
                    "for ",
                    self.text(name.as_str()),
                    " = ",
                    self.expr(start),
                    ", ",
                    self.expr(stop),
                    " do"
                ];
                self.body(head, block, "end")
            }
            StmtKind::Pairs(key, value, table, block) => {
                let head = docvec![
                    a,
                    "for ",
                    self.text(key.as_str()),
                    ", ",
                    self.text(value.as_str()),
                    " in pairs(",
                    self.one(table),
                    ") do"
                ];
                self.body(head, block, "end")
            }
        };
        docvec![a, mark, doc]
    }

    fn if_stmt(&mut self, arms: &[(Expr, Block)], otherwise: Option<&Block>) -> Document<'d> {
        let a = self.a;
        let mut parts = Vec::with_capacity(arms.len() + 2);
        for (i, (c, block)) in arms.iter().enumerate() {
            let keyword = if i == 0 { "if " } else { "elseif " };
            let head = docvec![a, keyword, self.expr(c), " then"];
            parts.push(self.branch(head, block));
        }
        if let Some(block) = otherwise {
            parts.push(self.branch(a.text("else"), block));
        }
        parts.push(a.text("end"));
        a.join(parts, a.line())
    }

    /// `head` and its block indented under it, without the `end`.
    fn branch(&mut self, head: Document<'d>, block: &Block) -> Document<'d> {
        let a = self.a;
        if block.is_empty() {
            return head;
        }
        let inner = self.stmts(block);
        docvec![a, head, docvec![a, a.line(), inner].nest(a, 2)]
    }

    fn params(&self, f: &Function) -> Document<'d> {
        let a = self.a;
        let names = f.params.iter().map(|p| a.text(a.alloc_str(p.as_str())));
        docvec![a, "(", a.join(names, a.text(", ")), ")"]
    }

    /// `e` where a list wants one value: a call that may return another
    /// number of values in parentheses.
    fn one(&mut self, e: &Expr) -> Document<'d> {
        let doc = self.expr(e);
        if matches!(e.kind, ExprKind::Call(_, _, Results::Any)) {
            return docvec![self.a, "(", doc, ")"];
        }
        doc
    }

    /// `e` called or indexed.
    fn prefix(&mut self, e: &Expr) -> Document<'d> {
        let doc = self.expr(e);
        if is_prefix(e) {
            doc
        } else {
            docvec![self.a, "(", doc, ")"]
        }
    }

    /// `e` as an operand, in parentheses when `parens` says so.
    fn operand(&mut self, e: &Expr, parens: bool) -> Document<'d> {
        let doc = self.expr(e);
        if parens {
            docvec![self.a, "(", doc, ")"]
        } else {
            doc
        }
    }

    /// A list of `items` between `open` and `close`, one per line with a
    /// trailing comma when it does not fit; `trailing` is false where Lua
    /// takes no comma after the last item (arguments).
    fn list(
        &self,
        open: &'d str,
        items: Vec<Document<'d>>,
        close: &'d str,
        trailing: bool,
    ) -> Document<'d> {
        let a = self.a;
        if items.is_empty() {
            return docvec![a, open, close];
        }
        let inner =
            docvec![a, a.break_("", ""), a.join(items, a.break_(",", ", "))].nest_if_broken(a, 2);
        let end = if trailing {
            a.break_(",", "")
        } else {
            a.break_("", "")
        };
        docvec![a, open, inner, end, close].group(a)
    }

    fn expr(&mut self, e: &Expr) -> Document<'d> {
        let a = self.a;
        let mark = self.mark(e.span);
        let doc = match &e.kind {
            ExprKind::Nil => a.text("nil"),
            ExprKind::Bool(b) => a.text(if *b { "true" } else { "false" }),
            ExprKind::Number(n) => self.text(n),
            ExprKind::Str(s) => self.text(&string(s)),
            ExprKind::Name(n) => self.text(n.as_str()),
            ExprKind::Index(object, key) => {
                let object = self.prefix(object);
                match &key.kind {
                    ExprKind::Str(k) if is_identifier(k) => docvec![a, object, ".", self.text(k)],
                    _ => docvec![a, object, "[", self.expr(key), "]"],
                }
            }
            ExprKind::Call(callee, args, _) => {
                let callee = self.prefix(callee);
                let args = self.items(args);
                docvec![a, callee, self.list("(", args, ")", false)]
            }
            ExprKind::Function(f) => {
                let head = docvec![a, "function", self.params(f)];
                self.body(head, &f.body, "end")
            }
            ExprKind::Table(fields) => {
                let mut items = Vec::with_capacity(fields.len());
                for (i, field) in fields.iter().enumerate() {
                    items.push(match field {
                        Field::Item(v) if i + 1 == fields.len() => self.one(v),
                        Field::Item(v) => self.expr(v),
                        Field::Pair(key, v) => {
                            let key = match &key.kind {
                                ExprKind::Str(k) if is_identifier(k) => self.text(k),
                                _ => docvec![a, "[", self.expr(key), "]"],
                            };
                            docvec![a, key, " = ", self.expr(v)]
                        }
                    });
                }
                self.list("{", items, "}", true)
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let (left, right) = priority(*op);
                let l = match &lhs.kind {
                    ExprKind::Binary(inner, ..) => left > priority(*inner).1,
                    ExprKind::Unary(..) => left > UNARY,
                    _ => false,
                };
                // `and` and `or` are associative: a chain nests no deeper
                let r = match &rhs.kind {
                    ExprKind::Binary(inner, ..) if inner == op => {
                        !matches!(op, BinOp::And | BinOp::Or) && priority(*inner).0 <= right
                    }
                    ExprKind::Binary(inner, ..) => priority(*inner).0 <= right,
                    _ => false,
                };
                let l = self.operand(lhs, l);
                let r = self.operand(rhs, r);
                docvec![a, l, " ", symbol(*op), " ", r]
            }
            ExprKind::Unary(op, inner) => {
                let parens = match &inner.kind {
                    ExprKind::Binary(op, ..) => priority(*op).0 <= UNARY,
                    // `- -x` would start a comment without the space
                    ExprKind::Unary(UnOp::Neg, _) => *op == UnOp::Neg,
                    _ => false,
                };
                let inner = self.operand(inner, parens);
                match op {
                    UnOp::Not => docvec![a, "not ", inner],
                    UnOp::Neg => docvec![a, "-", inner],
                    UnOp::Len => docvec![a, "#", inner],
                }
            }
        };
        docvec![a, mark, doc]
    }

    /// The documents of an expression list, the last one adjusted to one
    /// value.
    fn items(&mut self, items: &[Expr]) -> Vec<Document<'d>> {
        let mut docs = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            docs.push(if i + 1 == items.len() {
                self.one(item)
            } else {
                self.expr(item)
            });
        }
        docs
    }
}
