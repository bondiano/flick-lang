use flick_core::source::SourceMap;

use crate::args::CheckArguments;
use crate::command::{Command, Outcome};
use crate::sources;

pub struct CheckCommand {
    options: CheckArguments,
}

impl CheckCommand {
    pub fn new(options: CheckArguments) -> Self {
        Self { options }
    }
}

impl Command for CheckCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let mut diagnostics = Vec::new();
        for path in &self.options.paths {
            match sources::load(&mut sources, path) {
                Ok(file) => diagnostics.extend(flick_core::check(&sources, file)),
                Err(error) => diagnostics.push(error),
            }
        }
        Outcome {
            sources,
            diagnostics,
        }
    }
}
