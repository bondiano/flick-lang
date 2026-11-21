use std::collections::{BTreeMap, BTreeSet};

use semver::Version;

use crate::{BackendPin, Descriptor, TargetRequest, ValueKind, Verb, VersionRange};

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

fn is_option_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validate the option descriptors a backend publishes in `describe`.
pub fn validate_options(descriptor: &Descriptor) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for option in &descriptor.options {
        let name = &option.parameter.name;
        if !is_option_name(name) {
            return Err(format!(
                "invalid option name `{name}`: use lowercase ASCII kebab-case"
            ));
        }
        if !names.insert(name.as_str()) {
            return Err(format!("duplicate backend option `{name}`"));
        }
        let mut verbs = Vec::new();
        for &verb in &option.verbs {
            if verbs.contains(&verb) {
                return Err(format!("duplicate verb for backend option `{name}`"));
            }
            verbs.push(verb);
        }
        if option.parameter.kind == ValueKind::Flag {
            if option.parameter.multiple {
                return Err(format!("flag option `{name}` cannot set `multiple = true`"));
            }
            if !option.choices.is_empty() {
                return Err(format!("flag option `{name}` cannot declare `choices`"));
            }
            if option.implicit_value.is_some() {
                return Err(format!(
                    "flag option `{name}` cannot declare `implicit-value`"
                ));
            }
            if !matches!(option.default.as_deref(), None | Some("true" | "false")) {
                return Err(format!(
                    "flag option `{name}` default must be `\"true\"` or `\"false\"`"
                ));
            }
        }
        if !option.choices.is_empty() {
            if option.parameter.kind != ValueKind::Text {
                return Err(format!(
                    "option `{name}` declares `choices` on a non-text parameter"
                ));
            }
            let mut seen_choices = BTreeSet::new();
            for choice in &option.choices {
                if choice.is_empty() || !seen_choices.insert(choice.as_str()) {
                    return Err(format!(
                        "option `{name}` has an empty or duplicate choice `{choice}`"
                    ));
                }
            }
            if let Some(default) = &option.default
                && !option.choices.contains(default)
            {
                return Err(format!(
                    "option `{name}` default `{default}` is not in `choices`"
                ));
            }
            if let Some(implicit) = &option.implicit_value
                && !option.choices.contains(implicit)
            {
                return Err(format!(
                    "option `{name}` implicit-value `{implicit}` is not in `choices`"
                ));
            }
        }
        if option.parameter.multiple && option.implicit_value.is_some() {
            return Err(format!(
                "option `{name}` cannot set both `multiple = true` and `implicit-value`"
            ));
        }
    }
    Ok(())
}

fn verb_of(command: &str) -> Option<Verb> {
    match command {
        "build" => Some(Verb::Build),
        "run" => Some(Verb::Run),
        "dev" => Some(Verb::Dev),
        "test" => Some(Verb::Test),
        "inspect" => Some(Verb::Inspect),
        _ => None,
    }
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
    validate_options(descriptor)?;
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
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &request.options {
        let Some(name) = entry.key.strip_prefix("option.") else {
            continue;
        };
        let Some(declared) = descriptor.options.iter().find(|o| o.parameter.name == name) else {
            return Err(format!("unknown backend option `{name}`"));
        };
        let count = counts.entry(name).or_insert(0);
        *count += 1;
        if *count > 1 && !declared.parameter.multiple {
            return Err(format!("duplicate backend option `{name}`"));
        }
        if declared.parameter.kind == ValueKind::Flag
            && !matches!(entry.value.as_str(), "true" | "false")
        {
            return Err(format!(
                "invalid boolean value `{}` for backend option `{name}`",
                entry.value
            ));
        }
        if !declared.choices.is_empty() && !declared.choices.contains(&entry.value) {
            return Err(format!(
                "invalid value `{}` for backend option `{name}`",
                entry.value
            ));
        }
    }
    if let Some(verb) = request
        .options
        .iter()
        .find(|o| o.key == "command")
        .and_then(|o| verb_of(&o.value))
    {
        for declared in &descriptor.options {
            if declared.parameter.required
                && declared.default.is_none()
                && (declared.verbs.is_empty() || declared.verbs.contains(&verb))
                && !counts.contains_key(declared.parameter.name.as_str())
            {
                return Err(format!(
                    "missing required backend option `{}`",
                    declared.parameter.name
                ));
            }
        }
    }
    Ok(())
}
