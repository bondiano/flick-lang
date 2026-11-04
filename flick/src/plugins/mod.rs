//! The plugins a project's dependencies provide, as the command line and
//! the language server load them, and what they gave in earlier runs.
//! `FLICK_PLUGIN_TRACE=<file>` appends a line to the file for each call a
//! plugin answers, which the tests count.

mod cache;
#[cfg(feature = "wasm-plugins")]
mod wasm;

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_core::plugin::source::BlockSource;
use flick_core::plugin::{Expander, Failure};
use flick_plugin::{Arena, Block};

pub use cache::{cached, save};

/// The plugin of the dependency `package`, from the component it names.
pub fn plugin(package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
    let Some(component) = component else {
        return Err(format!("no dependency provides plugin \"{package}\""));
    };
    let loaded = load(package, component)?;
    Ok(match env::var_os("FLICK_PLUGIN_TRACE") {
        Some(file) => Arc::new(Traced {
            inner: loaded,
            file: PathBuf::from(file),
        }),
        None => loaded,
    })
}

#[cfg(feature = "wasm-plugins")]
fn load(package: &str, component: &Path) -> Result<Arc<dyn Expander>, String> {
    wasm::load(package, component)
}

#[cfg(not(feature = "wasm-plugins"))]
fn load(package: &str, component: &Path) -> Result<Arc<dyn Expander>, String> {
    let _ = component;
    Err(format!(
        "`{package}` is a wasm plugin, and this flick is built without wasm plugins; release builds include them"
    ))
}

/// A plugin whose calls are traced to `file`.
struct Traced {
    inner: Arc<dyn Expander>,
    file: PathBuf,
}

impl Traced {
    fn trace(&self, line: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

impl Expander for Traced {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn collect(&self, blocks: &[Block]) -> Result<Vec<u8>, String> {
        self.trace(&format!("collect {}", self.inner.id()));
        self.inner.collect(blocks)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.trace(&format!("expand {} {}", self.inner.id(), block.head));
        self.inner.expand(block, state, source)
    }
}
