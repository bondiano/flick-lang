//! `let mut` variables and `name = value`. A variable belongs to the
//! function that declares it: a function, `task`, lazy `for` or arm of a
//! handler of operations inside it neither reads nor assigns it, so the
//! function stays pure from the outside (`docs/formal/elaboration.md` §3.5).
//! Its type is fixed where it is declared, and an assigned value is checked
//! against it, as what `Ref.set` writes: what a `scope` owns cannot reach a
//! variable from outside the scope. Sequential loops and `try` keep completed
//! assignments; strict comprehensions do not assign an outer variable.

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Local};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::bindings::LetParts;
use crate::types::convert::is_type;
use crate::types::env::Found;
use crate::types::lang::Lang;

/// How a local name was bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Binding {
    /// A parameter, a pattern variable, a function or a name of `scope`.
    #[default]
    Other,
    /// `let name = value`: `mut` would make it a variable.
    Let,
    /// A `let` clause of `for`, whose syntax does not allow `mut`.
    Clause,
    /// `let mut name = value`.
    Mut,
}

/// What runs apart from the function around it.
#[derive(Debug, Clone, Copy)]
pub enum Apart {
    Function,
    Task,
    Sequence,
    HandlerArm,
}

#[derive(Debug, Default)]
pub struct Frames {
    /// The locals below each enclosing function, `task`, sequence or arm.
    apart: Vec<(usize, Apart)>,
    /// The locals below each enclosing strict comprehension.
    settled: Vec<usize>,
    loops: Vec<(usize, usize, Type, bool)>,
}

