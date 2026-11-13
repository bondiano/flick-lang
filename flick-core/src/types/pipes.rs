//! The pipes `x |> f` and `f <| x`, and the compositions `f >> g` and
//! `f << g`. A composition is the closure it stands for:
//! `let f' = f; let g' = g; fun x -> g' (f' x)`. Its operands are evaluated
//! once, left to right, where it is written, and it is typed as that
//! closure: its arrow has the rows of the calls it makes and holds what its
//! operands hold.

use std::mem;

use super::Checker;
use super::sugar_lints::unparen;
use super::ty::{Effects, Type};
use super::typing::Range;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// The callee and the arguments of `f a <| x`, which are those of `f a x`.
pub fn right_application<'n>(lhs: &'n Node, rhs: &'n Node) -> Vec<&'n Node> {
    let call = lhs.kind == K::AppExpr && !lhs.nodes().any(|n| n.kind == K::Children);
    let mut parts: Vec<&Node> = if call {
        lhs.nodes().collect()
    } else {
        vec![lhs]
    };
    parts.push(rhs);
    parts
}

/// The composition operator of `node`, if it is a composition.
fn composer(node: &Node) -> Option<K> {
    if node.kind != K::BinExpr {
        return None;
    }
    let op = node.child_tokens().find(|t| t.kind.is_binary_op())?;
    matches!(op.kind, K::GtGt | K::LtLt).then_some(op.kind)
}

pub fn composes(node: &Node) -> bool {
    composer(node).is_some()
}

/// The operands and operators of a chain of one composition operator, in
/// source order and through parentheses: `(f >> g) >> h` composes `f`, `g`
/// and `h`. Grouping changes neither the order the operands are evaluated
/// in nor the type, so the chain is one closure.
pub fn composition(node: &Node) -> Option<(Vec<&Node>, Vec<Token>)> {
    fn chain<'n>(node: &'n Node, op: K, operands: &mut Vec<&'n Node>, operators: &mut Vec<Token>) {
        for child in &node.children {
            match child {
                Element::Node(c) if composer(unparen(c)) == Some(op) => {
                    chain(unparen(c), op, operands, operators);
                }
                Element::Node(c) => operands.push(c),
                Element::Token(t) if t.kind == op => operators.push(*t),
                Element::Token(_) => {}
            }
        }
    }
    let op = composer(node)?;
    let (mut operands, mut operators) = (Vec::new(), Vec::new());
    chain(node, op, &mut operands, &mut operators);
    // a chain with an operand missing is no composition: the parser puts an
    // empty `Error` node at the file's start in its place
    let alternate = operands.len() == operators.len() + 1
        && (operators.iter().enumerate())
            .all(|(i, o)| operands[i].range().1 <= o.start && o.end <= operands[i + 1].range().0);
    alternate.then_some((operands, operators))
}

/// The key of the call of operand `i`: `f >>` for the first, `>> g` for
/// the others. No expression starts or ends at an operator, so these keys
/// name nothing else.
pub fn composed_key(operands: &[&Node], operators: &[Token], i: usize) -> Range {
    if i == 0 {
        (operands[0].range().0, operators[0].end)
    } else {
        (operators[i - 1].start, operands[i].range().1)
    }
}

/// The key of the value a composition returns: the operator next to the
/// function it applies last.
pub fn result_key(operators: &[Token]) -> Range {
    let op = match operators {
        [first, ..] if first.kind == K::LtLt => *first,
        [.., last] => *last,
        [] => unreachable!("a composition has an operator"),
    };
    (op.start, op.end)
}

/// The key of the type of the parameter of a composition's closure: its
/// first operator.
pub fn param_key(operators: &[Token]) -> Range {
    (operators[0].start, operators[0].end)
}

/// The indices of a composition's operands from the function it applies
/// first: `f >> g` applies `f` first, `f << g` applies `g` first. A chain
/// has one operand more than operators.
pub fn applied_order(operators: &[Token]) -> Vec<usize> {
    let operands = 0..=operators.len();
    if operators.first().is_some_and(|o| o.kind == K::LtLt) {
        operands.rev().collect()
    } else {
        operands.collect()
    }
}

