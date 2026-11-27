//! A REPL session without a terminal or a backend. Each input, a phrase,
//! is checked and lowered by the ordinary project pipeline as a module of
//! its own, `Repl.P<n>`, that imports every earlier phrase: a phrase is an
//! ordinary module, so it adds no semantics of its own. A phrase exposes
//! from each earlier one only the names no later one defined again, so the
//! newest definition of a name wins, while what an earlier phrase checked
//! keeps the binding it saw. An expression is the value `it`. A phrase that
//! fails leaves the session as it was.

mod analysis;
mod commands;
mod render;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

pub use analysis::HandlerStyle;
use analysis::inspect_task;
pub use commands::{AcceptedBg, AcceptedHandle, AcceptedProvide};
use render::{Layout, Name, Scope, Space, imported, name_of, starts_declaration};

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::{Export, Namespace};
use crate::project::config::Config;
use crate::project::{
    Compilation, Host, ModuleCache, Project, TargetOverride, compile_for, compile_with,
};
use crate::resolve::{Import, declarations, module_syntax};
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
    /// Whether the phrase was an expression (bound to `it`), rather than declarations.
    pub expression: bool,
    /// Checked/inferred type of `it` when `expression` is true.
    pub value_type: Option<Type>,
    /// Unresolved user effects on `it`, if any.
    pub effects: Vec<String>,
    /// Whether `it` has an `Eff r e a` task type that should be auto-awaited.
    pub is_task: bool,
    /// Result type `a` of `Eff r e a` when `is_task` is true.
    pub ok_type: Option<Type>,
    /// Error row `e` of `Eff r e a` when `is_task` is true and `e` is non-empty.
    pub err_type: Option<Type>,
}

/// Snapshot enabling [`Session::rollback_last`] after a runtime abort/interrupt.
struct UndoState {
    path: PathBuf,
    prev_scope: Scope,
    prev_handled_len: usize,
    prev_provided_len: usize,
}

