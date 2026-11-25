//! A REPL session without a terminal or a backend. Each input, a phrase,
//! is checked and lowered by the ordinary project pipeline as a module of
//! its own, `Repl.P<n>`, that imports every earlier phrase: a phrase is an
//! ordinary module, so it adds no semantics of its own. A phrase exposes
//! from each earlier one only the names no later one defined again, so the
//! newest definition of a name wins, while what an earlier phrase checked
//! keeps the binding it saw. An expression is the value `it`. A phrase that
//! fails leaves the session as it was.

mod render;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use render::{Layout, Name, Scope, Space, imported, name_of, starts_declaration};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::Export;
use crate::project::config::Config;
use crate::project::{Compilation, Host, ModuleCache, Project, compile_with};
use crate::resolve::{declarations, module_syntax};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax::parse;
use crate::types::{Type, builtins};

/// One accepted input.
#[derive(Debug, Clone)]
pub struct Phrase {
    /// Its module, `Repl.P<n>`.
    pub module: String,
    /// The input as it was entered, a file of [`Session::sources`].
    pub input: FileId,
    /// The names it brought in, with their types: what it defines, or what
    /// its imports expose.
    pub names: Vec<Export>,
}

/// What entering a phrase gave.
pub struct Accepted<'s> {
    pub phrase: &'s Phrase,
    /// The phrase module and every module it imports, lowered.
    pub compilation: Compilation,
    /// Its warnings, placed in the input.
    pub diagnostics: Vec<Diagnostic>,
}

/// The phrases entered so far and what checking them keeps.
pub struct Session {
    project: Project,
    sources: SourceMap,
    cache: ModuleCache,
    /// The text of each phrase module, by path.
    files: BTreeMap<PathBuf, String>,
    phrases: Vec<Phrase>,
    scope: Scope,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// The phrase modules, the only files of a session.
struct Files<'a>(&'a BTreeMap<PathBuf, String>);

impl Host for Files<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        (self.0.get(path))
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix(ROOT)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

const ROOT: &str = "/repl";

impl Session {
    pub fn new() -> Self {
        let mut sources = SourceMap::default();
        let config_file = sources.add(format!("{ROOT}/flick.toml"), "");
        let project = Project {
            root: PathBuf::from(ROOT),
            config: Config {
                source_dir: ".".to_owned(),
                ..Config::default()
            },
            config_file,
            workspace: None,
        };
        Session {
            project,
            sources,
            cache: ModuleCache::default(),
            files: BTreeMap::new(),
            phrases: Vec::new(),
            scope: Scope::default(),
        }
    }

    /// The inputs and phrase modules, for rendering diagnostics.
    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    pub fn phrases(&self) -> &[Phrase] {
        &self.phrases
    }

    /// The type of the newest value `name`.
    pub fn value(&self, name: &str) -> Option<&Type> {
        let (phrase, _) = self.scope.names.get(&(Space::Value, name.to_owned()))?;
        (self.phrases[*phrase].names.iter())
            .find(|e| e.entity.name == name && Space::of(e.entity.namespace) == Space::Value)?
            .scheme
            .as_ref()
    }

