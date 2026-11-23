use std::env;
use std::ffi::OsString;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::SourceMap;

use crate::args::{CheckArguments, CommandArguments, Commands, GlobalArguments, ProveArguments};
use crate::command::{Command, Outcome, finish as report, run_command};
use crate::commands::{BuildCommand, CheckCommand, FmtCommand, ScaffoldCommand, Verb};

mod args;
mod artifact;
mod backends;
mod builtins;
mod cache;
mod child;
mod command;
mod commands;
#[cfg(feature = "wasm-backends")]
mod component;
mod fetch;
mod lsp;
mod package_cache;
mod plugins;
mod progress;
mod publish;
mod report;
mod saved;
mod sources;
mod stdlib;
mod timings;
mod toolchain;
mod workspace;

/// Runs `command` again after every change of the project's files.
fn watch(command: &impl Command, global: &GlobalArguments) -> ExitCode {
    let paths = commands::watched(global);
    loop {
        let before = commands::settled_snapshot(&paths);
        run_command(command, global);
        timings::report(global);
        // once a build has used a backend, a signal cancels it instead of ending `flick`
        while !child::interrupted() && commands::watch_snapshot(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        if child::interrupted() {
            return ExitCode::from(130);
        }
        report::log(global, "a file changed: running again");
    }
}

/// The dependency verb of a CLI command.
fn dependency_command(cmd: Commands, offline: bool) -> Verb {
    match cmd {
        Commands::Install(o) => Verb::Install {
            locked: o.locked,
            offline,
        },
        Commands::Add(o) => Verb::Add {
            packages: o.packages,
            dev: o.dev,
        },
        Commands::Remove(o) => Verb::Remove {
            packages: o.packages,
        },
        Commands::Update(o) => Verb::Update {
            packages: o.packages,
        },
        Commands::Deps(o) => Verb::Deps { why: o.why },
        _ => unreachable!("only dependency commands"),
    }
}

fn main() -> ExitCode {
    let raw_argv: Vec<OsString> = env::args_os().collect();
    let args = match backends::options::parse_cli(&raw_argv, None) {
        backends::options::ParseOutcome::Parsed(args) => *args,
        backends::options::ParseOutcome::Exit(code) => return code,
    };
    builtins::keep_modules();
    let global = args.global.clone();
    // the language server sets its own subscriber for its log
    if global.timings && !matches!(args.cmd, Commands::Lsp(_)) {
        timings::install();
    }
    let code = run(args, &raw_argv);
    timings::report(&global);
    code
}

/// Runs the command of `args` on its project, or on each member of its workspace.
fn run(args: CommandArguments, raw_argv: &[OsString]) -> ExitCode {
    plugins::set_offline(args.global.offline || env::var_os("CI").is_some());
    backends::allow_tools(args.global.allow_tool.clone());
    backends::set_offline(args.global.offline);
    // what installs or edits the dependencies, or checks no module, needs no `std`
    let needs_std = !matches!(
        args.cmd,
        Commands::Fmt(_)
            | Commands::New(_)
            | Commands::Init(_)
            | Commands::Clean(_)
            | Commands::Install(_)
            | Commands::Add(_)
            | Commands::Remove(_)
            | Commands::Update(_)
            | Commands::Deps(_)
            | Commands::Plugins
            | Commands::Explain(_)
            | Commands::Doctor(_)
            | Commands::External(_)
    );
    if needs_std && let Err(message) = stdlib::prepare(args.global.project.as_deref()) {
        let diagnostic = Diagnostic::new(codes::P034, Severity::Error, message);
        let outcome = Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        };
        return report(outcome, &args.global);
    }
    if !matches!(
        args.cmd,
        Commands::Fmt(_)
            | Commands::New(_)
            | Commands::Init(_)
            | Commands::Clean(_)
            | Commands::Build(_)
            | Commands::Check(_)
            | Commands::Prove(_)
            | Commands::Test(_)
            | Commands::Install(_)
            | Commands::Add(_)
            | Commands::Remove(_)
            | Commands::Update(_)
            | Commands::Deps(_)
            | Commands::Pack(_)
            | Commands::Plugins
            | Commands::Explain(_)
            | Commands::Doctor(_)
            | Commands::External(_)
            // after its logging starts
            | Commands::Lsp(_)
    ) && let Err(diagnostic) = builtins::prepare()
    {
        let outcome = Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        };
        return report(outcome, &args.global);
    }
    let members = match workspace::members(&args) {
        Ok(Some(members)) => members,
        Ok(None) => return dispatch(args),
        Err(outcome) => return report(outcome, &args.global),
    };
    let several = members.len() > 1;
    let mut code = ExitCode::SUCCESS;
    for member in members {
        let mut args = match backends::options::parse_cli(raw_argv, Some(&member)) {
            backends::options::ParseOutcome::Parsed(args) => *args,
            backends::options::ParseOutcome::Exit(exit) => {
                if code == ExitCode::SUCCESS {
                    code = exit;
                }
                continue;
            }
        };
        if several {
            report::log(&args.global, &format!("in `{member}`"));
        }
        args.global.project = Some(member);
        let done = dispatch(args);
        if code == ExitCode::SUCCESS {
            code = done;
        }
    }
    code
}