/// The phrases entered so far and what checking them keeps.
pub struct Session {
    project: Project,
    sources: SourceMap,
    cache: ModuleCache,
    target: Option<TargetOverride>,
    /// The text of each phrase module, by path.
    files: BTreeMap<PathBuf, String>,
    phrases: Vec<Phrase>,
    scope: Scope,
    handled_effects: Vec<String>,
    provided_services: Vec<String>,
    last_undo: Option<UndoState>,
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
            target: None,
            files: BTreeMap::new(),
            phrases: Vec::new(),
            scope: Scope::default(),
            handled_effects: Vec::new(),
            provided_services: Vec::new(),
            last_undo: None,
        }
    }

    /// Creates a session configured to typecheck phrases against `target`
    /// (for example `"vm"` for `vm/native`).
    pub fn with_target(target: impl Into<String>) -> Self {
        let target_name = target.into();
        let mut session = Self::new();
        let rt = (target_name != "vm").then_some(true).or(Some(false));
        session.target = Some(TargetOverride {
            target: Some(target_name),
            target_overridden: true,
            rt,
            skip_backend_check_target: true,
            ..TargetOverride::default()
        });
        session
    }

    /// The inputs and phrase modules, for rendering diagnostics.
    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    pub fn phrases(&self) -> &[Phrase] {
        &self.phrases
    }

    pub fn handled_effects(&self) -> &[String] {
        &self.handled_effects
    }

    pub fn provided_services(&self) -> &[String] {
        &self.provided_services
    }

    /// The type of the newest value `name`.
    pub fn value(&self, name: &str) -> Option<&Type> {
        let (phrase, _) = self.scope.names.get(&(Space::Value, name.to_owned()))?;
        (self.phrases[*phrase].names.iter())
            .find(|e| e.entity.name == name && Space::of(e.entity.namespace) == Space::Value)?
            .scheme
            .as_ref()
    }

    /// Reverts the most recently accepted phrase or session command, restoring
    /// `phrases`, `scope`, `files`, and installed effect/service records.
    pub fn rollback_last(&mut self) -> bool {
        let Some(undo) = self.last_undo.take() else {
            return false;
        };
        self.files.remove(&undo.path);
        self.phrases.pop();
        self.scope = undo.prev_scope;
        self.handled_effects.truncate(undo.prev_handled_len);
        self.provided_services.truncate(undo.prev_provided_len);
        true
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
        let previous_it = self
            .scope
            .names
            .get(&(Space::Value, "it".to_owned()))
            .map(|(_, module)| module.clone());
        let initial_layout = if expression {
            Layout::expression(&head, input, previous_it.as_deref())
        } else {
            Layout::declarations(&head, input)
        };
        let (mut layout, mut diagnostics, mut compilation) =
            self.check_and_sign(&path, initial_layout);
        // Effectful expressions (such as `Platform.stdout_line "hi"` or `Ask.ask ()`)
        // fail pure module initialization with E001; retry them wrapped in `task { ... }`.
        if expression && self.has_pure_init_error(&path, &diagnostics) {
            let task_layout = Layout::task_expression(&head, input, previous_it.as_deref());
            (layout, diagnostics, compilation) = self.check_and_sign(&path, task_layout);
        }
        let placed = self.place(diagnostics, &path, &layout, file);
        if placed.iter().any(|d| d.severity == Severity::Error) {
            self.files.remove(&path);
            return Err(placed);
        }
        let value_type = if expression {
            Self::binding_scheme(&compilation, &module, "it")
        } else {
            None
        };
        let task_info = value_type
            .as_ref()
            .and_then(|ty| inspect_task(ty, &self.handled_effects, &self.provided_services));
        let (is_task, ok_type, err_type, effects) = match task_info {
            Some(info) => (true, Some(info.ok_type), info.err_type, info.effects),
            None => (false, None, None, Vec::new()),
        };
        self.commit_phrase(&path, module, file, &compilation, &imports, brought);
        Ok(Accepted {
            phrase: self.phrases.last().expect("the phrase just entered"),
            compilation,
            diagnostics: placed,
            expression,
            value_type,
            effects,
            is_task,
            ok_type,
            err_type,
        })
    }

    fn commit_phrase(
        &mut self,
        path: &Path,
        module: String,
        input: FileId,
        compilation: &Compilation,
        imports: &[Import],
        brought: Vec<(Name, String, Option<Export>)>,
    ) {
        let mut names: Vec<Export> = (compilation.modules.iter())
            .find(|m| m.name == module)
            .map(|m| m.interface.exports.clone())
            .unwrap_or_default();
        self.save_undo(path);
        let index = self.phrases.len();
        for export in &names {
            (self.scope.names).insert(name_of(export), (index, module.clone()));
        }
        for import in imports {
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
            input,
            names,
        });
    }

    fn save_undo(&mut self, path: &Path) {
        self.last_undo = Some(UndoState {
            path: path.to_owned(),
            prev_scope: self.scope.clone(),
            prev_handled_len: self.handled_effects.len(),
            prev_provided_len: self.provided_services.len(),
        });
    }

    fn binding_scheme(compilation: &Compilation, module: &str, name: &str) -> Option<Type> {
        compilation
            .modules
            .iter()
            .find(|m| m.name == module)?
            .interface
            .exports
            .iter()
            .find(|e| e.entity.name == name && e.entity.namespace == Namespace::Value)
            .and_then(|e| e.scheme.clone())
    }

    fn input_error(&self, file: FileId, message: String) -> Diagnostic {
        let whole = Span::new(
            file,
            0,
            u32::try_from(self.sources.get(file).text.len()).unwrap_or(0),
        );
        Diagnostic::error(codes::T002, message, whole)
    }

    fn has_pure_init_error(&self, path: &Path, diagnostics: &[Diagnostic]) -> bool {
        let Some(module) = self.file_of(path) else {
            return false;
        };
        diagnostics
            .iter()
            .any(|d| d.code == codes::E001 && d.span.is_some_and(|s| s.file == module))
    }

    fn check_and_sign(
        &mut self,
        path: &Path,
        mut layout: Layout,
    ) -> (Layout, Vec<Diagnostic>, Compilation) {
        let (mut diagnostics, mut compilation) = self.check(path, &layout.text);
        // a function needs a signature: the one the checker inferred is written
        if let Some(signed) = self.signed(&layout, path, &diagnostics) {
            layout = signed;
            (diagnostics, compilation) = self.check(path, &layout.text);
        }
        (layout, diagnostics, compilation)
    }

    /// Checks and lowers the phrase module at `path` and what it imports.
    fn check(&mut self, path: &Path, text: &str) -> (Vec<Diagnostic>, Compilation) {
        self.files.insert(path.to_owned(), text.to_owned());
        let roots = [path.to_owned()];
        let host = Files(&self.files);
        let (diagnostics, compilation, _) = if let Some(target) = self.target.as_ref() {
            compile_for(
                &host,
                &mut self.sources,
                Some(&self.project),
                &roots,
                None,
                Some(target),
                Some(&mut self.cache),
            )
        } else {
            compile_with(
                &host,
                &mut self.sources,
                Some(&self.project),
                &roots,
                None,
                None,
                Some(&mut self.cache),
            )
        };
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
