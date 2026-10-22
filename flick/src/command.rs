//! A command produces diagnostics; `run_command` reports them on stderr and
//! turns them into the exit code.

use std::process::ExitCode;

use flick_core::diagnostics::Diagnostic;
use flick_core::source::SourceMap;

use crate::args::GlobalArguments;
use crate::report;

/// What a command leaves to report: the sources its diagnostics point into.
#[derive(Default)]
pub struct Outcome {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait Command {
    fn run(&self) -> Outcome;
}

pub fn run_command(command: &impl Command, global: &GlobalArguments) -> ExitCode {
    let Outcome {
        sources,
        diagnostics,
    } = command.run();
    report::render(global, &sources, &diagnostics);
    report::exit_code(&diagnostics)
}
