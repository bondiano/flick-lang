//! Dynamic clap CLI parsing with lazy backend-declared option attachment.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser};
use flick_backend::command::ValueKind;
use flick_backend::{Descriptor, OptionDescriptor, Verb};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{self, Project, workspace};
use flick_core::source::SourceMap;

#[cfg(feature = "wasm-backends")]
use super::cache::descriptor_for;
use super::errors::{ClapErrorContext, handle_clap_error};
use super::scan::{Scan, scan_argv};
use super::{active_for_verb, resolve_target, verb_name};
use crate::args::{CommandArguments, Commands};
#[cfg(feature = "wasm-backends")]
use crate::commands::contributed;
use crate::report;
use crate::sources::FsHost;

/// Result of parsing `flick`'s command line.
pub(crate) enum ParseOutcome {
    Parsed(Box<CommandArguments>),
    Exit(ExitCode),
}

fn discover_for_parse(
    host: &FsHost,
    explicit_project: Option<&str>,
    package: Option<&str>,
) -> (Option<Project>, Vec<Project>) {
    if let Some(explicit) = explicit_project {
        let mut sources = SourceMap::default();
        let found = project::discover(host, &mut sources, &host.cwd, Some(Path::new(explicit)))
            .ok()
            .flatten();
        return (found, Vec::new());
    }
    let mut sources = SourceMap::default();
    if let Ok(Some(ws)) = workspace::find(host, &mut sources, &host.cwd) {
        if let Some(name) = package {
            if let Some(member) = ws.member(name) {
                let mut msources = SourceMap::default();
                let found = project::discover(host, &mut msources, &host.cwd, Some(member))
                    .ok()
                    .flatten();
                return (found, Vec::new());
            }
            return (None, Vec::new());
        }
        if ws.root == host.cwd && ws.manifest.project.is_none() {
            let members: Vec<Project> = ws
                .members
                .iter()
                .filter_map(|m| {
                    let mut msources = SourceMap::default();
                    project::discover(host, &mut msources, &host.cwd, Some(m))
                        .ok()
                        .flatten()
                })
                .collect();
            return (None, members);
        }
    }
    let found = project::discover(host, &mut sources, &host.cwd, None)
        .ok()
        .flatten();
    (found, Vec::new())
}

fn core_long_flags(cmd: &clap::Command, sub_name: &str) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    for arg in cmd.get_arguments() {
        if let Some(long) = arg.get_long() {
            flags.insert(long.to_owned());
        }
        for alias in arg.get_all_aliases().into_iter().flatten() {
            flags.insert(alias.to_owned());
        }
    }
    if let Some(sub) = cmd.find_subcommand(sub_name) {
        for arg in sub.get_arguments() {
            if let Some(long) = arg.get_long() {
                flags.insert(long.to_owned());
            }
            for alias in arg.get_all_aliases().into_iter().flatten() {
                flags.insert(alias.to_owned());
            }
        }
    }
    flags.insert("help".into());
    flags.insert("version".into());
    flags
}

fn build_clap_arg(opt: &OptionDescriptor, heading: &str) -> Arg {
    let name = &opt.parameter.name;
    let id = format!("backend-opt:{name}");
    let mut arg = Arg::new(id)
        .long(name.clone())
        .help(opt.parameter.help.clone())
        .help_heading(heading.to_owned());
    match opt.parameter.kind {
        ValueKind::Flag => {
            arg = arg
                .action(ArgAction::Set)
                .value_name("BOOL")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("true")
                .value_parser(PossibleValuesParser::new(["true", "false"]))
                .hide_possible_values(true)
                .hide_default_value(true);
        }
        ValueKind::Text | ValueKind::Path => {
            let value_name = match opt.parameter.kind {
                ValueKind::Path => "PATH",
                ValueKind::Text => "VALUE",
                ValueKind::Flag => unreachable!(),
            };
            arg = arg.value_name(value_name);
            if opt.parameter.multiple {
                arg = arg.action(ArgAction::Append);
            } else {
                arg = arg.action(ArgAction::Set);
            }
            if let Some(implicit) = &opt.implicit_value {
                arg = arg.num_args(0..=1).default_missing_value(implicit.clone());
            }
            if !opt.choices.is_empty() {
                arg = arg.value_parser(PossibleValuesParser::new(opt.choices.clone()));
            }
            if let Some(default) = &opt.default {
                arg = arg.default_value(default.clone());
            }
        }
    }
    arg
}

struct ParseDescriptors {
    descriptors: Vec<Descriptor>,
    primary: Option<Descriptor>,
    fetch_error: Option<Diagnostic>,
}

