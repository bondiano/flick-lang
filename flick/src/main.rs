use std::process::ExitCode;

use clap::Parser;

use crate::args::{CommandArguments, Commands, FfiCommands};
use crate::command::run_command;
use crate::commands::{BuildCommand, CheckCommand, FfiCheckCommand, FfiImportCommand, FmtCommand};

mod args;
mod command;
mod commands;
mod report;
mod sources;

fn main() -> ExitCode {
    let args = CommandArguments::parse();
    match args.cmd {
        Commands::Check(options) => run_command(
            &CheckCommand::new(options, args.global.project.clone()),
            &args.global,
        ),
        Commands::Build(options) => run_command(
            &BuildCommand::new(options, args.global.project.clone()),
            &args.global,
        ),
        Commands::Fmt(options) => run_command(&FmtCommand::new(options), &args.global),
        Commands::Test(options) => commands::test(&options, &args.global),
        Commands::Ffi(FfiCommands::Import(options)) => run_command(
            &FfiImportCommand::new(options, args.global.project.clone()),
            &args.global,
        ),
        Commands::Ffi(FfiCommands::Check(options)) => run_command(
            &FfiCheckCommand::new(options, args.global.project.clone()),
            &args.global,
        ),
    }
}
