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
    /// A local `let` in syntax that does not allow `mut`.
    Restricted(&'static str),
    /// `let mut name = value`.
    Mut,
    /// `let lazy name = value`: forced with `await`, at most once.
    Lazy,
    /// `let lazy mut name = value`: a `let lazy` an assignment may replace.
    LazyMut,
}

/// What runs apart from the function around it.
#[derive(Debug, Clone, Copy)]
pub enum Apart {
    Function,
    Task,
    Sequence,
    HandlerArm,
}

#[derive(Debug, Clone)]
pub struct LoopFrame {
    pub label: Option<String>,
    pub apart_depth: usize,
    pub settled_depth: usize,
    pub result: Type,
    pub accepts_value: bool,
}

#[derive(Debug, Default)]
pub struct Frames {
    /// The locals below each enclosing function, `task`, sequence or arm.
    apart: Vec<(usize, Apart)>,
    /// The locals below each enclosing strict comprehension.
    settled: Vec<usize>,
    loops: Vec<LoopFrame>,
}

impl Checker<'_> {
    pub fn enter_loop_node(&mut self, node: &Node, result: Type, value: bool) {
        let label_token = node.child_tokens().find(|t| t.kind == K::Label);
        let label = label_token.map(|t| {
            let text = t.text(self.text);
            text.strip_prefix('\'').unwrap_or(text).to_owned()
        });
        if let Some(ref name) = label {
            let current_apart = self.cx.mutation.apart.len();
            let current_settled = self.cx.mutation.settled.len();
            if self.cx.mutation.loops.iter().any(|f| {
                f.apart_depth == current_apart
                    && f.settled_depth == current_settled
                    && f.label.as_ref() == Some(name)
            }) {
                let span = label_token.map_or_else(|| self.span(node), |t| self.token_span(t));
                self.error(
                    codes::T071,
                    span,
                    format!("duplicate loop label `'{name}` in the same scope"),
                );
            }
        }
        self.cx.mutation.loops.push(LoopFrame {
            label,
            apart_depth: self.cx.mutation.apart.len(),
            settled_depth: self.cx.mutation.settled.len(),
            result,
            accepts_value: value,
        });
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
        self.enter_loop_node(node, result.clone(), node.kind == K::LoopExpr);
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
        let label_token = node.child_tokens().find(|t| t.kind == K::Label);
        let label = label_token.map(|t| {
            t.text(self.text)
                .strip_prefix('\'')
                .unwrap_or_else(|| t.text(self.text))
        });
        let target = if let Some(target_label) = label {
            let found = self
                .cx
                .mutation
                .loops
                .iter()
                .rposition(|f| f.label.as_deref() == Some(target_label));
            if let Some(idx) = found {
                let frame = &self.cx.mutation.loops[idx];
                if frame.apart_depth != self.cx.mutation.apart.len()
                    || frame.settled_depth != self.cx.mutation.settled.len()
                {
                    self.error(
                        codes::T071,
                        self.span(node),
                        "`break` and `continue` cannot cross function, task, or comprehension boundaries",
                    );
                    None
                } else {
                    Some(frame.clone())
                }
            } else {
                self.error(
                    codes::T071,
                    self.span(node),
                    format!("no enclosing loop labeled `'{target_label}`"),
                );
                None
            }
        } else {
            let target = self.cx.mutation.loops.last().cloned().filter(|frame| {
                frame.apart_depth == self.cx.mutation.apart.len()
                    && frame.settled_depth == self.cx.mutation.settled.len()
            });
            if target.is_none() {
                self.error(
                    codes::T071,
                    self.span(node),
                    "`break` and `continue` need an enclosing loop in the same function",
                );
            }
            target
        };

        if let Some(frame) = target
            && node.kind == K::BreakExpr
        {
            if let Some(e) = node.nodes().next() {
                if !frame.accepts_value {
                    self.error(
                        codes::T071,
                        self.span(node),
                        "only `loop` accepts `break value`",
                    );
                }
                self.check(e, &frame.result);
            } else {
                self.expect(&Type::lang(Lang::Unit), &frame.result, self.span(node));
            }
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
    /// (`write`: assigned) from code that runs apart from its function.
    pub fn mutable_use(&mut self, index: usize, span: Span, write: bool) -> bool {
        let binding = self.cx.locals[index].binding;
        if !matches!(binding, Binding::Mut | Binding::Lazy | Binding::LazyMut) {
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
        let what = if binding == Binding::Mut {
            "a `let mut` variable"
        } else {
            "a `let lazy` binding"
        };
        let diagnostic = Diagnostic::error(
            codes::G005,
            format!(
                "{inner} cannot use `{name}`, {what} of the function around it: it may run later, more than once or at the same time"
            ),
            span,
        )
        .with_fix(|| {
            let help = if binding != Binding::Mut {
                format!("to share a computation run once with {before}, build it with `lazy {{ … }}` and pass that")
            } else if write {
                // a copy cannot take an assignment: shared state lives in a cell
                format!(
                    "to change `{name}` from {before}, keep it in a `Ref`: `let {name} = Ref.make …`, then `Ref.set {name} …` and `Ref.get {name}`"
                )
            } else {
                format!(
                    "bind the value with `let` before {before} and use that name; to see later changes, keep `{name}` in a `Ref`"
                )
            };
            (help, Vec::new())
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
        let assignable = |b: Binding| matches!(b, Binding::Mut | Binding::LazyMut);
        let Some(index) = found.filter(|&i| assignable(self.cx.locals[i].binding)) else {
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
        if self.mutable_use(index, span, true) {
            self.settled_assignment(index, &name, span);
        }
        let ty =
            (self.lazy_assigned(index, node)).unwrap_or_else(|| self.cx.locals[index].ty.clone());
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

    pub(super) fn restricted_let(
        &mut self,
        node: &Node,
        written: Option<&Node>,
        rest: &[&Node],
        context: &'static str,
    ) {
        let mark = self.cx.locals.len();
        self.local_let(node, written, rest);
        for local in &mut self.cx.locals[mark..] {
            if local.binding == Binding::Let {
                local.binding = Binding::Restricted(context);
            }
        }
    }

    /// `FLICK-T069` for `name = …` where `name` is not a `let mut` variable
    /// of this function; the local `found` is the name if it is one.
    fn not_assignable(&mut self, name: &str, span: Span, found: Option<usize>) {
        let can_mutate = found.is_some_and(|index| {
            self.cx
                .mutation
                .apart
                .iter()
                .all(|(below, _)| index >= *below)
                && self.cx.mutation.settled.iter().all(|below| index >= *below)
        });
        let diagnostic = match found.map(|i| self.cx.locals[i].clone()) {
            Some(Local {
                binding: Binding::Let,
                at: Some(at),
                generated: false,
                ..
            }) => {
                let end = at + u32::try_from(name.len()).expect("source fits in u32");
                let diagnostic = Diagnostic::error(
                    codes::T069,
                    format!("`{name}` is not `let mut`, so it cannot be assigned"),
                    span,
                )
                .with_related(Span::new(self.file, at, end), "bound here");
                if can_mutate {
                    diagnostic.with_safe_fix(|| {
                        (
                            format!("declare it with `let mut {name}`"),
                            vec![Edit {
                                span: Span::new(self.file, at, at),
                                replacement: "mut ".to_owned(),
                            }],
                        )
                    })
                } else {
                    diagnostic
                }
            }
            Some(Local {
                binding: Binding::Restricted(context),
                ..
            }) => Diagnostic::error(
                codes::T069,
                format!(
                    "`{name}` is declared in {context}, where `let mut` is not allowed; bind a variable in an ordinary block"
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
