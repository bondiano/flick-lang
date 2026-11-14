use crate::artifact::{Artifact, validate_destination_paths};
use flick_core::build;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use tracing::debug;

use super::{BuildCommand, BuildPlan, Emission, write_artifacts};
use crate::cache;
use crate::progress::Progress;
use crate::sources::FsHost;

impl BuildCommand {
    /// The artifacts of an earlier build with the same key, written again.
    pub(super) fn cached(
        &self,
        project: &Project,
        host: &FsHost,
        progress: &Progress,
        key: &cache::Signed,
    ) -> Option<Vec<Diagnostic>> {
        let output = self.plan(host, Some(project), self.entries(project));
        if output.extras.schema_aot {
            return None;
        }
        progress.stage("Checking build cache");
        let Some(artifacts) = cache::load(&project.root, key) else {
            debug!(miss = %cache::missed(&project.root, key));
            return None;
        };
        let paths = super::publication::paths(&output, &artifacts);
        let current = validate_destination_paths(&output.out_dir, paths.iter().map(String::as_str))
            .is_ok()
            && cache::artifacts_current(&output.out_dir, &artifacts)
            && (!output.go
                || output.entries.iter().all(|(name, _)| {
                    cache::go_current(
                        &project.root,
                        key,
                        &output.out_dir,
                        &build::go::executable(name),
                    )
                }));
        progress.stage("Restoring cached artifacts");
        self.record(&output, None, &output.entries);
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
        self.publish(Some(project), &output, &mut emitted, None, progress);
        if output.go && emitted.diagnostics.is_empty() {
            for (name, _) in &output.entries {
                cache::mark_go(
                    &project.root,
                    key,
                    &output.out_dir,
                    &build::go::executable(name),
                );
            }
        }
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
        let paths = super::publication::paths(output, &emitted.artifacts);
        if !self.generation
            && let Some(project) = project
            && let Err(error) = cache::validate_record(&project.root, &output.out_dir, &paths)
        {
            emitted.diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot validate earlier build outputs: {error}"),
            ));
            return;
        }
        write_artifacts(output, emitted, progress);
        if self.generation
            || emitted
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
        {
            return;
        }
        let Some(project) = project else {
            return;
        };
        progress.stage("Recording build outputs");
        if let Err(err) = cache::record(&project.root, &output.out_dir, paths) {
            emitted.diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!(
                    "cannot record artifacts in `{}`: {err}",
                    output.out_dir.display()
                ),
            ));
        }
    }
}
