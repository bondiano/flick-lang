//! In-memory `.fl` module overlay delegating all non-overlaid reads and host
//! hooks to an underlying [`FsHost`].

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_core::diagnostics::Diagnostic;
use flick_core::plugin::{Expander, reach::Outside};
use flick_core::project::config::BackendEnvironment;
use flick_core::project::target::Wanted;
use flick_core::project::{self, BackendTags, Foreign, Host, Project};

use crate::sources::FsHost;

/// Serves overlaid in-memory `.fl` modules under a source root and delegates
/// all other reads to the underlying [`FsHost`].
pub(crate) struct OverlayHost<'a> {
    base: &'a FsHost,
    overlays: BTreeMap<PathBuf, Vec<u8>>,
}

impl<'a> OverlayHost<'a> {
    pub(crate) fn new(
        base: &'a FsHost,
        root: &Path,
        modules: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let overlays = modules
            .into_iter()
            .map(|(name, source)| {
                let path = project::normalize(&root.join(project::module_path(&name)));
                (path, source.into_bytes())
            })
            .collect();
        Self { base, overlays }
    }

    pub(crate) fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.overlays.keys()
    }
}

impl Host for OverlayHost<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if let Some(bytes) = self.overlays.get(path) {
            return Ok(bytes.clone());
        }
        let normalized = project::normalize(path);
        if let Some(bytes) = self.overlays.get(&normalized) {
            return Ok(bytes.clone());
        }
        self.base.read(path)
    }

    fn display(&self, path: &Path) -> String {
        self.base.display(path)
    }

    fn plugin(&self, package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        self.base.plugin(package, component)
    }

    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.base.cached(key)
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        self.base.save(key, value);
    }

    fn outside(&self) -> Option<Arc<dyn Outside>> {
        self.base.outside()
    }

    fn foreign(&self, project: &Project) -> Foreign {
        self.base.foreign(project)
    }

    fn verify(&self, project: &Project, wanted: &[Wanted]) -> Vec<Diagnostic> {
        self.base.verify(project, wanted)
    }

    fn stages_by_class(&self, project: &Project) -> bool {
        self.base.stages_by_class(project)
    }

    fn handles_service(
        &self,
        project: &Project,
        environment: &BackendEnvironment,
        service: &str,
    ) -> Option<bool> {
        self.base.handles_service(project, environment, service)
    }

    fn backend_tags(&self, project: &Project) -> Vec<BackendTags> {
        self.base.backend_tags(project)
    }

    fn loads_backends(&self) -> bool {
        self.base.loads_backends()
    }
}
