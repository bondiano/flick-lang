//! What a build without the `contracts` capability has of contract
//! verification: nothing. Its only caller is `flick prove`
//! (`crate::project::prove`), which such a build does not have either, so
//! no contract is ever verified here.

use super::proofs::Run;
use crate::diagnostics::Diagnostic;
use crate::resolve;
use crate::types::Typing;

/// A contract a `flick prove` run verified. A build without the capability
/// that proves contracts verifies none, so this type has no values.
#[derive(Debug, Clone)]
pub enum ProvenContract {}

/// Verifies no contract: this build carries no contract verification.
pub fn check_contracts(
    module: &resolve::Module,
    typing: &Typing,
    run: &Run,
) -> (Vec<Diagnostic>, Vec<ProvenContract>) {
    let _ = (module, typing, run);
    (Vec::new(), Vec::new())
}
