use std::sync::Once;

use super::BuildCommand;
use crate::builtins;
use crate::progress::{Mode, Progress};

impl BuildCommand {
    pub(super) fn progress(&self) -> Progress {
        let progress = Progress::new(if self.options.inputs {
            Mode::Hidden
        } else {
            self.display.unwrap_or(Mode::Hidden)
        });
        if self.display.is_some() {
            static PREPARED: Once = Once::new();
            PREPARED.call_once(|| {
                progress.stage("Preparing compiler");
                builtins::prepare();
            });
        }
        progress
    }
}
