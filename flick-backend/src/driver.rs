use std::{error, fmt};

use crate::request::validate_size;
use crate::validate::diagnostics;
use crate::{
    Backend, BuildRequest, BuildResult, DeclaredInputs, Diagnostic, Policy, Severity, cache_key,
    handshake, validate_request, validate_result,
};

#[derive(Debug)]
pub enum Failure {
    Protocol(String),
    Rejected(Vec<Diagnostic>),
    Unstable,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(why) => f.write_str(why),
            Self::Rejected(diagnostics) => write!(
                f,
                "backend rejected the request ({} diagnostics)",
                diagnostics.len()
            ),
            Self::Unstable => f.write_str(
                "backend generated different results for the same request; caching is disabled",
            ),
        }
    }
}

impl error::Error for Failure {}

/// Only this driver can construct a validated generation. It performs no
/// filesystem writes and starts no tools; publication consumes this result.
pub struct Generated {
    result: BuildResult,
    order: Vec<usize>,
    cache_key: Option<String>,
}

impl Generated {
    pub fn result(&self) -> &BuildResult {
        &self.result
    }
    pub fn execution_order(&self) -> &[usize] {
        &self.order
    }
    pub fn cache_key(&self) -> Option<&str> {
        self.cache_key.as_deref()
    }
}

/// Run static handshake, target preflight and generation in that order.
/// Cache eligibility requires a second independent, identical generation.
pub fn generate(
    backend: &dyn Backend,
    request: &BuildRequest,
    policy: &Policy,
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
    verify_determinism: bool,
) -> Result<Generated, Failure> {
    let protocol = Failure::Protocol;
    if let Some(pin) = backend.loaded_pin()
        && pin != &request.backend
    {
        return Err(Failure::Rejected(vec![Diagnostic {
            code: "P013".into(),
            severity: Severity::Error,
            message: format!(
                "backend {}@{}#{} preflight: request backend pin differs from the loaded component",
                pin.identity.id, pin.identity.version, pin.sha256
            ),
            span: None,
            notes: vec![],
        }]));
    }
    backend.check_cancelled().map_err(Failure::Rejected)?;
    let sized = validate_size(request, policy.limits);
    backend.check_cancelled().map_err(Failure::Rejected)?;
    sized.map_err(protocol)?;
    let descriptor = backend.describe();
    handshake(&request.backend, &descriptor, &request.target).map_err(protocol)?;
    // Bound declarations before reading content supplied by the host.
    let size = request
        .inputs
        .iter()
        .try_fold(0_u64, |n, i| n.checked_add(i.size));
    if size.is_none_or(|n| n > u64::try_from(policy.limits.request_bytes).unwrap_or(u64::MAX)) {
        return Err(protocol(
            "backend side inputs exceed their total input limit".into(),
        ));
    }
    let mut inputs = snapshot(backend, request, &mut read)?;
    let validated = validate_request(request, &mut inputs, policy.limits);
    backend.check_cancelled().map_err(Failure::Rejected)?;
    validated.map_err(protocol)?;
    let refused = |found: Vec<Diagnostic>, inputs: &mut DeclaredInputs| match diagnostics(
        request,
        &found,
        inputs,
        policy.limits,
    ) {
        Ok(()) if !found.is_empty() => Failure::Rejected(found),
        Ok(()) => protocol("backend refused a request without a diagnostic".into()),
        Err(why) => protocol(why),
    };
    let support = backend
        .check_target(request, &mut inputs)
        .map_err(|d| refused(d, &mut inputs))?;
    if request
        .target
        .required_capabilities
        .iter()
        .any(|c| !support.capabilities.contains(c))
        || support
            .capabilities
            .iter()
            .any(|c| !descriptor.capabilities.contains(c))
    {
        return Err(protocol(
            "target preflight returned inconsistent capabilities".into(),
        ));
    }
    let result = backend
        .generate(request, &mut inputs)
        .map_err(|d| refused(d, &mut inputs))?;
    if result
        .diagnostics
        .iter()
        .any(|d| d.severity == crate::Severity::Error)
    {
        diagnostics(request, &result.diagnostics, &mut inputs, policy.limits).map_err(protocol)?;
        return Err(Failure::Rejected(result.diagnostics));
    }
    let order = validate_result(request, &result, policy, &mut inputs).map_err(protocol)?;
    if verify_determinism {
        let repeated = backend
            .generate(request, &mut inputs)
            .map_err(|d| refused(d, &mut inputs))?;
        validate_result(request, &repeated, policy, &mut inputs).map_err(protocol)?;
        if result != repeated {
            return Err(Failure::Unstable);
        }
    }
    Ok(Generated {
        result,
        order,
        cache_key: verify_determinism.then(|| cache_key(request)),
    })
}

/// Cancellation takes priority over an unavailable input or a digest error.
fn snapshot(
    backend: &dyn Backend,
    request: &BuildRequest,
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<DeclaredInputs, Failure> {
    backend.check_cancelled().map_err(Failure::Rejected)?;
    let inputs = DeclaredInputs::new(&request.inputs, |id| {
        backend.check_cancelled().ok()?;
        let bytes = read(id);
        backend.check_cancelled().ok()?;
        bytes
    });
    backend.check_cancelled().map_err(Failure::Rejected)?;
    inputs.map_err(Failure::Protocol)
}
