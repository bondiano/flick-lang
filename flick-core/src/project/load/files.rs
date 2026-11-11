//! Files of other languages: a plugin whose package says `load = ["ext"]`
//! turns each file of that extension into a module, which the project then
//! imports and checks as its own. The module's text is the file's; the
//! code the plugin gives stands after it, as a block's does, at spans of the
//! file, so an editor goes to the file.
//!
//! shortcut: the file must be UTF-8 text (else `FLICK-S001`); its code names
//! std and the Prelude by their full names but no other module of the
//! project, whose imports are fixed before plugins run; a `CallSite` there
//! counts the lines of the code, and `deriving` in it derives nothing. Lift
//! each when a plugin needs it: bytes, imports `load` returns, origins for
//! lines, derives of generated types.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_plugin::Place;

use super::Loader;
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::expansion::Expansion;
use crate::plugin::render::render;
use crate::plugin::{Expander, Failure, placed};
use crate::project::module_name;
use crate::resolve::{Header, Item, ItemKind, ModuleSyntax};
use crate::source::{FileId, Span};
use crate::syntax::{self, Parse};

/// The name a module at `path` under `dir` must declare: a `loaded` file's is
/// that of the `.fl` file at its place.
pub(super) fn expected_name(path: &Path, dir: &Path, loaded: bool) -> Option<String> {
    let fl = loaded.then(|| path.with_extension("fl"));
    let at = fl.as_deref().unwrap_or(path);
    module_name(at.strip_prefix(dir).unwrap_or(at))
}

/// The plugin of each package, or why it does not load.
type Plugins = HashMap<String, Result<Arc<dyn Expander>, String>>;

impl Loader<'_> {
    /// The package whose plugin loads the files of `path`'s extension.
    pub(super) fn loader_of(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        self.loaders()
            .into_iter()
            .find(|(e, _)| e == ext)
            .map(|(_, p)| p)
    }

    /// Each extension a plugin loads and its package, the first that names it.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    pub(super) fn loaders(&self) -> Vec<(String, String)> {
        #[cfg(feature = "native-project")]
        {
            let mut out: Vec<(String, String)> = Vec::new();
            for (ext, package) in self.packages.loaders() {
                if !out.iter().any(|(e, _)| e == ext) {
                    out.push((ext.to_owned(), package.to_owned()));
                }
            }
            out
        }
        #[cfg(not(feature = "native-project"))]
        Vec::new()
    }

    /// `FLICK-P045` on each extension the plugins of two packages load.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    pub(super) fn loader_conflicts(&self, at: Span) -> Vec<Diagnostic> {
        #[cfg(feature = "native-project")]
        {
            let all: Vec<(&str, &str)> = self.packages.loaders().collect();
            (self.loaders().iter())
                .filter_map(|(ext, first)| {
                    let other = all.iter().find(|(e, p)| e == ext && p != first)?.1;
                    Some(Diagnostic::error(
                        codes::P045,
                        format!("the plugins of `{first}` and `{other}` both load `.{ext}` files"),
                        at,
                    ))
                })
                .collect()
        }
        #[cfg(not(feature = "native-project"))]
        {
            let _ = at;
            Vec::new()
        }
    }

    /// The file a module `name` may be under `dir` other than its `.fl`.
    pub(super) fn loaded_paths(&self, dir: &Path, name: &str) -> Vec<PathBuf> {
        let path = dir.join(super::module_path(name));
        (self.loaders().into_iter())
            .map(|(ext, _)| path.with_extension(ext))
            .collect()
    }

    /// The parse of the file at `path`, or for a `loaded` one no Flick and the
    /// header of the module `name`.
    pub(super) fn syntax_of(
        &mut self,
        path: &Path,
        file: FileId,
        loaded: bool,
        name: Option<&str>,
    ) -> (Arc<Parse>, Arc<ModuleSyntax>) {
        if loaded {
            Self::unparsed(file, name.unwrap_or(""))
        } else {
            self.parsed(path, file)
        }
    }

    /// A loaded file before its plugin gives its code: no Flick of its own,
    /// and a header that exposes all the code will declare.
    fn unparsed(file: FileId, name: &str) -> (Arc<Parse>, Arc<ModuleSyntax>) {
        let at = Span::new(file, 0, 0);
        let all = Item {
            kind: ItemKind::All,
            name: String::new(),
            span: at,
        };
        let header = Header {
            result: None,
            body: None,
            materialized_body: None,
            parameter_groups: Vec::new(),
            name: name.to_owned(),
            private: false,
            span: at,
            exposing: Some(vec![all]),
            parameters: Vec::new(),
        };
        let syntax = ModuleSyntax {
            header: Some(header),
            imports: Vec::new(),
            inline_imports: Vec::new(),
            applications: Vec::new(),
            bodiless: Vec::new(),
        };
        (Arc::new(syntax::parse("", file)), Arc::new(syntax))
    }

    /// The code of each module a plugin loads, from its file.
    pub(super) fn load_files(&mut self, plugins: &mut Plugins, diagnostics: &mut Vec<Diagnostic>) {
        if let Some(p) = self.project {
            diagnostics.extend(self.loader_conflicts(Span::new(p.config_file, 0, 0)));
        }
        for i in 0..self.modules.len() {
            let Some(package) = self.modules[i].loader.clone() else {
                continue;
            };
            let m = &self.modules[i];
            let text = self.sources.get(m.file).text.clone();
            let len = u32::try_from(text.len()).expect("source fits in u32");
            let whole = Span::new(m.file, 0, len);
            let path = self.host.display(&m.path);
            let plugin = match self.load_plugin(plugins, &package) {
                Ok(plugin) => plugin,
                Err(why) => {
                    let message =
                        format!("the plugin of package `{package}` cannot be loaded: {why}");
                    diagnostics.push(Diagnostic::error(codes::P035, message, whole));
                    continue;
                }
            };
            let failed = |why: String| {
                Diagnostic::error(codes::P036, format!("plugin `{package}` {why}"), whole)
            };
            let arena = match plugin.load(&path, text.as_bytes()) {
                Ok(arena) => arena,
                Err(Failure::Refused(found)) => {
                    diagnostics.extend(
                        found
                            .iter()
                            .map(|d| placed(d, &package, whole, whole, false)),
                    );
                    continue;
                }
                Err(Failure::Broken(why)) => {
                    diagnostics.push(failed(format!("failed to load this file: {why}")));
                    continue;
                }
            };
            let rendered = match render(&arena, len, Place::Declaration, &[]) {
                Ok(rendered) if rendered.holes.is_empty() => rendered,
                Ok(_) => {
                    diagnostics.push(failed(
                        "gave code with holes for a file, which has no user's code".into(),
                    ));
                    continue;
                }
                Err(why) => {
                    diagnostics.push(failed(format!("gave code that is not valid: {why}")));
                    continue;
                }
            };
            match Expansion::whole(&text, m.file, &rendered, &package) {
                Ok(expansion) => self.modules[i].expansion = Some(Arc::new(expansion)),
                Err(why) => {
                    diagnostics.push(failed(format!("gave code that cannot be placed: {why}")));
                }
            }
        }
    }
}
