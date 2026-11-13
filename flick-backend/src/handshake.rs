use semver::Version;

use crate::{BackendPin, Descriptor, TargetRequest, VersionRange};

fn accepts(range: &VersionRange, version: &str) -> bool {
    let (Ok(min), Ok(max), Ok(version)) = (
        Version::parse(&range.minimum),
        Version::parse(&range.maximum_exclusive),
        Version::parse(version),
    ) else {
        return false;
    };
    // A release range never implicitly accepts prereleases.
    version.pre.is_empty() && min <= version && version < max
}

/// Check identity, protocol ranges and static capabilities before invoking
/// target-specific preflight or generation. No fallback backend is chosen.
pub fn handshake(
    pin: &BackendPin,
    descriptor: &Descriptor,
    request: &TargetRequest,
) -> Result<(), String> {
    if pin.identity != descriptor.identity {
        return Err("backend identity does not match the project pin".into());
    }
    for (name, range, version) in [
        ("backend API", &descriptor.backend_api, &request.backend_api),
        ("IR", &descriptor.ir, &request.ir_version),
        ("language", &descriptor.language, &request.language_version),
    ] {
        if !accepts(range, version) {
            return Err(format!("incompatible {name} version `{version}`"));
        }
    }
    for required in &request.runtime_abis {
        if !descriptor
            .runtime_abis
            .iter()
            .any(|abi| abi.name == required.name && accepts(&abi.versions, &required.version))
        {
            return Err(format!("incompatible runtime ABI `{}`", required.name));
        }
    }
    if !descriptor.environments.contains(&request.environment)
        || !descriptor.profiles.contains(&request.profile)
        || !descriptor.modes.contains(&request.mode)
    {
        return Err("unsupported environment, profile or compilation mode".into());
    }
    for capability in &request.required_capabilities {
        if !descriptor.capabilities.contains(capability) {
            return Err(format!("unsupported capability `{}`", capability.name));
        }
    }
    Ok(())
}
