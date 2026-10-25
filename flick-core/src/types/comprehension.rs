//! `for { clauses } yield e`: each `p <- source` takes the elements of an
//! `Iterable` source and skips those `p` does not match, a guard `if c`
//! skips the element unless `c`, and `let` scopes over the rest. The body
//! runs when the sequence is iterated, so it is pure; a lazy `Seq` holds
//! what it uses, a strict `[for …]` is a `List` built at once.

use std::mem;

use super::Checker;
use super::do_notation::bind_key;
use super::effects::Stage;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

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
        let mark = self.locals.len();
        let uses = self.uses_mark();
        let saved = mem::take(&mut self.runtime);
        let reason = if strict {
            "a `[for …]` list is built from pure clauses: bind the result of a runtime call with `let` before it"
        } else {
            "the clauses of `for` run when the sequence is iterated, outside this function: bind the result of a runtime call with `let` before it"
        };
        let outer = mem::replace(&mut self.stage, Stage::pure(reason));
        let clauses = node
            .nodes()
            .filter(|n| n.kind == K::Block)
            .flat_map(Node::nodes);
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
        let item = match (node.nodes().filter(|n| n.kind != K::Block).last(), item) {
            (Some(body), Some(item)) => {
                self.check(body, &item);
                item
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        self.stage = outer;
        self.runtime = saved;
        let held = self.held_since(uses, mark);
        self.locals.truncate(mark);
        if strict {
            Type::app(Type::prim("List"), item)
        } else {
            let seq = Type::app(Type::prim("Seq"), item);
            self.close_over(seq, held, expected.cloned(), "sequence", self.span(node))
        }
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
