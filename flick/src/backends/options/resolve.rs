//! Validation and merging of `[target.<short>]` and CLI backend options.

use std::collections::BTreeMap;

use flick_backend::command::ValueKind;
use flick_backend::{Descriptor, OptionDescriptor, Verb};
use flick_core::diagnostics::{Diagnostic, Severity, codes, suggest};
use flick_core::project::Project;
use flick_core::project::config::TargetValue;
use flick_core::source::Span;

#[cfg(feature = "wasm-backends")]
use super::cache::all_known_descriptors;
use super::{ResolvedOptions, active_for_verb, format_verbs, short_name, verb_name};

fn validate_cli_options(
    descriptor: &Descriptor,
    declared: &BTreeMap<&str, &OptionDescriptor>,
    verb: Verb,
    project: Option<&Project>,
    cli: &BTreeMap<String, Vec<String>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let id = &descriptor.identity.id;
    for (key, values) in cli {
        let Some(opt) = declared.get(key.as_str()) else {
            diagnostics.push(unknown_cli_option(id, verb, key, descriptor, &[], project));
            continue;
        };
        if !active_for_verb(opt, verb) {
            diagnostics.push(wrong_verb_option(id, verb, opt));
            continue;
        }
        if values.len() > 1 && !opt.parameter.multiple {
            diagnostics.push(Diagnostic::new(
                codes::P058,
                Severity::Error,
                format!("option `--{key}` for `{id}` may be passed only once"),
            ));
            continue;
        }
        for value in values {
            if opt.parameter.kind == ValueKind::Flag && !matches!(value.as_str(), "true" | "false")
            {
                diagnostics.push(Diagnostic::new(
                    codes::P058,
                    Severity::Error,
                    format!("invalid boolean value `{value}` for `--{key}` of `{id}`"),
                ));
            } else if !opt.choices.is_empty() && !opt.choices.contains(value) {
                diagnostics.push(invalid_choice_diagnostic(id, opt, value, None));
            }
        }
    }
}

