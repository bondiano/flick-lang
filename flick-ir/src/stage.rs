//! How a stage of a function runs.

use serde::{Deserialize, Serialize};

/// How a stage of a function runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// `->`: at once, with nothing.
    Pure,
    /// At once, reading the effects of the context the runtime has set.
    Sync,
    /// Through the runtime, which may suspend it.
    Runtime,
}

/// How a function becomes the one of another stage its position expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lift {
    /// A function: the stage of this stage and the one expected, and how its
    /// result lifts.
    Stage {
        from: Stage,
        to: Stage,
        result: Option<Box<Lift>>,
    },
    /// The result of an `Eff`.
    Eff(Box<Lift>),
}
