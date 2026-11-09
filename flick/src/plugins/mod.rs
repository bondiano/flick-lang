//! The plugins a project's dependencies provide, as the command line and
//! the language server load them, and what they gave in earlier runs.
//! `FLICK_PLUGIN_TRACE=<file>` appends a line to the file for each call a
//! plugin answers, which the tests count.

mod cache;
mod outside;
#[cfg(feature = "wasm-plugins")]
mod wasm;

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_core::plugin::source::BlockSource;
use flick_core::plugin::{Expander, Failure};
use flick_plugin::{
    Arena, Block, CallSite, Completion, DeriveTarget, Diagnostic, Fact, Highlight, Hover, Piece,
    Published,
};

pub use cache::{cached, save};
pub use outside::{outside, set_offline};

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

    fn collect(&self, blocks: &[Block], source: &mut BlockSource) -> Result<Vec<u8>, String> {
        self.trace(&format!("collect {}", self.inner.id()));
        self.inner.collect(blocks, source)
    }

    fn publish(&self, state: &[u8]) -> Result<Vec<Published>, String> {
        self.trace(&format!("publish {}", self.inner.id()));
        self.inner.publish(state)
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

    fn transform(&self, program: &[u8]) -> Result<Vec<u8>, Failure> {
        self.trace(&format!("transform {}", self.inner.id()));
        self.inner.transform(program)
    }

    fn emit(
        &self,
        artifacts: &[flick_plugin::Artifact],
    ) -> Result<Vec<flick_plugin::Artifact>, Failure> {
        self.trace(&format!("emit {}", self.inner.id()));
        self.inner.emit(artifacts)
    }

    fn load(&self, path: &str, bytes: &[u8]) -> Result<Arena, Failure> {
        self.trace(&format!("load {} {path}", self.inner.id()));
        self.inner.load(path, bytes)
    }

    fn complete(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        member: bool,
        source: &mut BlockSource,
    ) -> Result<Vec<Completion>, String> {
        self.inner.complete(block, state, offset, member, source)
    }

    fn hover(
        &self,
        block: &Block,
        state: &[u8],
        offset: u32,
        facts: &[Fact],
    ) -> Result<Option<Hover>, String> {
        self.inner.hover(block, state, offset, facts)
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Result<Vec<Highlight>, String> {
        self.inner.highlight(block, state)
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.trace(&format!(
            "derive {} {class} {}",
            self.inner.id(),
            target.name
        ));
        self.inner
            .derive(target, class, args, module, state, source)
    }

    fn format(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Option<Vec<Piece>>, String> {
        self.trace(&format!("format {} {}", self.inner.id(), block.head));
        self.inner.format(block, state, source)
    }

    fn check(
        &self,
        block: &Block,
        state: &[u8],
        facts: &[Fact],
    ) -> Result<Vec<Diagnostic>, String> {
        self.trace(&format!("check {} {}", self.inner.id(), block.head));
        self.inner.check(block, state, facts)
    }

    fn check_calls(&self, calls: &[CallSite], state: &[u8]) -> Result<Vec<Diagnostic>, String> {
        self.trace(&format!("check-calls {} {}", self.inner.id(), calls.len()));
        self.inner.check_calls(calls, state)
    }
}
