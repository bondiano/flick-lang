//! The analysis editors work from: every module that parses, with its tree,
//! what checking recorded and its interface, from the same run that lowers
//! the modules for code generation.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

use super::{Compilation, Loaded, Mode, ModuleCache, Root, run};
use crate::diagnostics::Diagnostic;
use crate::iface::{Interface, fingerprint};
use crate::ir;
use crate::plugin::EditorBlock;
use crate::plugin::expansion::Expansion;
use crate::project::{Host, Project};
use crate::source::{FileId, SourceMap};
use crate::syntax::cst::Node;
use crate::types::Typing;

/// A module as editors see it.
pub struct Analyzed {
    pub name: String,
    pub path: PathBuf,
    pub file: FileId,
    pub test: bool,
    pub tree: Node,
    pub typing: Arc<Typing>,
    pub interface: Interface,
    /// Its plugin blocks, whose plugins editors ask.
    pub editing: Vec<EditorBlock>,
    /// The module with its plugin blocks expanded, when it has any.
    pub expansion: Option<Arc<Expansion>>,
}

impl Analyzed {
    pub(super) fn new(loaded: &Loaded, typing: Arc<Typing>, interface: Interface) -> Self {
        Analyzed {
            name: loaded.name.clone(),
            path: loaded.path.clone(),
            file: loaded.file,
            test: loaded.root == Root::Test,
            // the user's code inside plugin blocks parsed in place
            tree: loaded
                .expansion
                .as_ref()
                .map_or_else(|| loaded.parse.tree.clone(), |e| e.source_tree.clone()),
            typing,
            interface,
            editing: loaded.editing.clone(),
            expansion: loaded.expansion.clone(),
        }
    }
}

/// One analysis of a project: its modules and, when nothing has errors,
/// their IR for code generation.
pub struct Analysis {
    pub modules: Vec<Analyzed>,
    pub compilation: Compilation,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn module(&self, name: &str) -> Option<&Analyzed> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn by_file(&self, file: FileId) -> Option<&Analyzed> {
        self.modules.iter().find(|m| m.file == file)
    }
}

/// Analyzes `roots` and their imports.
pub fn analyze(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Analysis {
    let output = run(host, sources, project, roots, Mode::Analyze, None, None);
    Analysis {
        modules: output.analyzed,
        compilation: output.compilation,
        diagnostics: output.diagnostics,
    }
}

/// [`analyze`], reusing and updating what `cache` holds of earlier runs; for
/// editors, so its compilation holds no modules.
pub fn analyze_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    cache: &mut ModuleCache,
) -> Analysis {
    let output = run(
        host,
        sources,
        project,
        roots,
        Mode::Analyze,
        Some(cache),
        None,
    );
    Analysis {
        modules: output.analyzed,
        compilation: output.compilation,
        diagnostics: output.diagnostics,
    }
}

/// A fingerprint of the modules a compilation holds: their interfaces and
/// IR. An executable build and an editor projection made from the same
/// analysis have the same one.
pub fn analysis_fingerprint(compilation: &Compilation) -> String {
    let mut parts: BTreeMap<&str, String> = BTreeMap::new();
    for m in &compilation.modules {
        parts.insert(
            &m.name,
            format!("{} {}", fingerprint(&m.interface), ir::print(&m.ir)),
        );
    }
    let text = parts.into_iter().fold(String::new(), |mut acc, (n, p)| {
        let _ = writeln!(acc, "{n}\n{p}");
        acc
    });
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}
