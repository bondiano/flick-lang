//! `for { clauses } yield e`: each `p <- source` takes the elements of an
//! `Iterable` source and skips those `p` does not match, a guard `if c`
//! skips the element unless `c`, and `let` scopes over the rest. The body
//! runs when the sequence is iterated, so it makes Sync calls alone, reading
//! the services of the stage that built it; a lazy `Seq` holds what it
//! uses, a strict `[for …]` is a `List` built at once.

use std::mem;

use super::Checker;
use super::do_notation::bind_key;
use super::effects::{Site, Stage};
use super::ty::{Row, Type};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The services of a stage that reads atoms.
fn reader() -> Type {
    let read = Type::Con {
        module: "Atom".to_owned(),
        name: "Read".to_owned(),
    };
    Type::Row(Row {
        entries: vec![("Atom.Read".to_owned(), read)],
        tail: None,
    })
}

/// The clauses of a `for` and its element or `do` body.
pub fn for_parts(node: &Node) -> (impl Iterator<Item = &Node>, Option<&Node>) {
    let mut parts = node.nodes();
    let clauses = parts.next().into_iter().flat_map(Node::nodes);
    (clauses, parts.next())
}

/// Whether a `for` runs a `do` body instead of yielding elements.
pub fn is_loop(node: &Node) -> bool {
    node.has_token(K::DoKw)
}

/// The `for` of `[for …]`, a strict comprehension, if `list` is one.
pub fn comprehension(list: &Node) -> Option<&Node> {
    let mut items = list.nodes();
    match (items.next(), items.next()) {
        (Some(only), None) if only.kind == K::ForExpr => Some(only),
        _ => None,
    }
}

impl Checker<'_> {
    pub fn for_expr(&mut self, node: &Node, strict: bool, expected: Option<&Type>) -> Type {
        if is_loop(node) {
            return self.for_loop(node, expected);
        }
        let mark = self.locals.len();
        let uses = self.uses_mark();
        let saved = mem::take(&mut self.runtime);
        let (reason, sync_reason) = if strict {
            (
                "a `[for …]` list is built from pure clauses: bind the result of a runtime call with `let` before it",
                "a `[for …]` list makes Sync calls alone (`->{Atom.Read;}`): bind the result of a runtime call with `let` before it",
            )
        } else {
            (
                "the clauses of `for` run when the sequence is iterated, outside this function: bind the result of a runtime call with `let` before it",
                "the clauses of `for` run when the sequence is iterated, where only Sync calls (`->{Atom.Read;}`) run: bind the result of a runtime call with `let` before it",
            )
        };
        // a stage that may read atoms lends its reader to the clauses
        let stage = if self.stage.pure.is_none() {
            let mut stage = self.bounded_stage(reader(), Type::empty_row(), Site::Inferred);
            stage.sync = stage.pure.is_none();
            stage.only_sync = Some(sync_reason);
            stage
        } else {
            Stage::pure(reason)
        };
        let outer = mem::replace(&mut self.stage, stage);
        let (clauses, element) = for_parts(node);
        for clause in clauses {
            self.for_clause(clause);
        }
        let item = expected.and_then(|e| {
            if strict {
                self.list_item(e)
            } else {
                self.seq_item(e)
            }
        });
        let item = match (element, item) {
            (Some(body), Some(item)) => {
                self.check(body, &item);
                item
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        let inner = mem::replace(&mut self.stage, outer);
        self.runtime = saved;
        let mut held = self.held_since(uses, mark);
        let mut region = None;
        if inner.sync && inner.calls > 0 {
            let span = self.span(node);
            self.perform(&reader(), &Type::empty_row(), span);
            if !strict {
                self.typing.sync_generators.insert(node.range());
                let reader = self.reader_region(span);
                held.push((None, reader.clone()));
                region = Some(reader);
            }
        }
        self.locals.truncate(mark);
        if strict {
            return Type::app(Type::prim("List"), item);
        }
        let seq = Type::app(Type::prim("Seq"), item);
        let seq = self.close_over(seq, held, expected.cloned(), "sequence", self.span(node));
        match region {
            Some(region) if expected.is_none() => Self::read_through(seq, &region),
            _ => seq,
        }
    }

    /// `for { clauses } do body`: the body runs for each element, here and
    /// now, so the clauses and the body may make runtime calls.
    fn for_loop(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let mark = self.locals.len();
        let (clauses, body) = for_parts(node);
        for clause in clauses {
            self.for_clause(clause);
        }
        let unit = Type::prim("Unit");
        if let Some(body) = body {
            self.check(body, &unit);
        }
        self.locals.truncate(mark);
        if let Some(expected) = expected {
            self.expect(&unit, expected, self.span(node));
        }
        unit
    }

    fn for_clause(&mut self, clause: &Node) {
        match clause.kind {
            K::BindStmt => {
                let mut children = clause.nodes();
                let (pattern, source) = (children.next(), children.next());
                let (start, end) = bind_key(clause);
                let span = Span::new(self.file, start, end);
                let iter = self.prelude_method("iter", span);
                let Type::Fun { param, result, .. } = self.whnf(&iter) else {
                    return;
                };
                if let Some(source) = source {
                    self.check(source, &param);
                }
                let elements = self.instantiate(&result, span);
                let element = self.seq_item(&elements).unwrap_or(Type::Error);
                // a pattern that does not match skips the element
                if let Some(pattern) = pattern {
                    self.bind_pattern(pattern, &element);
                }
            }
            K::GuardStmt => {
                if let Some(condition) = clause.nodes().next() {
                    self.check(condition, &Type::prim("Bool"));
                }
            }
            K::LetDecl => self.local_let(clause, None, &[]),
            _ => self.error(
                codes::T019,
                self.span(clause),
                "a clause of `for` is `x <- source`, a guard `if c` or a `let`",
            ),
        }
    }

    /// `a` of `Seq a`.
    pub fn seq_item(&self, t: &Type) -> Option<Type> {
        match self.whnf(t) {
            Type::App { head, arg } if self.whnf(&head).is_prim("Seq") => Some(*arg),
            _ => None,
        }
    }
}
