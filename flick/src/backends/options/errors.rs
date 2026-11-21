//! Mapping clap parse failures into structured `FLICK-P058` / `FLICK-P056`
//! diagnostics.

use std::collections::BTreeSet;
use std::process::ExitCode;
#[cfg(feature = "wasm-backends")]
use std::slice;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use flick_backend::command::ValueKind;
use flick_backend::{Descriptor, OptionDescriptor, Verb};
use flick_core::diagnostics::{Diagnostic, Severity, codes, suggest};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::cli::ParseOutcome;
use super::resolve::{invalid_choice_diagnostic, unknown_cli_option};
use super::verb_name;
use crate::args::GlobalArguments;
use crate::report;

pub(super) struct ClapErrorContext<'a> {
    pub(super) verb: Verb,
    pub(super) global: &'a GlobalArguments,
    pub(super) primary_descriptor: Option<&'a Descriptor>,
    pub(super) fetch_error: Option<&'a Diagnostic>,
    pub(super) attached: &'a [OptionDescriptor],
    pub(super) core_flags: &'a BTreeSet<String>,
    pub(super) project: Option<&'a Project>,
}

pub(super) fn handle_clap_error(err: &clap::Error, ctx: &ClapErrorContext<'_>) -> ParseOutcome {
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        let _ = err.print();
        return ParseOutcome::Exit(ExitCode::SUCCESS);
    }
    #[cfg(feature = "wasm-backends")]
    if ctx.primary_descriptor.is_none()
        && err.kind() == ErrorKind::UnknownArgument
        && let Some(diag) = ctx.fetch_error
    {
        let exit = report::exit_code(slice::from_ref(diag));
        report::render(ctx.global, &SourceMap::default(), slice::from_ref(diag));
        return ParseOutcome::Exit(exit);
    }
    #[cfg(not(feature = "wasm-backends"))]
    let _ = ctx.fetch_error;
    if let Some(diag) = map_clap_error(
        err,
        ctx.verb,
        ctx.primary_descriptor,
        ctx.attached,
        ctx.core_flags,
        ctx.project,
    ) {
        report::render(ctx.global, &SourceMap::default(), &[diag]);
        return ParseOutcome::Exit(ExitCode::from(2));
    }
    let _ = err.print();
    ParseOutcome::Exit(ExitCode::from(2))
}

fn map_clap_error(
    err: &clap::Error,
    verb: Verb,
    descriptor: Option<&Descriptor>,
    attached: &[OptionDescriptor],
    core_flags: &BTreeSet<String>,
    project: Option<&Project>,
) -> Option<Diagnostic> {
    let core_list: Vec<String> = core_flags.iter().cloned().collect();
    match err.kind() {
        ErrorKind::UnknownArgument => {
            let raw = match err.get(ContextKind::InvalidArg)? {
                ContextValue::String(s) => s.as_str(),
                _ => return None,
            };
            let flag = raw.strip_prefix("--")?;
            let flag = flag.split_once('=').map_or(flag, |(k, _)| k);
            if let Some(desc) = descriptor {
                Some(unknown_cli_option(
                    &desc.identity.id,
                    verb,
                    flag,
                    desc,
                    &core_list,
                    project,
                ))
            } else {
                let mut diag = Diagnostic::new(
                    codes::P058,
                    Severity::Error,
                    format!("unknown option `--{flag}` on `flick {}`", verb_name(verb)),
                );
                let candidates: Vec<&str> = attached
                    .iter()
                    .map(|o| o.parameter.name.as_str())
                    .chain(core_list.iter().map(String::as_str))
                    .collect();
                if let Some(suggestion) = suggest::closest(flag, candidates) {
                    diag =
                        diag.with_fix(|| (format!("did you mean `--{suggestion}`?"), Vec::new()));
                }
                Some(diag)
            }
        }
        ErrorKind::InvalidValue => {
            let raw_arg = match err.get(ContextKind::InvalidArg)? {
                ContextValue::String(s) => s.as_str(),
                _ => return None,
            };
            let flag = raw_arg
                .strip_prefix("--")?
                .split([' ', '=', '['])
                .next()
                .unwrap_or("");
            let opt = attached.iter().find(|o| o.parameter.name == flag)?;
            let invalid = match err.get(ContextKind::InvalidValue)? {
                ContextValue::String(s) => s.as_str(),
                _ => return None,
            };
            let id = descriptor.map_or("backend", |d| d.identity.id.as_str());
            if opt.parameter.kind == ValueKind::Flag {
                return Some(Diagnostic::new(
                    codes::P058,
                    Severity::Error,
                    format!(
                        "invalid boolean value `{invalid}` for option `{flag}` of `{id}`; expected `true` or `false`"
                    ),
                ));
            }
            if opt.choices.is_empty() {
                return None;
            }
            Some(invalid_choice_diagnostic(id, opt, invalid, None))
        }
        _ => None,
    }
}
