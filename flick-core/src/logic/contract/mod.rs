//! Contract verification in the `PureTotalV2` domain: spanned terms with
//! linear `Int` operations, strict IR extraction, definition admission,
//! dependency ordering, symbolic execution, certificates, independent kernel
//! verification, and contract diagnostics (`T087`–`T089`, `T082`–`T084`).

pub mod cert;
pub mod defs;
pub mod eval;
mod state;
pub mod term;
pub mod verify;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;

use super::{DOMAIN_V2, Data, Program, Refused, Ty};

pub use cert::{ContractCert, LeafCert, PathCert, RangeCert};
pub use defs::{
    AdmittedV2, ContractDepNode, ContractOrder, admit_v2, collect_ir_contract_deps, order_contracts,
};
pub use state::{Budgets, INT_MAX, INT_MIN, MAX_DEPTH, MAX_OMEGA_BUDGET, MAX_PATHS, MAX_STEPS};
pub use term::{
    Arm, IntBinOp, IntCmpOp, LangTypes, Pat, PrimHint, Term, TermKind, extract, prim_hint_of_ty,
};
pub use verify::{check_contract_cert, check_contract_cert_with_budgets};

/// The extracted specification of one contracted function in `PureTotalV2`.
///
/// `body` (passed alongside this spec to [`prove_contract`]), `requires`, and
/// `ensures` are evaluated in an environment binding `params` in declaration
/// order.
#[derive(Debug, Clone)]
pub struct ContractSpec {
    /// Defining module name (e.g. `"App.Inventory"`).
    pub module: String,
    /// Function name (e.g. `"consume"`).
    pub name: String,
    /// Span of the function declaration (`let name ... = ...`).
    pub decl_span: Span,
    /// Span of the `where` contract block.
    pub contract_span: Span,
    /// Parameter names (matching lowered local names) and their `logic::Ty`,
    /// in declaration order.
    pub params: Vec<(Rc<str>, Ty)>,
    /// Return type of the function after applying `params`.
    pub result_ty: Ty,
    /// `require` boolean expressions in declaration order, with source spans.
    pub requires: Vec<(Term, Span)>,
    /// `ensure` predicate expressions (`Result -> Bool`) in declaration order,
    /// with source spans.
    pub ensures: Vec<(Term, Span)>,
}

/// Summary of a contract whose [`ContractCert`] passed
/// [`verify::check_contract_cert`]. Callers use this summary at call sites
/// instead of unfolding the callee's body.
#[derive(Debug, Clone)]
pub struct ContractSummary {
    pub module: String,
    pub name: String,
    pub params: Vec<(Rc<str>, Ty)>,
    pub result_ty: Ty,
    pub requires: Vec<(Term, Span)>,
    pub ensures: Vec<(Term, Span)>,
    /// Data types referenced by this callee's contract.
    pub data: Data,
}

impl ContractSummary {
    pub fn from_spec(spec: &ContractSpec, data: &Data) -> Self {
        Self {
            module: spec.module.clone(),
            name: spec.name.clone(),
            params: spec.params.clone(),
            result_ty: spec.result_ty.clone(),
            requires: spec.requires.clone(),
            ensures: spec.ensures.clone(),
            data: data.clone(),
        }
    }
}

/// Set of contracts in the current verification session.
///
/// Only contracts whose certificate passed [`verify::check_contract_cert`] can
/// enter `summaries` (via [`Self::prove_and_record`]). Failed, cyclic, refused,
/// or pending contracts live in `unverified` so they are never unfolded as
/// helpers and never assumed at call sites.
#[derive(Debug, Default, Clone)]
pub struct VerifiedContracts {
    summaries: HashMap<(String, String), ContractSummary>,
    unverified: HashSet<(String, String)>,
}