/// The call of one operand of a composition, as far as it is checked
/// before its argument.
struct Call {
    shape: Shape,
    provided: usize,
    operand: Span,
    span: Span,
    function: Type,
}

/// What the function of a call is.
enum Shape {
    Fun {
        param: Type,
        effects: Option<Box<Effects>>,
        result: Type,
        since: usize,
        eff: bool,
        /// The defaults around, set aside while the argument is typed.
        outer: Option<Vec<(Type, usize)>>,
    },
    Error,
    /// A function not known yet, made `param -> result`.
    Unknown {
        param: Type,
        result: Type,
    },
    /// Not a function.
    Not(Type),
}

/// A composition being typed: its operands, bound to the locals from
/// `mark` on, and its operator as written.
struct Chain<'a, 'n> {
    operands: &'a [&'n Node],
    operators: &'a [Token],
    mark: usize,
    op: &'static str,
}

impl Checker<'_> {
    /// `x |> f` applies `f` to `x`, of type `arg`: `x` is evaluated first.
    pub(super) fn pipe_from(&mut self, arg: Type, lhs: &Node, rhs: &Node) -> Type {
        let function = self.infer(rhs);
        let call = (lhs.range().0, rhs.range().1);
        match self.ui_callable(&function, call, self.span(rhs)) {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                self.expect(&arg, &param, self.span(lhs));
                if let Some(e) = effects {
                    let (start, _) = lhs.range();
                    let (_, end) = rhs.range();
                    let e = self.live_call(&function, &e);
                    self.call_effects(&e, Span::new(self.file, start, end));
                }
                let span = Span::new(self.file, lhs.range().0, rhs.range().1);
                let result = self.instantiate(&result, span);
                self.application_result(result, &function, &arg, false)
            }
            Type::Error => Type::Error,
            meta @ Type::Meta(_) => {
                let result = self.fresh();
                let _ = self.unify(&meta, &Type::pure_fun(arg, result.clone()));
                result
            }
            other => {
                let shown = self.show(&other);
                self.error(
                    codes::T003,
                    self.span(rhs),
                    format!("the right side of `|>` must be a function, found `{shown}`"),
                );
                Type::Error
            }
        }
    }

    /// `f a <| x` is the application `f a x`. Without its `x`, which the
    /// parser reported, it is its function alone.
    pub(super) fn right_apply(&mut self, lhs: &Node, rhs: &Node) -> Type {
        if rhs.kind == K::Error {
            self.infer(lhs);
            return Type::Error;
        }
        let parts = right_application(lhs, rhs);
        self.apply(parts[0], &parts[1..])
    }

    /// `f >> g` or `f << g`, checked against `expected` when it is known.
    pub(super) fn compose(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let Some((operands, operators)) = composition(node) else {
            for child in node.nodes() {
                self.infer(child);
            }
            return Type::Error;
        };
        // the operands first, at this stage, as the bindings of the closure
        let values: Vec<Type> = operands.iter().map(|o| self.infer(o)).collect();
        let mark = self.cx.locals.len();
        for (operand, ty) in operands.iter().zip(values) {
            let name = self.text_of(operand);
            self.push_local(&name, operand.range().0, ty);
        }
        let order = applied_order(&operators);
        let op = if operators[0].kind == K::GtGt {
            ">>"
        } else {
            "<<"
        };
        let chain = Chain {
            operands: &operands,
            operators: &operators,
            mark,
            op,
        };
        let (start, end) = result_key(&operators);
        let at = Span::new(self.file, start, end);
        let param = param_key(&operators);
        let mut body = |c: &mut Self, x: Option<&Type>, rest: Option<Type>| {
            let x = x.cloned().unwrap_or(Type::Error);
            c.note_value(param, &x);
            let value = c.composed(&chain, &order, x);
            match rest {
                Some(rest) => {
                    c.expect(&value, &rest, at);
                    rest
                }
                None => value,
            }
        };
        let ty = self.function_typed(&[None], &mut body, expected, node);
        self.cx.locals.truncate(mark);
        // at each operator, the function the chain makes: hover shows it
        for op in &operators {
            self.note_type(self.token_span(*op), &ty);
        }
        ty
    }

    /// The value of the calls of the operands `order` names, the first
    /// applied first, given `x`: each one typed as the application of its
    /// local to the value of the calls before it, as `apply` types `g' (f' x)`.
    /// The calls are taken in a loop, outermost first and then back out, as
    /// the recursion through `g' (f' x)` would: a long chain would take
    /// stack per operand.
    fn composed(&mut self, chain: &Chain, order: &[usize], x: Type) -> Type {
        let mut calls = Vec::with_capacity(order.len());
        for &i in order.iter().rev() {
            calls.push(self.composed_call(chain, i));
        }
        let mut value = x;
        for call in calls.into_iter().rev() {
            value = self.composed_result(chain, call, &value);
        }
        value
    }

    /// What the call of operand `i` checks before its argument.
    fn composed_call(&mut self, chain: &Chain, i: usize) -> Call {
        let provided = self.cx.regions.provided.len();
        let key = composed_key(chain.operands, chain.operators, i);
        let operand = self.span(chain.operands[i]);
        let span = Span::new(self.file, key.0, key.1);
        let function = self.use_local(chain.mark + i, span);
        let function = self.value_bound(function);
        let shape = match self.ui_callable(&function, key, operand) {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let since = self.diagnostics.len();
                let eff = self.eff_parts(&param).is_some();
                let outer = if eff {
                    self.cx.regions.defaults.replace(Vec::new())
                } else {
                    None
                };
                Shape::Fun {
                    param: *param,
                    effects,
                    result: *result,
                    since,
                    eff,
                    outer,
                }
            }
            Type::Error => Shape::Error,
            meta @ Type::Meta(_) => {
                let (param, result) = (self.fresh(), self.fresh());
                let function = Type::pure_fun(param.clone(), result.clone());
                if self.unify(&meta, &function).is_ok() {
                    Shape::Unknown { param, result }
                } else {
                    Shape::Not(meta)
                }
            }
            other => Shape::Not(other),
        };
        Call {
            shape,
            provided,
            operand,
            span,
            function,
        }
    }

    /// The value of `call` given its argument.
    fn composed_result(&mut self, chain: &Chain, call: Call, argument: &Type) -> Type {
        let Call {
            shape,
            provided,
            operand,
            span,
            function,
        } = call;
        let mut defaults = Vec::new();
        let ty = match shape {
            Shape::Fun {
                param,
                effects,
                result,
                since,
                eff,
                outer,
            } => {
                self.expect(argument, &param, span);
                if eff {
                    let mine = mem::replace(&mut self.cx.regions.defaults, outer);
                    defaults.extend(mine.unwrap_or_default());
                }
                self.explain_since(since, &param, operand, &|t| {
                    format!("this function takes `{t}`")
                });
                if eff {
                    self.grow();
                }
                if let Some(e) = effects {
                    self.call_effects(&self.live_call(&function, &e), span);
                }
                let result = self.instantiate(&result, span);
                self.application_result(result, &function, argument, false)
            }
            Shape::Error => Type::Error,
            Shape::Unknown { param, result } => {
                self.expect(argument, &param, span);
                result
            }
            Shape::Not(ty) => {
                self.not_composable(&ty, operand, chain.op);
                Type::Error
            }
        };
        self.settle_defaults(defaults);
        self.settle_provided(provided);
        self.note_value(super::typing::range(span), &ty);
        ty
    }

    fn not_composable(&mut self, ty: &Type, span: Span, op: &str) {
        let shown = self.show(ty);
        self.error(
            codes::T003,
            span,
            format!("`{op}` composes functions, but this has type `{shown}`"),
        );
    }
}