impl Checker<'_> {
    pub fn enter_loop(&mut self, result: Type, value: bool) {
        self.cx.mutation.loops.push((
            self.cx.mutation.apart.len(),
            self.cx.mutation.settled.len(),
            result,
            value,
        ));
    }
    pub fn leave_loop(&mut self) {
        self.cx.mutation.loops.pop();
    }

    pub fn control_loop(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let result = if node.kind == K::WhileExpr {
            Type::lang(Lang::Unit)
        } else {
            expected.cloned().unwrap_or_else(|| self.fresh())
        };
        let mut children = node.nodes();
        self.enter_loop(result.clone(), node.kind == K::LoopExpr);
        if node.kind == K::WhileExpr
            && let Some(c) = children.next()
        {
            self.check(c, &Type::lang(Lang::Bool));
        }
        if let Some(body) = children.next() {
            self.check(body, &Type::lang(Lang::Unit));
        }
        self.leave_loop();
        if let Some(expected) = expected {
            self.expect(&result, expected, self.span(node));
        }
        result
    }

    pub fn loop_exit(&mut self, node: &Node) -> Type {
        let target = self
            .cx
            .mutation
            .loops
            .last()
            .cloned()
            .filter(|(depth, barriers, _, _)| {
                *depth == self.cx.mutation.apart.len()
                    && *barriers == self.cx.mutation.settled.len()
            });
        if let Some((_, _, ty, value)) = target {
            if node.kind == K::BreakExpr {
                if let Some(e) = node.nodes().next() {
                    if !value {
                        self.error(
                            codes::T071,
                            self.span(node),
                            "only `loop` accepts `break value`",
                        );
                    }
                    self.check(e, &ty);
                } else {
                    self.expect(&Type::lang(Lang::Unit), &ty, self.span(node));
                }
            }
        } else {
            self.error(
                codes::T071,
                self.span(node),
                "`break` and `continue` need an enclosing loop in the same function",
            );
        }
        self.fresh()
    }

    /// Code that runs apart from the function around it starts here; the
    /// locals bound so far are that function's.
    pub fn enter_apart(&mut self, what: Apart) {
        self.cx.mutation.apart.push((self.cx.locals.len(), what));
    }

    pub fn leave_apart(&mut self) {
        self.cx.mutation.apart.pop();
    }

    /// A strict comprehension starts here.
    pub fn enter_settled(&mut self) {
        self.cx.mutation.settled.push(self.cx.locals.len());
    }

    pub fn leave_settled(&mut self) {
        self.cx.mutation.settled.pop();
    }

    /// `FLICK-G005` when the local `index`, a `let mut` variable, is used
    /// from code that runs apart from its function.
    pub fn mutable_use(&mut self, index: usize, span: Span) -> bool {
        if self.cx.locals[index].binding != Binding::Mut {
            return true;
        }
        self.typing.mutable_reads.insert((span.start, span.end));
        let Some(&(_, what)) =
            (self.cx.mutation.apart.iter().rev()).find(|(below, _)| index < *below)
        else {
            return true;
        };
        let name = self.cx.locals[index].name.clone();
        let (inner, before) = match what {
            Apart::Function => ("A function", "the function"),
            Apart::Task => ("A `task`", "the `task`"),
            Apart::Sequence => ("A lazy `for`", "the `for`"),
            Apart::HandlerArm => ("An arm of a handler of operations", "the handler"),
        };
        let diagnostic = Diagnostic::error(
            codes::G005,
            format!(
                "{inner} cannot use `{name}`, a `let mut` variable of the function around it: it may run later, more than once or at the same time"
            ),
            span,
        )
        .with_fix(|| {
            (
                format!("bind the value with `let` before {before} and use that name"),
                Vec::new(),
            )
        });
        let diagnostic = match self.cx.locals[index].at {
            Some(at) => {
                let end = at + u32::try_from(name.len()).expect("source fits in u32");
                diagnostic.with_related(Span::new(self.file, at, end), "declared here")
            }
            None => diagnostic,
        };
        self.diagnostics.push(diagnostic);
        false
    }

    pub(super) fn contains_mutable(&self, node: &Node) -> bool {
        self.typing.mutable_reads.contains(&node.range())
            || node.nodes().any(|n| self.contains_mutable(n))
    }

    /// `let mut name = value`: one type, fixed here, which a signature or
    /// `: T` may give; it is not generalized.
    pub(super) fn local_mut(
        &mut self,
        decl: &Node,
        sig: Option<&Node>,
        parts: &LetParts,
        name: &str,
        at: u32,
    ) {
        let written = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .or(parts.header);
        self.store.level += 1;
        let ty = match written {
            Some(written) => {
                let ty = if sig.is_some() {
                    self.signature(written).0
                } else {
                    self.annotation(written)
                };
                if matches!(self.whnf(&ty), Type::Forall { .. }) {
                    let span = if sig.is_some() {
                        self.span(written)
                    } else {
                        self.span(decl)
                    };
                    self.error(codes::T013, span, format!("`{name}` is a `let mut` variable, which has one type: it cannot be polymorphic"));
                    if let Some(body) = parts.body {
                        self.infer(body);
                    }
                    self.store.level -= 1;
                    self.push_local(name, at, Type::Error);
                    if let Some(local) = self.cx.locals.last_mut() {
                        local.binding = Binding::Mut;
                    }
                    return;
                }
                if let Some(body) = parts.body {
                    self.check(body, &ty);
                }
                ty
            }
            None => parts.body.map_or(Type::Error, |b| self.infer(b)),
        };
        self.store.level -= 1;
        self.store.lower(&ty);
        self.push_local(name, at, ty);
        if let Some(local) = self.cx.locals.last_mut() {
            local.binding = Binding::Mut;
        }
    }

    /// `name = value`.
    pub fn assign(&mut self, node: &Node) -> Type {
        let unit = Type::lang(Lang::Unit);
        let mut children = node.nodes();
        let (Some(target), value) = (children.next(), children.next()) else {
            return unit;
        };
        let Some(token) = target.child_tokens().find(|t| t.kind == K::Ident) else {
            return unit;
        };
        let name = token.text(self.text).to_owned();
        let span = self.token_span(token);
        let generated = span.start >= self.generated;
        let found =
            (self.cx.locals.iter()).rposition(|l| l.name == name && l.generated == generated);
        let Some(index) = found.filter(|&i| self.cx.locals[i].binding == Binding::Mut) else {
            self.not_assignable(&name, span, found);
            if let Some(value) = value {
                self.infer(value);
            }
            return unit;
        };
        self.record_name(span, Ref::Local);
        if let Some(at) = self.cx.locals[index].at
            && !generated
        {
            let end = at + u32::try_from(name.len()).expect("source fits in u32");
            self.typing
                .binders
                .insert((span.start, span.end), (at, end));
        }
        self.cx.regions.uses.push(index);
        if self.mutable_use(index, span) {
            self.settled_assignment(index, &name, span);
        }
        let ty = self.cx.locals[index].ty.clone();
        self.note_type(span, &ty);
        if let Some(value) = value {
            self.check(value, &ty);
        }
        unit
    }

    /// `FLICK-T070` for an assignment of the local `index` inside a strict
    /// comprehension that started after it was declared.
    fn settled_assignment(&mut self, index: usize, name: &str, span: Span) {
        let Some(_) = (self.cx.mutation.settled.iter().rev()).find(|below| index < **below) else {
            return;
        };
        let place = "a strict comprehension: use `for … do` to update state, or build the value with a fold";
        self.error(
            codes::T070,
            span,
            format!("`{name}` is declared outside {place}"),
        );
    }

    /// `FLICK-T069` for `name = …` where `name` is not a `let mut` variable
    /// of this function; the local `found` is the name if it is one.
    fn not_assignable(&mut self, name: &str, span: Span, found: Option<usize>) {
        let diagnostic = match found.map(|i| self.cx.locals[i].clone()) {
            Some(Local {
                binding: Binding::Let,
                at: Some(at),
                generated: false,
                ..
            }) => {
                let end = at + u32::try_from(name.len()).expect("source fits in u32");
                Diagnostic::error(
                    codes::T069,
                    format!("`{name}` is not `let mut`, so it cannot be assigned"),
                    span,
                )
                .with_related(Span::new(self.file, at, end), "bound here")
                .with_safe_fix(|| {
                    (
                        format!("declare it with `let mut {name}`"),
                        vec![Edit {
                            span: Span::new(self.file, at, at),
                            replacement: "mut ".to_owned(),
                        }],
                    )
                })
            }
            Some(Local {
                binding: Binding::Clause,
                ..
            }) => Diagnostic::error(
                codes::T069,
                format!(
                    "`{name}` is a `for` clause binding and cannot be assigned; move a `let mut` binding into the loop body"
                ),
                span,
            ),
            Some(_) => Diagnostic::error(
                codes::T069,
                format!(
                    "`{name}` is not `let mut`: a parameter, a pattern variable or a function cannot be assigned; bind a new name with `let`"
                ),
                span,
            ),
            None if !matches!(self.env.find(Namespace::Value, name), Found::Missing) => {
                Diagnostic::error(
                    codes::T069,
                    format!(
                        "only a `let mut` variable of this function can be assigned, and `{name}` is a definition of a module"
                    ),
                    span,
                )
            }
            None => Diagnostic::error(
                codes::T069,
                format!("cannot find `{name}` to assign: declare it with `let mut {name} = …`"),
                span,
            )
            .with_fix(|| {
                (
                    format!("bind `{name}` with `let`"),
                    vec![Edit {
                        span: Span::new(self.file, span.start, span.start),
                        replacement: "let ".to_owned(),
                    }],
                )
            }),
        };
        self.diagnostics.push(diagnostic);
    }
}
