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
    (operands.len() == operators.len() + 1).then_some((operands, operators))
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

/// One call a composition makes: of the local its operand is bound to.
struct Call {
    local: usize,
    key: Range,
    operand: Span,
}

impl Checker<'_> {
    /// `x |> f` applies `f` to `x`, evaluating `x` first.
    pub(super) fn pipe(&mut self, lhs: &Node, rhs: &Node) -> Type {
        let arg = self.infer(lhs);
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

    /// `f a <| x` is the application `f a x`.
    pub(super) fn right_apply(&mut self, lhs: &Node, rhs: &Node) -> Type {
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
        let calls: Vec<Call> = applied_order(&operators)
            .into_iter()
            .map(|i| Call {
                local: mark + i,
                key: composed_key(&operands, &operators, i),
                operand: self.span(operands[i]),
            })
            .collect();
        let op = if operators[0].kind == K::GtGt {
            ">>"
        } else {
            "<<"
        };
        let (start, end) = result_key(&operators);
        let at = Span::new(self.file, start, end);
        let param = param_key(&operators);
        let body = |c: &mut Self, stages: &[(Type, Option<Box<Effects>>)], rest: Option<Type>| {
            c.note_value(param, &stages[0].0);
            let value = c.composed(&calls, stages[0].0.clone(), op);
            match rest {
                Some(rest) => {
                    c.expect(&value, &rest, at);
                    rest
                }
                None => value,
            }
        };
        let ty = self.function_typed(&[None], body, expected, node);
        self.cx.locals.truncate(mark);
        ty
    }

    /// The value of the calls `calls`, the first applied first, given `x`:
    /// each one typed as the application of its local to the value of the
    /// calls before it, as `apply` types `g' (f' x)`.
    fn composed(&mut self, calls: &[Call], x: Type, op: &str) -> Type {
        let Some((call, inner)) = calls.split_last() else {
            return x;
        };
        let mut defaults = Vec::new();
        let provided = self.cx.regions.provided.len();
        let span = Span::new(self.file, call.key.0, call.key.1);
        let function = self.use_local(call.local, span);
        let function = self.value_bound(function);
        let ty = match self.ui_callable(&function, call.key, call.operand) {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let since = self.diagnostics.len();
                let eff = self.eff_parts(&param).is_some();
                let outer = eff.then(|| self.cx.regions.defaults.replace(Vec::new()));
                let argument = self.composed(inner, x, op);
                self.expect(&argument, &param, span);
                if let Some(outer) = outer {
                    let mine = mem::replace(&mut self.cx.regions.defaults, outer);
                    defaults.extend(mine.unwrap_or_default());
                }
                self.explain_since(since, &param, call.operand, &|t| {
                    format!("this function takes `{t}`")
                });
                if eff {
                    self.grow();
                }
                if let Some(e) = effects {
                    self.call_effects(&self.live_call(&function, &e), span);
                }
                let result = self.instantiate(&result, span);
                self.application_result(result, &function, &argument, false)
            }
            Type::Error => {
                self.composed(inner, x, op);
                Type::Error
            }
            meta @ Type::Meta(_) => {
                let (param, result) = (self.fresh(), self.fresh());
                let function = Type::pure_fun(param.clone(), result.clone());
                if self.unify(&meta, &function).is_ok() {
                    let argument = self.composed(inner, x, op);
                    self.expect(&argument, &param, span);
                    result
                } else {
                    self.composed(inner, x, op);
                    self.not_composable(&meta, call.operand, op);
                    Type::Error
                }
            }
            other => {
                self.composed(inner, x, op);
                self.not_composable(&other, call.operand, op);
                Type::Error
            }
        };
        self.settle_defaults(defaults);
        self.settle_provided(provided);
        self.note_value(call.key, &ty);
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