fn collect_parse_descriptors(
    scan: &Scan,
    verb: Verb,
    project: Option<&Project>,
    virtual_members: &[Project],
) -> Result<ParseDescriptors, Diagnostic> {
    #[allow(unused_mut, reason = "mutated only with wasm-backends")]
    let mut descriptors: Vec<Descriptor> = Vec::new();
    #[allow(unused_mut, reason = "mutated only with wasm-backends")]
    let mut primary: Option<Descriptor> = None;
    #[allow(unused_mut, reason = "mutated only with wasm-backends")]
    let mut fetch_error: Option<Diagnostic> = None;

    let virtual_union = !virtual_members.is_empty()
        && !scan.help
        && matches!(verb, Verb::Build | Verb::Test)
        && scan.target.is_none();

    if virtual_union {
        #[cfg(feature = "wasm-backends")]
        {
            let mut seen_ids = BTreeSet::new();
            for member in virtual_members {
                if let Ok((choice, _)) = resolve_target(None, Some(member))
                    && seen_ids.insert(choice.id().to_owned())
                {
                    let (desc, err) =
                        descriptor_for(&choice, Some(member), !scan.help, scan.offline);
                    if let Some(desc) = desc {
                        descriptors.push(desc);
                    } else if fetch_error.is_none() {
                        fetch_error = err;
                    }
                }
            }
        }
    } else {
        let (choice, _) = resolve_target(scan.target.as_deref(), project)?;
        #[cfg(feature = "wasm-backends")]
        {
            let (desc, err) = descriptor_for(&choice, project, !scan.help, scan.offline);
            if let Some(desc) = desc {
                primary = Some(desc.clone());
                descriptors.push(desc);
            } else {
                fetch_error = err;
            }
        }
        #[cfg(not(feature = "wasm-backends"))]
        let _ = choice;
    }
    Ok(ParseDescriptors {
        descriptors,
        primary,
        fetch_error,
    })
}

fn attach_backend_args(
    mut cmd: clap::Command,
    sub_name: &str,
    verb: Verb,
    descriptors: &[Descriptor],
    core_flags: &BTreeSet<String>,
) -> (clap::Command, Vec<OptionDescriptor>, Vec<Diagnostic>) {
    let mut attached: Vec<OptionDescriptor> = Vec::new();
    let mut attached_names: BTreeSet<String> = BTreeSet::new();
    let mut collision_warnings: Vec<Diagnostic> = Vec::new();

    for desc in descriptors {
        let heading = format!("Options of `{}`", desc.identity.id);
        let mut to_add = Vec::new();
        for opt in &desc.options {
            if !active_for_verb(opt, verb) {
                continue;
            }
            let name = &opt.parameter.name;
            if core_flags.contains(name) {
                collision_warnings.push(Diagnostic::new(
                    codes::P058,
                    Severity::Warning,
                    format!(
                        "backend `{}` declares option `--{name}`, which collides with a core `flick` option: it is ignored",
                        desc.identity.id
                    ),
                ));
                continue;
            }
            if attached_names.insert(name.clone()) {
                to_add.push((opt.clone(), build_clap_arg(opt, &heading)));
            }
        }
        if !to_add.is_empty() {
            cmd = cmd.mut_subcommand(sub_name, |mut sub| {
                for (opt, arg) in to_add {
                    attached.push(opt);
                    sub = sub.arg(arg);
                }
                sub
            });
        }
    }
    (cmd, attached, collision_warnings)
}

fn extract_backend_options(
    matches: &clap::ArgMatches,
    sub_name: &str,
    attached: &[OptionDescriptor],
) -> BTreeMap<String, Vec<String>> {
    let mut backend_options: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(sub_matches) = matches.subcommand_matches(sub_name) {
        for opt in attached {
            let name = &opt.parameter.name;
            let id = format!("backend-opt:{name}");
            if sub_matches.value_source(&id) != Some(ValueSource::CommandLine) {
                continue;
            }
            let vals: Vec<String> = sub_matches
                .get_many::<String>(&id)
                .map(|vs| vs.cloned().collect())
                .unwrap_or_default();
            if !vals.is_empty() {
                backend_options.insert(name.clone(), vals);
            }
        }
    }
    backend_options
}

fn finish_parsed(
    matches: &clap::ArgMatches,
    sub_name: &str,
    attached: &[OptionDescriptor],
    collision_warnings: &[Diagnostic],
) -> ParseOutcome {
    let mut parsed = match CommandArguments::from_arg_matches(matches) {
        Ok(parsed) => parsed,
        Err(err) => {
            let _ = err.print();
            return ParseOutcome::Exit(ExitCode::from(2));
        }
    };

    if !collision_warnings.is_empty() {
        report::render(&parsed.global, &SourceMap::default(), collision_warnings);
    }

    let backend_options = extract_backend_options(matches, sub_name, attached);
    match &mut parsed.cmd {
        Commands::Build(o) => o.backend_options = backend_options,
        Commands::Test(o) => o.backend_options = backend_options,
        Commands::Run(o) | Commands::Dev(o) => o.backend_options = backend_options,
        Commands::Inspect(o) => o.backend_options = backend_options,
        _ => {}
    }

    ParseOutcome::Parsed(Box::new(parsed))
}

