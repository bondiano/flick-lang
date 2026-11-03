use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::Parser;
use flick_core::project::pnpm;

use crate::args::{CommandArguments, Commands, FfiCommands, GlobalArguments, SchemaCommands};
use crate::command::{Command, finish as report, run_command};
use crate::commands::{
    BuildCommand, CheckCommand, FfiCheckCommand, FfiImportCommand, FmtCommand, ScaffoldCommand,
};

mod args;
mod builtins;
mod cache;
mod child;
mod command;
mod commands;
mod fetch;
mod lsp;
mod mapper;
mod progress;
mod report;
mod sources;
mod toolchain;
mod workspace;

/// Runs `command` again after every change of the project's files.
fn watch(command: &impl Command, global: &GlobalArguments) -> ExitCode {
    let paths = commands::watched(global);
    loop {
        let before = commands::watch_snapshot(&paths);
        run_command(command, global);
        while commands::watch_snapshot(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        report::log(global, "a file changed: running again");
    }
}

/// The pnpm command of a dependency command.
fn dependency_command(cmd: Commands) -> pnpm::Command {
    match cmd {
        Commands::Install(o) => pnpm::Command::Install {
            locked: o.locked,
            offline: o.offline,
        },
        Commands::Add(o) => pnpm::Command::Add {
            packages: o.packages,
            dev: o.dev,
        },
        Commands::Remove(o) => pnpm::Command::Remove {
            packages: o.packages,
        },
        Commands::Update(o) => pnpm::Command::Update {
            packages: o.packages,
        },
        Commands::Deps(o) => match o.why {
            Some(package) => pnpm::Command::Why { package },
            None => pnpm::Command::List,
        },
        _ => unreachable!("only dependency commands"),
    }
}

fn main() -> ExitCode {
    let args = CommandArguments::parse();
    if !matches!(
        args.cmd,
        Commands::Fmt(_)
            | Commands::New(_)
            | Commands::Init(_)
            | Commands::Clean(_)
            | Commands::Build(_)
            | Commands::Test(_)
            | Commands::Install(_)
            | Commands::Add(_)
            | Commands::Remove(_)
            | Commands::Update(_)
            | Commands::Deps(_)
            | Commands::Explain(_)
            | Commands::Doctor
            // after its logging starts
            | Commands::Lsp(_)
    ) {
        builtins::prepare();
    }
    let members = match workspace::members(&args) {
        Ok(Some(members)) => members,
        Ok(None) => return dispatch(args),
        Err(outcome) => return report(outcome, &args.global),
    };
    let several = members.len() > 1;
    let mut code = ExitCode::SUCCESS;
    for member in members {
        let mut args = CommandArguments::parse();
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
        Commands::Ffi(FfiCommands::Import(options)) => run_command(
            &FfiImportCommand::new(options, global.project.clone()),
            global,
        ),
        Commands::Ffi(FfiCommands::Check(options)) => run_command(
            &FfiCheckCommand::new(options, global.project.clone()),
            global,
        ),
        Commands::New(options) => run_command(
            &ScaffoldCommand {
                dir: options.directory.into(),
                template: options.template,
                fresh: true,
            },
            global,
        ),
        Commands::Init(options) => run_command(
            &ScaffoldCommand {
                dir: ".".into(),
                template: options.template,
                fresh: false,
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
            commands::deps(&dependency_command(args.cmd), global),
            global,
        ),
        Commands::Pack(o) => report(commands::pack(&o, global), global),
        Commands::Diff(o) => report(commands::diff(&o, global), global),
        Commands::Schema(SchemaCommands::Export(o)) => {
            report(commands::schema_export(&o, project), global)
        }
        Commands::Doc(o) => report(commands::doc(&o, project), global),
        Commands::Explain(o) => report(commands::explain(&o), global),
        Commands::Doctor => report(commands::doctor(global), global),
        Commands::Inspect(o) => report(commands::inspect(&o, project), global),
        Commands::Lsp(_) => lsp::serve(),
        Commands::ContentMapper(o) => match o.declarations {
            Some(paths) => mapper::declarations(&paths),
            None => mapper::serve(),
        },
    }
}