    /// Checks and lowers `input` against the earlier phrases. On success it
    /// is the newest phrase; its errors are placed in `input`.
    pub fn enter(&mut self, input: &str) -> Result<Accepted<'_>, Vec<Diagnostic>> {
        let n = self.phrases.len() + 1;
        let module = format!("Repl.P{n}");
        let path = PathBuf::from(format!("{ROOT}/Repl/P{n}.fl"));
        let file = self.sources.add(format!("repl:{n}"), input);
        let parsed = parse(input, file);
        let expression = !parsed.diagnostics.is_empty() && !starts_declaration(input, file);
        let imports = if expression {
            Vec::new()
        } else {
            module_syntax(&parsed.tree, input, file).imports
        };
        // what the phrase defines or imports, which no earlier phrase gives it
        let mut own: BTreeSet<Name> = BTreeSet::new();
        let mut brought: Vec<(Name, String, Option<Export>)> = Vec::new();
        if expression {
            own.insert((Space::Value, "it".to_owned()));
        } else {
            let defined = declarations(&parsed.tree, input, file).into_iter();
            own.extend(defined.map(|d| (Space::of(d.namespace), d.name)));
            for import in &imports {
                let exports = (builtins().iter())
                    .find(|i| i.module == import.module)
                    .map_or(&[][..], |i| &i.exports[..]);
                for (name, export) in imported(import, exports) {
                    own.insert(name.clone());
                    brought.push((name, import.module.clone(), export.cloned()));
                }
            }
        }
        let earlier: Vec<String> = self.phrases.iter().map(|p| p.module.clone()).collect();
        let again: Vec<String> = imports.iter().map(|i| i.module.clone()).collect();
        let head = format!(
            "module {module} exposing (..)\n{}",
            self.scope.imports(&earlier, &own, &again)
        );
        let mut layout = if expression {
            let previous = self.scope.names.get(&(Space::Value, "it".to_owned()));
            Layout::expression(&head, input, previous.map(|(_, module)| module.as_str()))
        } else {
            Layout::declarations(&head, input)
        };
        let (mut diagnostics, mut compilation) = self.check(&path, &layout.text);
        // a function needs a signature: the one the checker inferred is written
        if let Some(signed) = self.signed(&layout, &path, &diagnostics) {
            layout = signed;
            (diagnostics, compilation) = self.check(&path, &layout.text);
        }
        let placed = self.place(diagnostics, &path, &layout, file);
        if placed.iter().any(|d| d.severity == Severity::Error) {
            self.files.remove(&path);
            return Err(placed);
        }
        let mut names: Vec<Export> = (compilation.modules.iter())
            .find(|m| m.name == module)
            .map(|m| m.interface.exports.clone())
            .unwrap_or_default();
        let index = self.phrases.len();
        for export in &names {
            (self.scope.names).insert(name_of(export), (index, module.clone()));
        }
        for import in &imports {
            self.scope
                .modules
                .insert(import.module.clone(), import.alias.clone());
        }
        for (name, from, export) in brought {
            names.extend(export);
            self.scope.names.insert(name, (index, from));
        }
        self.phrases.push(Phrase {
            module,
            input: file,
            names,
        });
        Ok(Accepted {
            phrase: self.phrases.last().expect("the phrase just entered"),
            compilation,
            diagnostics: placed,
        })
    }

    /// Checks and lowers the phrase module at `path` and what it imports.
    fn check(&mut self, path: &Path, text: &str) -> (Vec<Diagnostic>, Compilation) {
        self.files.insert(path.to_owned(), text.to_owned());
        let roots = [path.to_owned()];
        let host = Files(&self.files);
        let (diagnostics, compilation, _) = compile_with(
            &host,
            &mut self.sources,
            Some(&self.project),
            &roots,
            None,
            None,
            Some(&mut self.cache),
        );
        (diagnostics, compilation)
    }

    /// The file the last check read for the phrase module at `path`.
    fn file_of(&self, path: &Path) -> Option<FileId> {
        let shown = Files(&self.files).display(path);
        let index = (self.sources.files().enumerate())
            .filter(|(_, f)| f.path == shown)
            .last()?
            .0;
        Some(FileId::new(u32::try_from(index).ok()?))
    }

    /// `layout` with the signature each function without one was inferred,
    /// when every such function has one.
    fn signed(&self, layout: &Layout, path: &Path, diagnostics: &[Diagnostic]) -> Option<Layout> {
        let module = self.file_of(path)?;
        let unsigned: Vec<&Diagnostic> = (diagnostics.iter())
            .filter(|d| d.code == codes::T004 && d.span.is_some_and(|s| s.file == module))
            .collect();
        if unsigned.is_empty() {
            return None;
        }
        let mut inserts = Vec::new();
        for diagnostic in unsigned {
            // the fix is not safe in a file, where a signature can pin a type
            // other code relies on being inferred; a phrase is checked once,
            // with the very type the checker inferred for it
            let fix = diagnostic.fixes.iter().find(|f| !f.edits.is_empty())?;
            for edit in &fix.edits {
                if edit.span.start != edit.span.end || edit.span.file != module {
                    return None;
                }
                inserts.push((edit.span.start, edit.replacement.clone()));
            }
        }
        Some(layout.inserted(&inserts))
    }

    /// `diagnostics` of the phrase module placed in `input`: one in a
    /// generated part is dropped, or, if an error, put on the whole input.
    /// Those of earlier phrases, which a check reports again, are dropped.
    fn place(
        &self,
        diagnostics: Vec<Diagnostic>,
        path: &Path,
        layout: &Layout,
        input: FileId,
    ) -> Vec<Diagnostic> {
        let Some(module) = self.file_of(path) else {
            return diagnostics;
        };
        let whole = Span::new(
            input,
            0,
            u32::try_from(self.sources.get(input).text.len()).unwrap_or(0),
        );
        let mut placed = Vec::new();
        for mut d in diagnostics {
            match d.span {
                Some(span) if span.file == module => match layout.place(span, input) {
                    Some(span) => d.span = Some(span),
                    None if d.severity == Severity::Error => d.span = Some(whole),
                    None => continue,
                },
                Some(_) => continue,
                None => {}
            }
            d.related.retain_mut(|r| {
                if r.span.file != module {
                    return true;
                }
                layout.place(r.span, input).map(|s| r.span = s).is_some()
            });
            d.fixes.retain_mut(|f| {
                f.edits.iter_mut().all(|e| {
                    e.span.file != module
                        || layout.place(e.span, input).map(|s| e.span = s).is_some()
                })
            });
            placed.push(d);
        }
        placed
    }
}