impl VerifiedContracts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `(module, name)` as a contracted function whose contract is not
    /// (yet) verified (pending, failed, refused, or cyclic).
    pub fn mark_unverified(&mut self, module: String, name: String) {
        let key = (module, name);
        self.summaries.remove(&key);
        self.unverified.insert(key);
    }

    /// Records a [`ContractSummary`] that was already certified by the kernel
    /// earlier in the same `flick prove` run (e.g. from an imported module).
    pub fn insert_verified(&mut self, summary: ContractSummary) {
        let key = (summary.module.clone(), summary.name.clone());
        self.unverified.remove(&key);
        self.summaries.insert(key, summary);
    }

    pub fn get(&self, module: &str, name: &str) -> Option<&ContractSummary> {
        self.summaries.get(&(module.to_owned(), name.to_owned()))
    }

    pub fn is_verified(&self, module: &str, name: &str) -> bool {
        self.summaries
            .contains_key(&(module.to_owned(), name.to_owned()))
    }

    pub fn is_unverified(&self, module: &str, name: &str) -> bool {
        self.unverified
            .contains(&(module.to_owned(), name.to_owned()))
    }

    /// Every `(module, name)` known to carry a contract (verified or not),
    /// used so [`admit_v2`] never unfolds a contracted callee's body.
    pub fn all_contracted_keys(&self) -> HashSet<(String, String)> {
        let mut keys: HashSet<(String, String)> = self.summaries.keys().cloned().collect();
        keys.extend(self.unverified.iter().cloned());
        keys
    }

    /// Proves `(spec, body)`, verifies the resulting certificate with the
    /// independent kernel, and records `spec`'s [`ContractSummary`] ONLY on
    /// kernel acceptance. On any error, marks `(spec.module, spec.name)` as
    /// unverified so later callers cannot assume or unfold it.
    pub fn prove_and_record(
        &mut self,
        spec: &ContractSpec,
        body: &Term,
        data: &Data,
        program: &dyn Program,
        lang: &LangTypes,
    ) -> Result<ContractCert, ContractFailure> {
        self.prove_and_record_with_budgets(spec, body, data, program, lang, Budgets::default())
    }

    /// Same as [`Self::prove_and_record`] under explicit [`Budgets`].
    pub fn prove_and_record_with_budgets(
        &mut self,
        spec: &ContractSpec,
        body: &Term,
        data: &Data,
        program: &dyn Program,
        lang: &LangTypes,
        budgets: Budgets,
    ) -> Result<ContractCert, ContractFailure> {
        let key = (spec.module.clone(), spec.name.clone());
        match prove_contract_with_budgets(spec, body, data, program, self, lang, budgets) {
            Ok(cert) => {
                self.unverified.remove(&key);
                self.summaries
                    .insert(key, ContractSummary::from_spec(spec, data));
                Ok(cert)
            }
            Err(err) => {
                self.summaries.remove(&key);
                self.unverified.insert(key);
                Err(err)
            }
        }
    }
}

/// Proves `(spec, body)` in `PureTotalV2` and independently verifies the
/// resulting certificate with [`verify::check_contract_cert`].
///
/// Never returns `Ok` unless the independent kernel accepts the certificate.
pub fn prove_contract(
    spec: &ContractSpec,
    body: &Term,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
) -> Result<ContractCert, ContractFailure> {
    prove_contract_with_budgets(
        spec,
        body,
        data,
        program,
        verified,
        lang,
        Budgets::default(),
    )
}

/// Same as [`prove_contract`] under explicit deterministic [`Budgets`].
pub fn prove_contract_with_budgets(
    spec: &ContractSpec,
    body: &Term,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
    budgets: Budgets,
) -> Result<ContractCert, ContractFailure> {
    let cert = eval::prove_with_budgets(spec, body, data, program, verified, lang, budgets)?;
    verify::check_contract_cert_with_budgets(
        spec, body, &cert, data, program, verified, lang, budgets,
    )?;
    Ok(cert)
}

/// An `Int` arithmetic operation that carries a range obligation in `PureTotalV2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Neg,
}

