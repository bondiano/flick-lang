//! Explicit owned operation functions, independent of source handler syntax.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Error, Local};
use crate::OpClass;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandlerEffect {
    pub effect: String,
    /// Only mentioned operations are installed: absent operations forward.
    pub operations: Vec<HandlerOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandlerOperation {
    pub name: String,
    pub class: OpClass,
    pub dicts: usize,
    /// Ordinary arguments through the first runtime stage, not all later
    /// arrows of a value returned by this operation.
    pub arity: usize,
    /// Owned unary-curried function: dictionaries, then arguments, then
    /// `resume` for Control. No implicit captures are outside this token.
    pub function: Local,
}

pub(super) fn validate(effects: &[HandlerEffect]) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for effect in effects {
        if !names.insert(&effect.effect) {
            return Err(Error::DuplicateHandledEffect(effect.effect.clone()));
        }
        let mut operations = BTreeSet::new();
        for op in &effect.operations {
            if !operations.insert(&op.name) {
                return Err(Error::DuplicateHandledOperation {
                    effect: effect.effect.clone(),
                    op: op.name.clone(),
                });
            }
            op.dicts
                .checked_add(op.arity)
                .and_then(|arity| arity.checked_add(usize::from(op.class == OpClass::Control)))
                .ok_or_else(|| Error::HandlerArityOverflow {
                    effect: effect.effect.clone(),
                    op: op.name.clone(),
                })?;
        }
    }
    Ok(())
}
