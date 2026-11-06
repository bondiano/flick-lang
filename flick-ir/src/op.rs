//! The classes of operations of effects.

use serde::{Deserialize, Serialize};

/// What a handler's branch for an operation is: `tail` gives the operation
/// its value, `abort` the handler its value without coming back, and
/// `control` the handler its value with the rest of the body as `resume`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpClass {
    #[default]
    Tail,
    Abort,
    Control,
}

impl OpClass {
    pub fn is_tail(&self) -> bool {
        *self == OpClass::Tail
    }
}
