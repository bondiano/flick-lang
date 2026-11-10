use std::sync::Once;

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

    /// The built-in modules checked, or the outcome of why they cannot be;
    /// the phase shows once a process, as they are read once. `--inputs`
    /// only names files and needs none.
    pub(super) fn prepare(&self, progress: &Progress) -> Result<(), Outcome> {
        static SHOWN: Once = Once::new();
        if self.options.inputs {
            return Ok(());
        }
        SHOWN.call_once(|| progress.stage("Preparing compiler"));
        builtins::prepare().map_err(|diagnostic| Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        })
    }
}
