//! A handler of effects: a `try` with arms for operations, or `provide S
//! impl`. Per effect it has the function of each operation; then the body,
//! what becomes of the body's value, and the arms for its errors.

use serde::{Deserialize, Serialize};

use super::{Arm, Expr};
use crate::types::OpClass;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handler {
    pub effects: Vec<Handled>,
    pub body: Expr,
    /// A function of the body's value, the handler's value; without it the
    /// body's value is the handler's.
    pub ret: Option<Expr>,
    /// The arms for the body's errors, as those of a `Try`.
    pub errors: Vec<Arm>,
}

/// An effect a handler handles, with the function of each operation's arms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handled {
    /// `M.E`, the key of the effect.
    pub effect: String,
    pub operations: Vec<OpArm>,
}

/// The arms of an operation as one function. It takes what the operation
/// takes, its dictionaries first, as a field of `S.Impl` does: for `tail`
/// its result is the operation's value, for `abort` the handler's, and for
/// `control` it is pure up to a last runtime parameter, `resume`, and its
/// result the handler's. `provide S impl` handles each operation by the
/// field of `impl`, as a `tail` arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpArm {
    pub name: String,
    pub class: OpClass,
    /// The dictionaries it takes first.
    pub dicts: usize,
    /// The arguments it takes then, up to the operation's first runtime stage.
    pub arity: usize,
    pub function: Expr,
}

impl Handler {
    /// Calls `f` on each subexpression, in evaluation order: the
    /// implementations, the body, then the value and the error arms.
    pub fn each_child<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        for handled in &self.effects {
            handled.operations.iter().for_each(|a| f(&a.function));
        }
        f(&self.body);
        if let Some(ret) = &self.ret {
            f(ret);
        }
        for arm in &self.errors {
            if let Some(guard) = &arm.guard {
                f(guard);
            }
            f(&arm.body);
        }
    }

    pub fn each_child_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        for handled in &mut self.effects {
            handled
                .operations
                .iter_mut()
                .for_each(|a| f(&mut a.function));
        }
        f(&mut self.body);
        if let Some(ret) = &mut self.ret {
            f(ret);
        }
        for arm in &mut self.errors {
            if let Some(guard) = &mut arm.guard {
                f(guard);
            }
            f(&mut arm.body);
        }
    }
}
