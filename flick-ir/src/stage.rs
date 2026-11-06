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
