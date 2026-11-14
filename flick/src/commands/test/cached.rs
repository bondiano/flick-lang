//! Restore test artifacts under the selected backend.

use super::super::build::{BuildPlan, Emission, Purpose, publication_paths, publish_artifacts};
use super::Runner;
use crate::progress::Progress;
use crate::{backends::Selected, cache};
use flick_core::build::{Extras, go};
use flick_core::project::Project;
use std::path::Path;
use tracing::debug;

pub(super) fn cached_prepare(
    project: &Project,
    key: &cache::Signed,
    out: &Path,
    progress: &Progress,
    backend: &Selected,
) -> Option<Runner> {
    let Some(artifacts) = cache::load(&project.root, key) else {
        debug!(miss = %cache::missed(&project.root, key));
        return None;
    };
    if !backend.accepts_cached(&artifacts) {
        return None;
    }
    let go = !backend.is_component() && artifacts.iter().any(|a| a.path == "go/test/go.mod");
    let runner = out.join(if go {
        go::executable(go::TEST)
    } else {
        "test.js".to_owned()
    });
    let current = cache::artifacts_current(out, &artifacts)
        && (!go || cache::go_current(&project.root, key, out, &go::executable(go::TEST)));
    let output = BuildPlan {
        purpose: Purpose::Test,
        backend: backend.clone(),
        out_dir: out.to_path_buf(),
        entries: Vec::new(),
        typescript: false,
        go,
        suites: if go {
            vec!["cached".to_owned()]
        } else {
            Vec::new()
        },
        extras: Extras::default(),
        release: false,
    };
    if current {
        cache::record(&project.root, out, publication_paths(&output, &artifacts)).ok()?;
        progress.stage("Reusing test build");
        return Some(if backend.is_component() {
            Runner::component(out.to_path_buf(), project)
        } else {
            Runner::native(runner)
        });
    }
    progress.stage("Restoring cached test artifacts");
    let mut emitted = Emission {
        artifacts,
        diagnostics: Vec::new(),
    };
    publish_artifacts(&output, &mut emitted, progress, Some(&project.root));
    if emitted.diagnostics.is_empty() {
        if go {
            cache::mark_go(&project.root, key, out, &go::executable(go::TEST));
        }
        Some(if backend.is_component() {
            Runner::component(out.to_path_buf(), project)
        } else {
            Runner::native(runner)
        })
    } else {
        None
    }
}