impl ArithOp {
    pub fn label(self) -> &'static str {
        match self {
            ArithOp::Add => "`+`",
            ArithOp::Sub => "`-`",
            ArithOp::Neg => "unary `-`",
        }
    }
}

/// A concrete assignment verified against the path's linear constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterexample {
    /// `(display_name, rendered_value)` in parameter declaration order,
    /// followed by any constrained call-result witness if relevant.
    pub bindings: Vec<(String, String)>,
}

impl Counterexample {
    pub fn suffix(&self) -> String {
        if self.bindings.is_empty() {
            return String::new();
        }
        let items: Vec<String> = self
            .bindings
            .iter()
            .map(|(name, value)| format!("{name} = {value}"))
            .collect();
        format!(", e.g. when `{}`", items.join(", "))
    }
}

/// Why a function's contract failed to verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractFailure {
    /// `FLICK-T087`: a postcondition (`ensure`) is not established.
    Postcondition {
        span: Span,
        decl_span: Span,
        counterexample: Option<Counterexample>,
    },
    /// `FLICK-T088`: a callee's precondition (`require`) is not established at a call.
    Precondition {
        span: Span,
        callee: String,
        callee_require: Option<Span>,
        counterexample: Option<Counterexample>,
    },
    /// `FLICK-T089`: an `Int` arithmetic operation may leave `[-2147483648, 2147483647]`.
    Overflow {
        span: Span,
        op: ArithOp,
        counterexample: Option<Counterexample>,
    },
    /// `FLICK-T082`: outside domain `PureTotalV2`.
    Refused(Refused),
    /// `FLICK-T083`: invalid certificate (independent kernel rejected it).
    Invalid { span: Span, why: String },
    /// `FLICK-T084`: budget exhausted (nothing proved).
    Exhausted { span: Span },
}

impl ContractFailure {
    pub fn diagnostic(&self, function: &str) -> Diagnostic {
        let ce = |counterexample: &Option<Counterexample>| {
            counterexample
                .as_ref()
                .map_or_else(String::new, Counterexample::suffix)
        };
        match self {
            ContractFailure::Postcondition {
                span,
                decl_span,
                counterexample,
            } => {
                let diag = Diagnostic::error(
                    codes::T087,
                    format!(
                        "the postcondition of `{function}` is not established{}",
                        ce(counterexample)
                    ),
                    *span,
                );
                if *decl_span == *span {
                    diag
                } else {
                    diag.with_related(*decl_span, format!("for `{function}`"))
                }
            }
            ContractFailure::Precondition {
                span,
                callee,
                callee_require,
                counterexample,
            } => {
                let diag = Diagnostic::error(
                    codes::T088,
                    format!(
                        "the precondition of `{callee}` is not established at this call{}",
                        ce(counterexample)
                    ),
                    *span,
                );
                if let Some(req_span) = *callee_require {
                    diag.with_related(req_span, format!("required by `{callee}` here"))
                } else {
                    diag
                }
            }
            ContractFailure::Overflow {
                span,
                op,
                counterexample,
            } => Diagnostic::error(
                codes::T089,
                format!(
                    "{} may leave the range of `Int` (`-2147483648..=2147483647`){}",
                    op.label(),
                    ce(counterexample)
                ),
                *span,
            ),
            ContractFailure::Refused(r) => Diagnostic::error(
                codes::T082,
                format!(
                    "the contract of `{function}` cannot be proved in the domain `{DOMAIN_V2}`: {}",
                    r.why
                ),
                r.span,
            ),
            ContractFailure::Invalid { span, why } => Diagnostic::error(
                codes::T083,
                format!("the contract of `{function}` has no valid certificate: {why}"),
                *span,
            ),
            ContractFailure::Exhausted { span } => Diagnostic::error(
                codes::T084,
                format!(
                    "checking the contract of `{function}` ran out of its budget: nothing is proved"
                ),
                *span,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
