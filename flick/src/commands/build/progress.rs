use std::sync::OnceLock;

use flick_core::diagnostics::Diagnostic;
use flick_core::source::SourceMap;

use super::BuildCommand;
use crate::builtins;
use crate::command::Outcome;
use crate::progress::{Mode, Progress};

impl BuildCommand {
    pub(super) fn progress(&self) -> Progress {
        Progress::new(if self.options.inputs {
            Mode::Hidden
        } else {
            self.display.unwrap_or(Mode::Hidden)
        })
    }

    /// The built-in modules checked, once a run, or the outcome of why they
    /// cannot be. `--inputs` only names files and needs none.
    pub(super) fn prepare(&self, progress: &Progress) -> Result<(), Outcome> {
        static PREPARED: OnceLock<Result<(), Diagnostic>> = OnceLock::new();
        if self.options.inputs {
            return Ok(());
        }
        let prepared = PREPARED.get_or_init(|| {
            progress.stage("Preparing compiler");
            builtins::prepare()
        });
        prepared.clone().map_err(|diagnostic| Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        })
    }
}
