//! Library foreign files retain the existing shared shipping policy. Their
//! bytes are also declared inputs, so a guest has no ambient file access.

use flick_backend::InputKind;
use flick_core::project::Compilation;

use super::Prepared;
use crate::artifact::Artifact;
use crate::commands::build::shipped;

impl Prepared {
    pub(super) fn foreign_assets(&mut self, compilation: &Compilation) -> Vec<String> {
        let (files, diagnostics) = shipped::foreign_files(compilation, &|_| true);
        self.diagnostics.extend(diagnostics);
        let mut inputs = Vec::new();
        for file in files {
            inputs.push(self.input(
                format!("foreign/{}", file.path),
                InputKind::Foreign,
                file.contents.as_bytes().to_vec(),
            ));
            self.shared.push(Artifact::from(file));
        }
        inputs
    }
}
