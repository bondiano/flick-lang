#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
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

/// Per-stage durations collected by [`generate_with_timings`].
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StageTimings {
    /// Pin check, request size bound, `describe()`, and static `handshake()`.
    pub handshake: Duration,
    /// Host side-input snapshot and SHA-256 verification (`DeclaredInputs::new`).
    pub input_snapshot: Duration,
    /// Host request validation (`validate_request`, including host CBOR Program decode #1).
    pub request_validation: Duration,
    /// Target preflight (`check_target`, guest CBOR decode #2).
    pub check_target: Duration,
    /// First `generate` call (guest CBOR decode #3 and emission).
    pub generate_first: Duration,
    /// Host validation of the first `BuildResult` (`validate_result`).
    pub result_validation: Duration,
    /// Determinism replay `generate` call (guest CBOR decode #4 and emission).
    pub generate_replay: Duration,
    /// Host validation of the replay `BuildResult` and equality comparison.
    pub replay_validation: Duration,
    /// Deterministic request cache key computation (`cache_key`).
    pub cache_key: Duration,
}

/// Run static handshake, target preflight and generation in that order.
/// Cache eligibility requires a second independent, identical generation.
pub fn generate(
    backend: &dyn Backend,
    request: &BuildRequest,
    policy: &Policy,
    read: impl FnMut(&str) -> Option<Vec<u8>>,
    verify_determinism: bool,
) -> Result<Generated, Failure> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        generate_inner(backend, request, policy, read, verify_determinism, None)
    }
    #[cfg(target_arch = "wasm32")]
    {
        generate_wasm(backend, request, policy, read, verify_determinism)
    }
}

/// Same contract as [`generate`], while capturing opt-in per-stage durations.
#[cfg(not(target_arch = "wasm32"))]
pub fn generate_with_timings(
    backend: &dyn Backend,
    request: &BuildRequest,
    policy: &Policy,
    read: impl FnMut(&str) -> Option<Vec<u8>>,
    verify_determinism: bool,
) -> Result<(Generated, StageTimings), Failure> {
    let mut timings = StageTimings::default();
    let generated = generate_inner(
        backend,
        request,
        policy,
        read,
        verify_determinism,
        Some(&mut timings),
    )?;
    Ok((generated, timings))
}

#[cfg(not(target_arch = "wasm32"))]
fn preflight_handshake(
    backend: &dyn Backend,
    request: &BuildRequest,
    policy: &Policy,
) -> Result<crate::Descriptor, Failure> {
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
    Ok(descriptor)
}

#[cfg(not(target_arch = "wasm32"))]
fn generate_inner(
    backend: &dyn Backend,
    request: &BuildRequest,
    policy: &Policy,
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
    verify_determinism: bool,
    mut timings: Option<&mut StageTimings>,
) -> Result<Generated, Failure> {
    let protocol = Failure::Protocol;
    let t_handshake = timings.is_some().then(Instant::now);
    let descriptor = preflight_handshake(backend, request, policy)?;
    if let (Some(t), Some(record)) = (t_handshake, timings.as_deref_mut()) {
        record.handshake = t.elapsed();
    }

    let t_snapshot = timings.is_some().then(Instant::now);
    let mut inputs = snapshot(backend, request, &mut read)?;
    if let (Some(t), Some(record)) = (t_snapshot, timings.as_deref_mut()) {
        record.input_snapshot = t.elapsed();
    }

    let t_validate_req = timings.is_some().then(Instant::now);
    let validated = validate_request(request, &mut inputs, policy.limits);
    backend.check_cancelled().map_err(Failure::Rejected)?;
    validated.map_err(protocol)?;
    if let (Some(t), Some(record)) = (t_validate_req, timings.as_deref_mut()) {
        record.request_validation = t.elapsed();
    }

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

    let t_check = timings.is_some().then(Instant::now);
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
    if let (Some(t), Some(record)) = (t_check, timings.as_deref_mut()) {
        record.check_target = t.elapsed();
    }

    let t_gen1 = timings.is_some().then(Instant::now);
    let result = backend
        .generate(request, &mut inputs)
        .map_err(|d| refused(d, &mut inputs))?;
    if let (Some(t), Some(record)) = (t_gen1, timings.as_deref_mut()) {
        record.generate_first = t.elapsed();
    }

    let t_val1 = timings.is_some().then(Instant::now);
    if result
        .diagnostics
        .iter()
        .any(|d| d.severity == crate::Severity::Error)
    {
        diagnostics(request, &result.diagnostics, &mut inputs, policy.limits).map_err(protocol)?;
        return Err(Failure::Rejected(result.diagnostics));
    }
    let order = validate_result(request, &result, policy, &mut inputs).map_err(protocol)?;
    if let (Some(t), Some(record)) = (t_val1, timings.as_deref_mut()) {
        record.result_validation = t.elapsed();
    }

    if verify_determinism {
        let t_gen2 = timings.is_some().then(Instant::now);
        let repeated = backend
            .generate(request, &mut inputs)
            .map_err(|d| refused(d, &mut inputs))?;
        if let (Some(t), Some(record)) = (t_gen2, timings.as_deref_mut()) {
            record.generate_replay = t.elapsed();
        }

        let t_val2 = timings.is_some().then(Instant::now);
        validate_result(request, &repeated, policy, &mut inputs).map_err(protocol)?;
        if result != repeated {
            return Err(Failure::Unstable);
        }
        if let (Some(t), Some(record)) = (t_val2, timings.as_deref_mut()) {
            record.replay_validation = t.elapsed();
        }
    }

    let t_key = timings.is_some().then(Instant::now);
    let key = verify_determinism.then(|| cache_key(request));
    if let (Some(t), Some(record)) = (t_key, timings) {
        record.cache_key = t.elapsed();
    }

    Ok(Generated {
        result,
        order,
        cache_key: key,
    })
}

#[cfg(target_arch = "wasm32")]
fn generate_wasm(
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
