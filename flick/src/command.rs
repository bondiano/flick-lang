//! A command produces diagnostics; `run_command` reports them on stderr and
//! turns them into the exit code.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use flick_core::diagnostics::Diagnostic;
use flick_core::source::SourceMap;

use crate::args::{DiagnosticsFormat, GlobalArguments};
use crate::report;

/// What a command leaves to report: the sources its diagnostics point into.
#[derive(Default)]
pub struct Outcome {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait Command {
    fn run(&self) -> Outcome;

    /// What a person reads after the diagnostics of `outcome`, which took
    /// `took`: how the command went, as a build says what it wrote.
    fn summary(&self, _outcome: &Outcome, _took: Duration, _color: bool) -> Option<String> {
        None
    }
}

pub fn run_command(command: &impl Command, global: &GlobalArguments) -> ExitCode {
    let started = Instant::now();
    let mut outcome = command.run();
    // the summary counts what the exit code counts
    if global.warnings_as_errors {
        outcome.diagnostics = report::promote(&outcome.diagnostics);
    }
    let summary = (matches!(global.diagnostics, DiagnosticsFormat::Human) && !global.quiet)
        .then(|| command.summary(&outcome, started.elapsed(), report::color(global)))
        .flatten();
    let code = finish(outcome, global);
    if let Some(summary) = summary {
        let _ = io::stderr().lock().write_all(summary.as_bytes());
    }
    code
}

/// Reports `outcome` on stderr; its exit code.
pub fn finish(outcome: Outcome, global: &GlobalArguments) -> ExitCode {
    let Outcome {
        sources,
        diagnostics,
    } = outcome;
    report::render(global, &sources, &diagnostics);
    if global.warnings_as_errors {
        return report::exit_code(&report::promote(&diagnostics));
    }
    report::exit_code(&diagnostics)
}