/// Runs the command on its project.
/// `flick prove`: `flick check` of the proof root, or of the paths given.
fn prove(options: &ProveArguments, global: &GlobalArguments) -> ExitCode {
    let Some(paths) = commands::proof_paths(&options.paths, global.project.as_deref()) else {
        return ExitCode::SUCCESS;
    };
    let options = CheckArguments {
        paths,
        watch: options.watch,
        fix: false,
        only: Vec::new(),
    };
    let watching = options.watch;
    let command = CheckCommand::new(options, global.project.clone());
    if watching {
        watch(&command, global)
    } else {
        run_command(&command, global)
    }
}

fn dispatch(args: CommandArguments) -> ExitCode {
    let global = &args.global;
    let project = global.project.as_deref();
    match args.cmd {
        Commands::Check(options) => {
            let watching = options.watch;
            let command = CheckCommand::new(options, global.project.clone());
            if watching {
                watch(&command, global)
            } else {
                run_command(&command, global)
            }
        }
        Commands::Prove(options) => prove(&options, global),
        Commands::Build(options) => {
            let watching = options.watch;
            let command = BuildCommand::new(
                options,
                global.project.clone(),
                Some(progress::Mode::for_cli(global)),
            );
            if watching {
                watch(&command, global)
            } else {
                run_command(&command, global)
            }
        }
        Commands::Fmt(options) => {
            run_command(&FmtCommand::new(options, global.project.clone()), global)
        }
        Commands::Test(options) => commands::test(&options, global),
        Commands::New(options) => run_command(
            &ScaffoldCommand {
                dir: options.directory.into(),
                options: options.scaffold,
                fresh: true,
                offline: global.offline,
            },
            global,
        ),
        Commands::Init(options) => run_command(
            &ScaffoldCommand {
                dir: ".".into(),
                options: options.scaffold,
                fresh: false,
                offline: global.offline,
            },
            global,
        ),
        Commands::Run(options) => commands::run(&options, global),
        Commands::Dev(options) => commands::dev(&options, global),
        Commands::Clean(options) => report(commands::clean(&options, global), global),
        Commands::Add(o) if o.source().is_some() => {
            report(commands::add_library(&o, global), global)
        }
        Commands::Install(_)
        | Commands::Add(_)
        | Commands::Remove(_)
        | Commands::Update(_)
        | Commands::Deps(_) => report(
            commands::deps(&dependency_command(args.cmd, global.offline), global),
            global,
        ),
        Commands::Pack(o) => report(commands::pack(&o, global), global),
        Commands::Plugins => report(commands::plugins(global), global),
        Commands::Diff(o) => report(commands::diff(&o, global), global),
        Commands::Doc(o) => report(commands::doc(&o, project), global),
        Commands::Explain(o) => report(commands::explain(&o), global),
        Commands::Expand(o) => report(commands::expand(&o, project), global),
        Commands::Doctor(o) => report(commands::doctor(&o, global), global),
        Commands::Inspect(o) => report(commands::inspect(&o, project), global),
        Commands::Lsp(_) => lsp::serve(),
        Commands::External(words) => external(&words, global),
    }
}

/// A command a backend or a plugin of the project adds.
#[cfg(feature = "wasm-backends")]
fn external(words: &[OsString], global: &GlobalArguments) -> ExitCode {
    commands::contributed::run(words, global)
}

#[cfg(not(feature = "wasm-backends"))]
fn external(words: &[OsString], global: &GlobalArguments) -> ExitCode {
    let name = (words.first()).map(|w| w.to_string_lossy().into_owned());
    let diagnostic = Diagnostic::new(
        codes::P052,
        Severity::Error,
        format!(
            "flick has no command `{}`: this build adds no contributed commands",
            name.unwrap_or_default()
        ),
    );
    let outcome = Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![diagnostic],
    };
    report(outcome, global)
}
