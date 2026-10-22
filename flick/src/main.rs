use std::process::ExitCode;

use clap::Parser;

use crate::args::{CommandArguments, Commands};
use crate::command::run_command;
use crate::commands::{CheckCommand, FmtCommand};

mod args;
mod command;
mod commands;
mod report;
mod sources;

fn main() -> ExitCode {
    let args = CommandArguments::parse();
    match args.cmd {
        Commands::Check(options) => run_command(&CheckCommand::new(options), &args.global),
        Commands::Fmt(options) => run_command(&FmtCommand::new(options), &args.global),
    }
}
