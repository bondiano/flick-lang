//! The values facts are about and the atoms of formulas, read from typed
//! syntax. A value is told apart by where it is bound, not by its name, so
//! a shadowing binder is another value; only a binding that never changes
//! is one. Anything the logic does not read is opaque.

use std::rc::Rc;

use super::logic::Formula;
use crate::consteval::{self, Const};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::children::arguments;
use crate::types::mutation::Binding;
use crate::types::sugar_lints::unparen;
use crate::types::typing::{Range, Ref};

/// A value facts are about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// An immutable local, by where its binder is written.
    Local(u32),
    /// `T.value v`: the invariant at this index of the checker's, and `v`.
    Projection(usize, Rc<Value>),
    /// The one evaluation of the expression at this range: nothing else
    /// names it, so only its type tells of it.
    Once(Range),
    /// The parameter of a check, before it is given an argument.
    Param,
}

impl Value {
    /// The value with the parameter of a check given `argument`.
    fn given(&self, argument: &Value) -> Value {
        match self {
            Value::Param => argument.clone(),
            Value::Projection(at, inner) => Value::Projection(*at, Rc::new(inner.given(argument))),
            other => other.clone(),
        }
    }
}

/// What a formula reads of values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Atom {
    /// `f v` of a top-level function of a module, which takes no
    /// dictionary.
    Call {
        module: String,
        name: String,
        argument: Value,
    },
    /// A `Bool` value itself.
    Truth(Value),
}

impl Atom {
    /// The atom with the parameter of a check given `argument`.
    pub fn given(&self, argument: &Value) -> Atom {
        match self {
            Atom::Call {
                module,
                name,
                argument: of,
            } => Atom::Call {
                module: module.clone(),
                name: name.clone(),
                argument: of.given(argument),
            },
            Atom::Truth(v) => Atom::Truth(v.given(argument)),
        }
    }
}

/// An atom as the walk read it, and where its name is written: whether it
/// takes a dictionary is known once the module is solved.
#[derive(Debug, Clone, PartialEq)]
pub struct Raw {
    pub atom: Atom,
    pub at: Range,
}

/// A formula the path to an expression ran, and the result it gave.
pub type Fact = (Rc<Formula<Raw>>, bool);

/// The parts of a formula read at most: a larger one is opaque.
const SIZE: u32 = 64;

impl Checker<'_> {
    /// The formula of `condition`, already checked, when a proof may read
    /// it.
    pub(in crate::types) fn proof_condition(&self, condition: &Node) -> Option<Rc<Formula<Raw>>> {
        if !self.proofs.enabled() {
            return None;
        }
        let mut size = SIZE;
        let formula = self.proof_formula(condition, &mut size, &|node| self.proof_value(node));
        Some(Rc::new(formula))
    }

    /// The formula of a Boolean expression of the program, its values read
    /// by `value`.
    pub(super) fn proof_formula(
        &self,
        node: &Node,
        size: &mut u32,
        value: &dyn Fn(&Node) -> Option<Value>,
    ) -> Formula<Raw> {
        if *size == 0 {
            return Formula::Opaque;
        }
        *size -= 1;
        let node = unparen(node);
        let children: Vec<&Node> = node.nodes().collect();
        match (node.kind, children.as_slice()) {
            (K::Block, [only]) => self.proof_formula(only, size, value),
            (K::Literal, _) => match consteval::eval(node, self.text) {
                Some(Const::Bool(b)) => Formula::Const(b),
                _ => Formula::Opaque,
            },
            (K::PrefixExpr, [operand]) if node.has_token(K::NotKw) => {
                Formula::Not(Box::new(self.proof_formula(operand, size, value)))
            }
            (K::BinExpr, [left, right]) if node.has_token(K::AndAnd) || node.has_token(K::OrOr) => {
                let left = Box::new(self.proof_formula(left, size, value));
                let right = Box::new(self.proof_formula(right, size, value));
                if node.has_token(K::AndAnd) {
                    Formula::And(left, right)
                } else {
                    Formula::Or(left, right)
                }
            }
            (K::AppExpr, [head, rest @ ..]) => match arguments(rest)[..] {
                [argument] => self.proof_call(head, argument, value),
                _ => Formula::Opaque,
            },
            (K::NameExpr, _) => match value(node) {
                Some(v) => Formula::Atom(Raw {
                    atom: Atom::Truth(v),
                    at: node.range(),
                }),
                None => Formula::Opaque,
            },
            _ => Formula::Opaque,
        }
    }

    /// `f argument` of a top-level function `f`.
    fn proof_call(
        &self,
        head: &Node,
        argument: &Node,
        value: &dyn Fn(&Node) -> Option<Value>,
    ) -> Formula<Raw> {
        let at = head.range();
        let Some(Ref::Value { module, name }) = self.typing.names.get(&at) else {
            return Formula::Opaque;
        };
        match value(argument) {
            Some(argument) => Formula::Atom(Raw {
                atom: Atom::Call {
                    module: module.clone(),
                    name: name.clone(),
                    argument,
                },
                at,
            }),
            None => Formula::Opaque,
        }
    }

    /// The value `node` evaluates to where the walk is now, when facts may
    /// be about it: an immutable local, or `T.value` of one.
    pub(in crate::types) fn proof_value(&self, node: &Node) -> Option<Value> {
        let node = unparen(node);
        match node.kind {
            K::NameExpr => {
                if self.typing.names.get(&node.range()) != Some(&Ref::Local) {
                    return None;
                }
                let name = word(node, self.text)?;
                let generated = node.range().0 >= self.generated;
                let local = (self.cx.locals.iter())
                    .rev()
                    .find(|l| l.name == name && l.generated == generated)?;
                let at = local.at.filter(|_| local.binding != Binding::Mut)?;
                Some(self.proofs.aliased(at))
            }
            K::AppExpr => self.proof_projection(node, &|n| self.proof_value(n)),
            _ => None,
        }
    }

    /// `T.value e` of a type with an invariant: `e`'s value as `value`
    /// reads it, or this one evaluation of it.
    pub(super) fn proof_projection(
        &self,
        node: &Node,
        value: &dyn Fn(&Node) -> Option<Value>,
    ) -> Option<Value> {
        let children: Vec<&Node> = node.nodes().collect();
        let (head, rest) = children.split_first()?;
        let [argument] = arguments(rest)[..] else {
            return None;
        };
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()) else {
            return None;
        };
        let index = self.proofs.projected_by(module, name)?;
        let inner = value(argument).unwrap_or_else(|| Value::Once(unparen(argument).range()));
        Some(Value::Projection(index, Rc::new(inner)))
    }
}

/// The one name a name expression or a variable pattern writes.
pub(super) fn word<'t>(node: &Node, text: &'t str) -> Option<&'t str> {
    let mut tokens = node
        .child_tokens()
        .filter(|t| matches!(t.kind, K::Ident | K::Con));
    match (tokens.next(), tokens.next()) {
        (Some(only), None) => Some(only.text(text)),
        _ => None,
    }
}