/// Validate `[target.<short>]` and CLI backend options against `descriptor`,
/// merging active options for `verb` with precedence CLI > `flick.toml` > default.
pub(crate) fn resolve(
    descriptor: &Descriptor,
    verb: Verb,
    project: Option<&Project>,
    cli: &BTreeMap<String, Vec<String>>,
) -> Result<ResolvedOptions, Vec<Diagnostic>> {
    let id = &descriptor.identity.id;
    let short = short_name(id);
    let declared: BTreeMap<&str, &OptionDescriptor> = descriptor
        .options
        .iter()
        .map(|o| (o.parameter.name.as_str(), o))
        .collect();
    let mut diagnostics = Vec::new();
    let mut toml_values: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();

    if let Some(table) = project.and_then(|p| p.config.targets.get(short)) {
        for (key, entry) in table {
            let Some(opt) = declared.get(key.as_str()) else {
                diagnostics.push(unknown_toml_option(
                    id,
                    short,
                    key,
                    entry.key_span,
                    descriptor,
                    project,
                ));
                continue;
            };
            match decode_toml_value(id, opt, &entry.value, entry.value_span) {
                Ok(decoded) => {
                    toml_values.insert(key.clone(), decoded);
                }
                Err(mut errs) => diagnostics.append(&mut errs),
            }
        }
    }

    validate_cli_options(descriptor, &declared, verb, project, cli, &mut diagnostics);

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let mut values = BTreeMap::new();
    for opt in &descriptor.options {
        if !active_for_verb(opt, verb) {
            continue;
        }
        let name = &opt.parameter.name;
        if let Some(cli_vals) = cli.get(name) {
            values.insert(name.clone(), cli_vals.clone());
        } else if let Some(toml_val) = toml_values.get(name) {
            if let Some(vals) = toml_val {
                values.insert(name.clone(), vals.clone());
            }
        } else if let Some(default) = &opt.default {
            values.insert(name.clone(), vec![default.clone()]);
        } else if opt.parameter.required {
            diagnostics.push(Diagnostic::new(
                codes::P058,
                Severity::Error,
                format!(
                    "missing required option `--{name}` for `{id}` on `{}`",
                    verb_name(verb)
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(ResolvedOptions { values })
    } else {
        Err(diagnostics)
    }
}

fn decode_toml_value(
    id: &str,
    opt: &OptionDescriptor,
    value: &TargetValue,
    span: Span,
) -> Result<Option<Vec<String>>, Vec<Diagnostic>> {
    let name = &opt.parameter.name;
    match opt.parameter.kind {
        ValueKind::Flag => match value {
            TargetValue::Bool(b) => Ok(Some(vec![if *b { "true" } else { "false" }.to_owned()])),
            _ => Err(vec![Diagnostic::error(
                codes::P058,
                format!("option `{name}` of `{id}` expects a boolean"),
                span,
            )]),
        },
        ValueKind::Text | ValueKind::Path => {
            if opt.parameter.multiple {
                match value {
                    TargetValue::List(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        let mut errs = Vec::new();
                        for (item, item_span) in items {
                            if !opt.choices.is_empty() && !opt.choices.contains(item) {
                                errs.push(invalid_choice_diagnostic(
                                    id,
                                    opt,
                                    item,
                                    Some(*item_span),
                                ));
                            } else {
                                out.push(item.clone());
                            }
                        }
                        if errs.is_empty() {
                            Ok(Some(out))
                        } else {
                            Err(errs)
                        }
                    }
                    _ => Err(vec![Diagnostic::error(
                        codes::P058,
                        format!("option `{name}` of `{id}` expects a list of strings"),
                        span,
                    )]),
                }
            } else {
                match value {
                    TargetValue::String(text) => {
                        if !opt.choices.is_empty() && !opt.choices.contains(text) {
                            Err(vec![invalid_choice_diagnostic(id, opt, text, Some(span))])
                        } else {
                            Ok(Some(vec![text.clone()]))
                        }
                    }
                    TargetValue::Bool(true) if opt.implicit_value.is_some() => {
                        Ok(Some(vec![opt.implicit_value.clone().expect("checked")]))
                    }
                    TargetValue::Bool(false) if opt.implicit_value.is_some() => Ok(None),
                    _ => {
                        let expected = if opt.implicit_value.is_some() {
                            "a string or boolean"
                        } else {
                            "a string"
                        };
                        Err(vec![Diagnostic::error(
                            codes::P058,
                            format!("option `{name}` of `{id}` expects {expected}"),
                            span,
                        )])
                    }
                }
            }
        }
    }
}

pub(super) fn invalid_choice_diagnostic(
    id: &str,
    opt: &OptionDescriptor,
    value: &str,
    span: Option<Span>,
) -> Diagnostic {
    let allowed = opt
        .choices
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let message = format!(
        "invalid value `{value}` for option `{}` of `{id}`; expected one of: {allowed}",
        opt.parameter.name
    );
    let mut diag = match span {
        Some(span) => Diagnostic::error(codes::P058, message, span),
        None => Diagnostic::new(codes::P058, Severity::Error, message),
    };
    if let Some(suggestion) = suggest::closest(value, opt.choices.iter().map(String::as_str)) {
        if let Some(span) = span {
            diag = diag.did_you_mean(span, || Some(format!("\"{suggestion}\"")));
        } else {
            diag = diag.with_fix(|| (format!("did you mean `{suggestion}`?"), Vec::new()));
        }
    }
    diag
}

fn wrong_verb_option(id: &str, verb: Verb, opt: &OptionDescriptor) -> Diagnostic {
    let name = &opt.parameter.name;
    let available = format_verbs(&opt.verbs);
    Diagnostic::new(
        codes::P058,
        Severity::Error,
        format!(
            "option `--{name}` of `{id}` is not available for `flick {}`",
            verb_name(verb)
        ),
    )
    .with_fix(|| {
        (
            format!("`--{name}` is available for {available}"),
            Vec::new(),
        )
    })
}

fn unknown_toml_option(
    id: &str,
    short: &str,
    key: &str,
    span: Span,
    descriptor: &Descriptor,
    project: Option<&Project>,
) -> Diagnostic {
    let mut diag = Diagnostic::error(
        codes::P058,
        format!("unknown option `{key}` in `[target.{short}]` for `{id}`"),
        span,
    );
    let names = descriptor.options.iter().map(|o| o.parameter.name.as_str());
    if let Some(suggestion) = suggest::closest(key, names) {
        diag = diag.did_you_mean(span, || Some(suggestion.to_owned()));
    } else if let Some(other) = find_other_backend_with_option(id, key, project) {
        let other_short = short_name(&other);
        diag = diag.with_fix(|| {
            (
                format!(
                    "backend `{other}` declares `{key}`: put it under `[target.{other_short}]`"
                ),
                Vec::new(),
            )
        });
    }
    diag
}

pub(super) fn unknown_cli_option(
    id: &str,
    verb: Verb,
    flag: &str,
    descriptor: &Descriptor,
    core_flags: &[String],
    project: Option<&Project>,
) -> Diagnostic {
    if let Some(opt) = descriptor
        .options
        .iter()
        .find(|o| o.parameter.name == flag && !active_for_verb(o, verb))
    {
        return wrong_verb_option(id, verb, opt);
    }
    let mut diag = Diagnostic::new(
        codes::P058,
        Severity::Error,
        format!(
            "unknown option `--{flag}` for `{id}` on `flick {}`",
            verb_name(verb)
        ),
    );
    let candidates: Vec<&str> = descriptor
        .options
        .iter()
        .filter(|o| active_for_verb(o, verb))
        .map(|o| o.parameter.name.as_str())
        .chain(core_flags.iter().map(String::as_str))
        .collect();
    if let Some(suggestion) = suggest::closest(flag, candidates) {
        diag = diag.with_fix(|| (format!("did you mean `--{suggestion}`?"), Vec::new()));
    } else if let Some(other) = find_other_backend_with_option(id, flag, project) {
        let other_short = short_name(&other);
        diag = diag.with_fix(|| {
            (
                format!(
                    "backend `{other}` declares `--{flag}`: select it with `--target {other_short}`"
                ),
                Vec::new(),
            )
        });
    }
    diag
}

fn find_other_backend_with_option(
    current_id: &str,
    option_name: &str,
    project: Option<&Project>,
) -> Option<String> {
    #[cfg(feature = "wasm-backends")]
    {
        for descriptor in all_known_descriptors(project) {
            if descriptor.identity.id != current_id
                && descriptor
                    .options
                    .iter()
                    .any(|o| o.parameter.name == option_name)
            {
                return Some(descriptor.identity.id);
            }
        }
    }
    #[cfg(not(feature = "wasm-backends"))]
    let _ = (current_id, option_name, project);
    None
}