/// Parse `raw_argv` (including argv[0]), attaching backend-declared options for
/// the selected backend when the command is `build`, `run`, `dev`, `test`, or `inspect`.
pub(crate) fn parse_cli(raw_argv: &[OsString], project_override: Option<&str>) -> ParseOutcome {
    let tail = raw_argv.get(1..).unwrap_or(&[]);

    #[cfg(feature = "wasm-backends")]
    if project_override.is_none()
        && matches!(tail, [a] if a == "--help" || a == "-h" || a == "help")
    {
        let sections = contributed::help();
        let command = CommandArguments::command().after_help(sections);
        return finish_plain_command(command, raw_argv);
    }

    let scan = scan_argv(tail);
    let global_for_errors = scan.minimal_global();

    if !scan.help
        && let Some(diag) = scan.removed
    {
        report::render(&global_for_errors, &SourceMap::default(), &[diag]);
        return ParseOutcome::Exit(ExitCode::from(2));
    }

    let Some(verb) = scan.verb else {
        return parse_plain(raw_argv);
    };

    let sub_name = verb_name(verb);
    let base_cmd = CommandArguments::command();
    let core_flags = core_long_flags(&base_cmd, sub_name);

    // Non-`output` `inspect` invocations never load a backend; non-`--help`
    // invocations parse core flags first so `build --inputs`, `run <script>`,
    // and invocations without backend flags neither compile uncached backend
    // components nor hit the network during CLI parsing.
    let skip_backend_options =
        verb == Verb::Inspect && !scan.help && scan.emit.as_deref() != Some("output");
    if !scan.help {
        match base_cmd.clone().try_get_matches_from(raw_argv) {
            Ok(matches) => return finish_parsed(&matches, sub_name, &[], &[]),
            Err(err) if skip_backend_options || err.kind() != ErrorKind::UnknownArgument => {
                return handle_clap_error(
                    &err,
                    &ClapErrorContext {
                        verb,
                        global: &global_for_errors,
                        primary_descriptor: None,
                        fetch_error: None,
                        attached: &[],
                        core_flags: &core_flags,
                        project: None,
                    },
                );
            }
            Err(_) => {}
        }
    }

    let host = FsHost::new();
    let explicit_project = project_override.or(scan.project.as_deref());
    let (project, virtual_members) =
        discover_for_parse(&host, explicit_project, scan.package.as_deref());

    let ParseDescriptors {
        descriptors,
        primary: primary_descriptor,
        fetch_error,
    } = match collect_parse_descriptors(&scan, verb, project.as_ref(), &virtual_members) {
        Ok(found) => found,
        Err(diag) => {
            report::render(&global_for_errors, &SourceMap::default(), &[diag]);
            return ParseOutcome::Exit(ExitCode::from(2));
        }
    };

    let (cmd, attached, collision_warnings) =
        attach_backend_args(base_cmd, sub_name, verb, &descriptors, &core_flags);

    match cmd.try_get_matches_from(raw_argv) {
        Ok(matches) => finish_parsed(&matches, sub_name, &attached, &collision_warnings),
        Err(err) => handle_clap_error(
            &err,
            &ClapErrorContext {
                verb,
                global: &global_for_errors,
                primary_descriptor: primary_descriptor.as_ref(),
                fetch_error: fetch_error.as_ref(),
                attached: &attached,
                core_flags: &core_flags,
                project: project.as_ref(),
            },
        ),
    }
}

#[cfg(feature = "wasm-backends")]
fn finish_plain_command(command: clap::Command, raw_argv: &[OsString]) -> ParseOutcome {
    match command.try_get_matches_from(raw_argv) {
        Ok(matches) => match CommandArguments::from_arg_matches(&matches) {
            Ok(parsed) => ParseOutcome::Parsed(Box::new(parsed)),
            Err(err) => {
                let _ = err.print();
                ParseOutcome::Exit(ExitCode::from(2))
            }
        },
        Err(err) => {
            let code = if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
            let _ = err.print();
            ParseOutcome::Exit(code)
        }
    }
}

fn parse_plain(raw_argv: &[OsString]) -> ParseOutcome {
    match CommandArguments::try_parse_from(raw_argv) {
        Ok(parsed) => ParseOutcome::Parsed(Box::new(parsed)),
        Err(err) => {
            let code = if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
            let _ = err.print();
            ParseOutcome::Exit(code)
        }
    }
}
