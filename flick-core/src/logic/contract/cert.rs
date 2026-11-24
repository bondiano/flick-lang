//! Certificates produced by `PureTotalV2` contract symbolic execution and
//! independently verified by [`super::verify::check_contract_cert`].

use crate::logic::omega;

/// A complete certificate attesting that a function's body establishes its
/// contract (postconditions, callee preconditions, and 32-bit `Int` range
/// obligations) on every reachable execution path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCert {
    /// Must equal [`crate::logic::DOMAIN_V2`] (`"PureTotalV2"`).
    pub domain: &'static str,
    pub root: PathCert,
}

/// A leaf discharge for a single linear/boolean obligation or dead path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeafCert {
    /// Discharged by direct tautology or syntactic contradiction in the path
    /// state (re-checked independently by the kernel).
    Trivial,
    /// Discharged by an independently verifiable Omega refutation certificate.
    Omega(omega::Proof),
}

/// Both bounds of a 32-bit signed `Int` range obligation
/// (`-2147483648 <= res <= 2147483647`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeCert {
    pub lower: LeafCert,
    pub upper: LeafCert,
}

/// Tree of verified obligations mirroring the symbolic execution paths of a
/// contracted function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathCert {
    /// Current path assumptions are unsatisfiable (dead branch).
    Dead(LeafCert),
    /// Range obligation on an `Int` `+`, `-`, or unary `-` evaluated in body
    /// mode, followed by the certificate for the continuation.
    Range {
        cert: RangeCert,
        next: Box<PathCert>,
    },
    /// Saturated call to a verified contracted function: `pre` certifies its
    /// `require` clauses, and `post` certifies the continuation after assuming
    /// its `ensure` clauses on a fresh bounded result symbol.
    Call {
        callee: String,
        pre: Box<PathCert>,
        post: Box<PathCert>,
    },
    /// Case split on a boolean condition (`if`, short-circuit `&&`/`||`,
    /// `!=` inequality split, or pattern condition).
    Branch {
        then_branch: Box<PathCert>,
        else_branch: Box<PathCert>,
    },
    /// Exhaustive constructor split on an algebraic data type in declaration
    /// order.
    MatchData { branches: Vec<(String, PathCert)> },
    /// Conjunction of sub-obligations (e.g. equality `a == b` as `a >= b` and
    /// `b >= a`, or multiple `require`/`ensure` clauses).
    And(Vec<PathCert>),
    /// Terminal obligation discharge.
    Leaf(LeafCert),
}
