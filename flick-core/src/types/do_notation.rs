//! `do { … }`: `p <- e` is `flat_map (fun p -> rest) e`, an expression
//! statement is bound to nothing, and a guard `if c` is
//! `if c then rest else nothing`. The methods are the Prelude's `Monad` and
//! `Alternative`, whatever names are in scope; the lowering reads the same
//! statements and the keys [`bind_key`] and [`guard_key`] give it.

use super::Checker;
use super::bindings::{let_parts, signature_name};
use super::ty::Type;
use crate::diagnostics::codes;

use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

/// Where the `flat_map` of a statement is recorded: the `<-` of a binding,
/// the empty range at the start of an expression statement.
pub fn bind_key(statement: &Node) -> (u32, u32) {
    match statement.child_tokens().find(|t| t.kind == K::LArrow) {
        Some(arrow) => (arrow.start, arrow.end),
        None => (statement.range().0, statement.range().0),
    }
}

/// Where the `nothing` of a guard is recorded: its `if`.
pub fn guard_key(guard: &Node) -> (u32, u32) {
    guard
        .child_tokens()
        .find(|t| t.kind == K::IfKw)
        .map_or(guard.range(), |t| (t.start, t.end))
}

impl Checker<'_> {
    pub fn do_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let Some(block) = node.nodes().find(|n| n.kind == K::Block) else {
            return Type::Error;
        };
        let statements: Vec<&Node> = block.nodes().collect();
        let mark = self.cx.locals.len();
        let ty = self.do_rest(node, &statements, expected);
        self.cx.locals.truncate(mark);
        self.sugar.push((node.range(), ty.clone()));
        ty
    }

    pub(super) fn do_rest(
        &mut self,
        node: &Node,
        statements: &[&Node],
        expected: Option<&Type>,
    ) -> Type {
        let Some((&first, rest)) = statements.split_first() else {
            let end = node.range().1;
            self.error(
                codes::T019,
                Span::new(self.file, end, end),
                "a `do` block must end with an expression: its value is the value of the block",
            );
            return Type::Error;
        };
        match first.kind {
            K::Signature => {
                let name = signature_name(first, self.text);
                let decl = rest.first().copied().filter(|n| {
                    n.kind == K::LetDecl && let_parts(n, self.text).name.as_deref() == name
                });
                if let Some(decl) = decl {
                    self.local_let(decl, Some(first), &rest[1..]);
                    self.do_rest(node, &rest[1..], expected)
                } else {
                    self.lone_signature(first, false);
                    self.do_rest(node, rest, expected)
                }
            }
            K::LetDecl => {
                self.local_let(first, None, rest);
                self.do_rest(node, rest, expected)
            }
            K::GuardStmt => {
                if let Some(condition) = first.nodes().next() {
                    self.check(condition, &Type::lang(Lang::Bool));
                }
                let (start, end) = guard_key(first);
                let nothing = self.prelude_method(Lang::Nothing, Span::new(self.file, start, end));
                let ty = self.do_rest(node, rest, expected);
                self.expect(&nothing, &ty, self.span(first));
                ty
            }
            K::BindStmt | K::BindGroup if rest.is_empty() => {
                self.error(
                    codes::T019,
                    self.span(first),
                    "a `do` block must end with an expression, not a binding: add `pure x` after it",
                );
                Type::Error
            }
            _ if rest.is_empty() => match expected {
                Some(expected) => {
                    self.check(first, expected);
                    expected.clone()
                }
                None => self.infer(first),
            },
            K::BindGroup => self.do_group(node, first, rest, expected),
            _ => self.do_bind(node, first, rest, expected),
        }
    }

    /// `flat_map (fun pattern -> rest) value`, checked as that application.
    fn do_bind(
        &mut self,
        node: &Node,
        statement: &Node,
        rest: &[&Node],
        expected: Option<&Type>,
    ) -> Type {
        let (pattern, value, lambda) = if statement.kind == K::BindStmt {
            let mut children = statement.nodes();
            (children.next(), children.next(), statement)
        } else {
            (None, Some(statement), node)
        };
        let (start, end) = bind_key(statement);
        let span = Span::new(self.file, start, end);
        let method = self.prelude_method(Lang::FlatMap, span);
        let Type::Fun {
            param: continuation,
            result,
            ..
        } = self.whnf(&method)
        else {
            return Type::Error;
        };
        let applied = self.instantiate(&result, span);
        let Type::Fun {
            param: monadic,
            result,
            ..
        } = self.whnf(&applied)
        else {
            return Type::Error;
        };
        let ty = self.instantiate(&result, span);
        if let Some(expected) = expected {
            self.expect(&ty, expected, self.span(statement));
        }
        match value {
            Some(value) if self.unknown_monad(&monadic) => {
                let actual = self.infer(value);
                let at = self.span(value);
                if self.subsume(&actual, &monadic, at).is_err() {
                    let shown = self.show(&actual);
                    self.error(
                        codes::T003,
                        at,
                        format!(
                            "a statement of a `do` block is a value of its monad, as `Some x`, `[x]` or `task {{ … }}`; this one has type `{shown}`"
                        ),
                    );
                }
            }
            Some(value) => self.check(value, &monadic),
            None => return Type::Error,
        }
        if statement.kind != K::BindStmt {
            self.sugar.push((statement.range(), *monadic));
        }
        let body = |c: &mut Self, result: Option<Type>| c.do_rest(node, rest, result.as_ref());
        self.function_with(&[pattern], body, Some(&continuation), lambda);
        ty
    }

    /// Whether `m a` does not know its `m` yet, so a mismatch would show `_ _`.
    fn unknown_monad(&self, monadic: &Type) -> bool {
        match self.whnf(monadic) {
            Type::App { head, .. } => matches!(self.whnf(&head), Type::Meta(_)),
            _ => false,
        }
    }

    /// A method of a Prelude class at `span`, instantiated, with the
    /// dictionary its class needs recorded there.
    pub(super) fn prelude_method(&mut self, method: Lang, span: Span) -> Type {
        let entity = lang::item(method).entity();
        let Some(scheme) = self.env.value(&entity).cloned() else {
            return Type::Error;
        };
        let ty = self.instantiate(&scheme, span);
        self.record_entity(&entity, &ty, span);
        ty
    }
}
