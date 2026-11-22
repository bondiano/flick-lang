use crate::artifact::{Artifact, validate_destination_paths};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use tracing::debug;

use super::{BuildCommand, BuildPlan, Emission, publish_artifacts};
use crate::cache;
use crate::progress::Progress;

impl BuildCommand {
    /// The artifacts of an earlier build with the same key, written again.
    pub(super) fn cached(
        &self,
        project: &Project,
        output: &BuildPlan,
        progress: &Progress,
        key: &cache::Signed,
    ) -> Option<Vec<Diagnostic>> {
        progress.stage("Checking build cache");
        let Some(artifacts) = cache::load(&project.root, key) else {
            debug!(miss = %cache::missed(&project.root, key));
            return None;
        };
        if !output.backend.accepts_cached(&artifacts) {
            return None;
        }
        let paths = super::publication::paths(&artifacts);
        let current = validate_destination_paths(&output.out_dir, paths.iter().map(String::as_str))
            .is_ok()
            && cache::artifacts_current(&output.out_dir, &artifacts);
        progress.stage("Restoring cached artifacts");
        self.record(output, None, &output.entries, &artifacts);
        if current && !self.generation {
            if !cache::outputs_current(&project.root, &output.out_dir, &paths) {
                progress.stage("Recording build outputs");
                if let Err(err) = cache::record(&project.root, &output.out_dir, paths) {
                    return Some(vec![Diagnostic::new(
                        codes::P013,
                        Severity::Error,
                        format!(
                            "cannot record artifacts in `{}`: {err}",
                            output.out_dir.display()
                        ),
                    )]);
                }
            }
            return Some(Vec::new());
        }
        let mut emitted = Emission {
            artifacts,
            diagnostics: Vec::new(),
        };
        self.publish(Some(project), output, &mut emitted, None, progress);
        Some(emitted.diagnostics)
    }

    /// Cache hits and fresh builds have the same publication and ownership policy.
    pub(super) fn publish(
        &self,
        project: Option<&Project>,
        output: &BuildPlan,
        emitted: &mut Emission,
        extra: Option<Artifact>,
        progress: &Progress,
    ) {
        // a run writes what it runs, which no plugin's `emit` changes
        if !self.generation
            && let Some(project) = project
        {
            super::hooks::emit(project, emitted);
        }
        emitted.artifacts.extend(extra);
        let root = project
            .filter(|_| !self.generation)
            .map(|project| project.root.as_path());
        publish_artifacts(output, emitted, progress, root);
    }
}
