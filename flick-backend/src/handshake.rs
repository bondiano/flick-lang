use std::collections::{BTreeMap, BTreeSet};

use semver::Version;

use crate::{AdapterArgKind, BackendPin, Descriptor, TargetRequest, ValueKind, Verb, VersionRange};

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

fn is_tag_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn is_ident_word(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

const CORE_FOREIGN_WORDS: &[&str] = &[
    "arity",
    "catch",
    "retention",
    "nullable",
    "optional",
    "reviewed",
    "owned",
];

const CORE_ATTRIBUTES: &[&str] = &[
    "lang", "derive", "maker", "cell", "label", "owner", "sync", "stores", "implicit", "when",
    "export",
];

/// Validate the foreign adapter words and type-form attributes a backend
/// publishes in `describe`.
pub fn validate_foreign_metadata(descriptor: &Descriptor) -> Result<(), String> {
    let mut seen_words = BTreeSet::new();
    for word in &descriptor.foreign_adapters {
        if !is_ident_word(&word.name) {
            return Err(format!(
                "invalid foreign adapter word `{}`: adapter words start with a lowercase letter and use lowercase letters, digits, or `_`",
                word.name
            ));
        }
        if CORE_FOREIGN_WORDS.contains(&word.name.as_str()) {
            return Err(format!(
                "foreign adapter word `{}` collides with a core contract word",
                word.name
            ));
        }
        if !seen_words.insert(word.name.as_str()) {
            return Err(format!("duplicate foreign adapter word `{}`", word.name));
        }
        for arg in &word.args {
            if arg.name.is_empty() {
                return Err(format!(
                    "foreign adapter word `{}` has an argument with an empty name",
                    word.name
                ));
            }
            if let AdapterArgKind::Keyword(choices) = &arg.kind {
                if choices.is_empty() {
                    return Err(format!(
                        "foreign adapter word `{}` keyword argument `{}` must declare at least one choice",
                        word.name, arg.name
                    ));
                }
                let mut seen_choices = BTreeSet::new();
                for choice in choices {
                    if !is_ident_word(choice) || !seen_choices.insert(choice.as_str()) {
                        return Err(format!(
                            "foreign adapter word `{}` keyword argument `{}` has an invalid or duplicate choice `{choice}`",
                            word.name, arg.name
                        ));
                    }
                }
            }
        }
    }
    let mut seen_attrs = BTreeSet::new();
    for attr in &descriptor.type_form_attributes {
        if !is_ident_word(&attr.name) {
            return Err(format!(
                "invalid type-form attribute `@{}`: attributes start with a lowercase letter and use lowercase letters, digits, or `_`",
                attr.name
            ));
        }
        if CORE_ATTRIBUTES.contains(&attr.name.as_str()) {
            return Err(format!(
                "type-form attribute `@{}` collides with a core attribute",
                attr.name
            ));
        }
        if !seen_attrs.insert(attr.name.as_str()) {
            return Err(format!("duplicate type-form attribute `@{}`", attr.name));
        }
    }
    Ok(())
}

/// Validate the environment tags and axes a backend publishes in `describe`.
pub fn validate_tags(descriptor: &Descriptor) -> Result<(), String> {
    let mut seen_tags = BTreeSet::new();
    for tag in &descriptor.tags {
        if !is_tag_name(tag) {
            return Err(format!(
                "invalid environment tag `{tag}`: tags start with a lowercase letter and use lowercase letters, digits, `-`, or `_`"
            ));
        }
        if !seen_tags.insert(tag.as_str()) {
            return Err(format!("duplicate environment tag `{tag}`"));
        }
    }
    let mut seen_axes = BTreeSet::new();
    for axis in &descriptor.axes {
        if !is_tag_name(&axis.name) {
            return Err(format!(
                "invalid tag axis name `{}`: use lowercase ASCII letters, digits, `-`, or `_`",
                axis.name
            ));
        }
        if !seen_axes.insert(axis.name.as_str()) {
            return Err(format!("duplicate tag axis `{}`", axis.name));
        }
        if axis.tags.is_empty() {
            return Err(format!(
                "tag axis `{}` must declare at least one tag",
                axis.name
            ));
        }
        for tag in &axis.tags {
            if !is_tag_name(tag) {
                return Err(format!(
                    "invalid tag `{tag}` on axis `{}`: tags start with a lowercase letter and use lowercase letters, digits, `-`, or `_`",
                    axis.name
                ));
            }
            if !seen_tags.insert(tag.as_str()) {
                return Err(format!(
                    "duplicate environment tag `{tag}` on axis `{}`",
                    axis.name
                ));
            }
        }
        if let Some(default) = &axis.default
            && !axis.tags.contains(default)
        {
            return Err(format!(
                "default tag `{default}` on axis `{}` is not in its tag list",
                axis.name
            ));
        }
    }
    Ok(())
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

fn validate_request_environment(
    descriptor: &Descriptor,
    request: &TargetRequest,
) -> Result<(), String> {
    let env_supported = if request.host_package.is_some() {
        let short = descriptor
            .identity
            .id
            .rsplit('/')
            .next()
            .unwrap_or(&descriptor.identity.id);
        request.environment.backend == short
            || descriptor
                .environments
                .iter()
                .any(|e| e.backend == request.environment.backend)
            || descriptor.tags.contains(&request.environment.backend)
    } else {
        descriptor.environments.contains(&request.environment)
    };
    if !env_supported
        || !descriptor.profiles.contains(&request.profile)
        || !descriptor.modes.contains(&request.mode)
    {
        return Err("unsupported environment, profile or compilation mode".into());
    }
    for tag in &request.tags {
        if !is_tag_name(tag) {
            return Err(format!("invalid environment tag `{tag}` in target request"));
        }
    }
    Ok(())
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
    validate_tags(descriptor)?;
    validate_options(descriptor)?;
    validate_foreign_metadata(descriptor)?;
    crate::validate_value_abi(descriptor)?;
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
    validate_request_environment(descriptor, request)?;
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
