//! The host an in-memory documentation example compiles through: the real
//! project's files, with one path answered by the example's text.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_core::plugin::{Expander, reach::Outside};
use flick_core::project::Host;

use crate::sources::FsHost;

/// Serves `text` at `path` and everything else from `host`, so an example is
/// a source root in the real package rather than a file written to it.
pub(super) struct ExampleHost<'a> {
    pub host: &'a FsHost,
    pub path: PathBuf,
    pub text: String,
}

impl<'a> ExampleHost<'a> {
    pub fn new(host: &'a FsHost, path: PathBuf, text: String) -> Self {
        ExampleHost { host, path, text }
    }
}

impl Host for ExampleHost<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == self.path {
            Ok(self.text.as_bytes().to_vec())
        } else {
            self.host.read(path)
        }
    }

    fn display(&self, path: &Path) -> String {
        self.host.display(path)
    }

    fn plugin(&self, package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        self.host.plugin(package, component)
    }

    fn outside(&self) -> Option<Arc<dyn Outside>> {
        self.host.outside()
    }
}
