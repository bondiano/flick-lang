//! Explicit bridged foreign-call metadata for the counted memory IR.
//!
//! Only callers that explicitly opt into foreign bridging emit
//! [`Operation::ForeignCall`](super::Operation::ForeignCall). Closed-world
//! lowering (`perceus::lower` and `perceus::lower_with_declarations`) never
//! constructs this node and continues to reject arbitrary foreign bindings.

use serde::{Deserialize, Serialize};

use crate::{Adapter, ForeignMode};

/// Structural wire-marshalling hint derived from checked compiler types so the
/// counted host bridge distinguishes `Int`, `Float`, and opaque `JsValue`/nominal
/// host handles without heuristic guessing or probing JS getters/proxies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireHint {
    #[default]
    Any,
    Unit,
    Opaque,
    Int,
    Float,
    List(Box<WireHint>),
    Option(Box<WireHint>),
    Result(Box<WireHint>, Box<WireHint>),
    Tuple(Vec<WireHint>),
    Record(Vec<(String, WireHint)>),
    Fun {
        args: Vec<WireHint>,
        result: Box<WireHint>,
    },
    Eff(Box<WireHint>),
}

/// Self-contained specification for one explicitly bridged `Operation::ForeignCall`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignCallSpec {
    pub module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub name: String,
    pub mode: ForeignMode,
    pub dicts: usize,
    pub arity: usize,
    pub taken: usize,
    pub runtime: bool,
    pub has_catch: bool,
    pub adapter: Adapter,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arg_hints: Vec<WireHint>,
    #[serde(default)]
    pub result_hint: WireHint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaining_stages: Vec<bool>,
}

impl ForeignCallSpec {
    /// Number of consumed operand locals carried by `Operation::ForeignCall`.
    pub fn expected_args(&self) -> usize {
        self.dicts + self.arity + usize::from(self.has_catch)
    }
}
