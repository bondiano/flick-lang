//! `flick new` and `flick init`: backend-neutral project scaffolding driven by
//! templates contributed through the `flick:command` interface by official
//! backends (`--target`) or third-party packages (`--from`).
//!
//! The core writes no template files of its own: contributors return pure file
//! plans (including `flick.toml`), which the host validates and publishes
//! atomically. `init` never overwrites existing files: every collision is
//! reported (`FLICK-P019`) before anything is written, except `AGENTS.md`,
//! which a repository keeps as it is, and `flick.lock`, into which new pins
//! are merged.

#[cfg(feature = "wasm-backends")]
mod source;

#[cfg(feature = "wasm-backends")]
use std::env;
#[cfg(feature = "wasm-backends")]
use std::fs;
#[cfg(feature = "wasm-backends")]
use std::io::{self, Write as _};
#[cfg(feature = "wasm-backends")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "wasm-backends")]
use flick_backend::ArtifactKind;
#[cfg(feature = "wasm-backends")]
use flick_backend::command::{Plan, TemplateDescriptor, TemplateRequest};
#[cfg(feature = "wasm-backends")]
use flick_core::diagnostics::{Code, suggest};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "wasm-backends")]
use flick_core::project;
#[cfg(feature = "wasm-backends")]
use flick_core::project::lock::{LOCK_FILE, Lock};
use flick_core::source::SourceMap;

#[cfg(feature = "wasm-backends")]
use source::{LockPin, load_source};

use crate::args::ScaffoldArguments;
#[cfg(feature = "wasm-backends")]
use crate::artifact::{self, Artifact, validate_destination_paths};
use crate::command::{Command, Outcome};
#[cfg(feature = "wasm-backends")]
use crate::commands::contributed::plan::standalone_diagnostic;
#[cfg(feature = "wasm-backends")]
use crate::toolchain::COMPILER;
#[cfg(feature = "wasm-backends")]
use crate::{fetch, publish};

pub struct ScaffoldCommand {
    pub dir: PathBuf,
    pub options: ScaffoldArguments,
    /// `new`: the directory must not exist yet.
    pub fresh: bool,
    pub offline: bool,
}

impl Command for ScaffoldCommand {
    fn run(&self) -> Outcome {
        #[cfg(feature = "wasm-backends")]
        {
            self.run_with_backends()
        }
        #[cfg(not(feature = "wasm-backends"))]
        {
            let _ = (&self.dir, &self.options, self.fresh, self.offline);
            Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![Diagnostic::new(
                    codes::P013,
                    Severity::Error,
                    "project scaffolding requires WebAssembly backend support",
                )],
            }
        }
    }
}

#[cfg(feature = "wasm-backends")]
impl ScaffoldCommand {
    fn run_with_backends(&self) -> Outcome {
        let mut sources = SourceMap::default();
        if self.fresh && fs::symlink_metadata(&self.dir).is_ok() {
            return single_error(
                sources,
                codes::P019,
                format!(
                    "`{}` exists: `flick new` makes a new directory; use `flick init` inside one",
                    self.dir.display()
                ),
            );
        }

        let existing_lock = match self.read_existing_lock() {
            Ok(lock) => lock,
            Err(diag) => return error_outcome(sources, diag),
        };

        let loaded = match load_source(
            &self.dir,
            &self.options,
            existing_lock.as_ref(),
            self.offline,
            &mut sources,
        ) {
            Ok(loaded) => loaded,
            Err(diag) => return error_outcome(sources, diag),
        };

        let available = match loaded.contributor.templates() {
            Ok(templates) => templates,
            Err(why) => {
                return single_error(
                    sources,
                    codes::P013,
                    format!("cannot list templates from contributor: {why}"),
                );
            }
        };

        let template = match select_template(self.options.template.as_deref(), &available) {
            Ok(name) => name,
            Err(diag) => return error_outcome(sources, diag),
        };

        let name = package_name(&self.dir);
        let request = TemplateRequest {
            template: template.clone(),
            name: name.clone(),
            compiler: COMPILER.to_owned(),
            init: !self.fresh,
        };
        let plan = match loaded.contributor.scaffold(&request) {
            Ok(plan) => plan,
            Err(diagnostics) => {
                return Outcome {
                    sources,
                    diagnostics: diagnostics.iter().map(standalone_diagnostic).collect(),
                };
            }
        };

        let (artifacts, notes_and_warnings) = match validate_template_plan(&template, &plan) {
            Ok(valid) => valid,
            Err(diagnostics) => {
                return Outcome {
                    sources,
                    diagnostics,
                };
            }
        };

        let (artifacts, lock_artifact) = match apply_lock_pin(
            artifacts,
            existing_lock,
            loaded.pin,
            &template,
            &mut sources,
        ) {
            Ok(pair) => pair,
            Err(diag) => return error_outcome(sources, diag),
        };

        let writes = match self.prepare_writes(artifacts, lock_artifact) {
            Ok(writes) => writes,
            Err(diagnostics) => {
                return Outcome {
                    sources,
                    diagnostics,
                };
            }
        };

        if let Err(diag) = self.publish_writes(&writes) {
            return error_outcome(sources, diag);
        }

        let action = if self.fresh { "Created" } else { "Initialized" };
        println!("{action} `{name}` ({template}) in {}", shown(&self.dir));
        let _ = io::stdout().write_all(&plan.stdout);

        Outcome {
            sources,
            diagnostics: notes_and_warnings,
        }
    }

    fn read_existing_lock(&self) -> Result<Option<Lock>, Diagnostic> {
        if self.fresh {
            return Ok(None);
        }
        let path = self.dir.join(LOCK_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        fetch::read_lock(&path)
            .map(Some)
            .map_err(|msg| Diagnostic::new(codes::P034, Severity::Error, msg))
    }

    fn prepare_writes(
        &self,
        artifacts: Vec<Artifact>,
        lock_artifact: Option<Artifact>,
    ) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
        if self.fresh {
            let mut writes = artifacts;
            writes.extend(lock_artifact);
            return Ok(writes);
        }
        let mut collisions = Vec::new();
        let mut writes = Vec::with_capacity(artifacts.len() + usize::from(lock_artifact.is_some()));
        for artifact in artifacts {
            let target = self.dir.join(&artifact.path);
            if fs::symlink_metadata(&target).is_ok() {
                if artifact.path == "AGENTS.md" {
                    continue;
                }
                collisions.push(Diagnostic::new(
                    codes::P019,
                    Severity::Error,
                    format!(
                        "`{}` exists; `flick init` writes nothing over it",
                        artifact.path
                    ),
                ));
            } else {
                writes.push(artifact);
            }
        }
        if !collisions.is_empty() {
            return Err(collisions);
        }
        writes.extend(lock_artifact);
        Ok(writes)
    }

    fn publish_writes(&self, writes: &[Artifact]) -> Result<(), Diagnostic> {
        let created_root = if self.fresh {
            let abs = resolve_path(&self.dir);
            let missing = first_missing_ancestor(&abs);
            fs::create_dir_all(&self.dir).map_err(|err| {
                Diagnostic::new(
                    codes::P004,
                    Severity::Error,
                    format!("cannot create `{}`: {err}", self.dir.display()),
                )
            })?;
            missing
        } else {
            None
        };

        let cleanup = |root: &Option<PathBuf>| {
            if let Some(dir) = root {
                let _ = fs::remove_dir_all(dir);
            }
        };

        let root = match fs::canonicalize(&self.dir) {
            Ok(root) => root,
            Err(err) => {
                cleanup(&created_root);
                return Err(Diagnostic::new(
                    codes::P004,
                    Severity::Error,
                    format!("cannot inspect `{}`: {err}", self.dir.display()),
                ));
            }
        };

        if let Err(why) = validate_destination_paths(&root, writes.iter().map(|a| a.path.as_str()))
        {
            cleanup(&created_root);
            return Err(Diagnostic::new(codes::P055, Severity::Error, why));
        }

        if let Err(err) = publish::commit(&root, writes, &[], None) {
            cleanup(&created_root);
            return Err(Diagnostic::new(
                codes::P004,
                Severity::Error,
                format!("cannot write project files: {err}"),
            ));
        }

        Ok(())
    }
}

#[cfg(feature = "wasm-backends")]
fn single_error(sources: SourceMap, code: Code, message: String) -> Outcome {
    error_outcome(sources, Diagnostic::new(code, Severity::Error, message))
}

#[cfg(feature = "wasm-backends")]
fn error_outcome(sources: SourceMap, diagnostic: Diagnostic) -> Outcome {
    Outcome {
        sources,
        diagnostics: vec![diagnostic],
    }
}

#[cfg(feature = "wasm-backends")]
fn format_available(available: &[TemplateDescriptor]) -> String {
    available
        .iter()
        .map(|t| format!("  {} — {}", t.name, t.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(feature = "wasm-backends")]
fn select_template(
    requested: Option<&str>,
    available: &[TemplateDescriptor],
) -> Result<String, Diagnostic> {
    if let Some(name) = requested {
        if available.iter().any(|t| t.name == name) {
            return Ok(name.to_owned());
        }
        let message = if available.is_empty() {
            format!("unsupported template `{name}`: this contributor provides no project templates")
        } else {
            format!(
                "unsupported template `{name}`; available templates:\n{}",
                format_available(available)
            )
        };
        let mut diag = Diagnostic::new(codes::P060, Severity::Error, message);
        if let Some(suggestion) = suggest::closest(name, available.iter().map(|t| t.name.as_str()))
        {
            diag = diag.with_fix(|| (format!("did you mean `--template {suggestion}`?"), vec![]));
        }
        return Err(diag);
    }

    if available.iter().any(|t| t.name == "app") {
        return Ok("app".to_owned());
    }
    if let [only] = available {
        return Ok(only.name.clone());
    }
    if available.is_empty() {
        return Err(Diagnostic::new(
            codes::P060,
            Severity::Error,
            "this contributor provides no project templates",
        ));
    }
    Err(Diagnostic::new(
        codes::P060,
        Severity::Error,
        format!(
            "pass `--template <name>` to choose a template; available templates:\n{}",
            format_available(available)
        ),
    ))
}

#[cfg(feature = "wasm-backends")]
fn validate_template_plan(
    template: &str,
    plan: &Plan,
) -> Result<(Vec<Artifact>, Vec<Diagnostic>), Vec<Diagnostic>> {
    if !plan.steps.is_empty() || plan.resume.is_some() || !plan.needs.is_empty() {
        return Err(vec![Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!(
                "template `{template}` plan is refused: templates may only return files and diagnostics, not host steps or continuations"
            ),
        )]);
    }
    let diagnostics: Vec<Diagnostic> = plan.diagnostics.iter().map(standalone_diagnostic).collect();
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err(diagnostics);
    }
    let artifacts: Vec<Artifact> = plan
        .files
        .iter()
        .map(|f| Artifact {
            path: f.path.clone(),
            contents: f.bytes.clone(),
            kind: if f.executable {
                ArtifactKind::Executable
            } else {
                ArtifactKind::Resource
            },
        })
        .collect();
    if let Err(why) = artifact::validate(&artifacts) {
        return Err(vec![Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!("template `{template}` plan is refused: {why}"),
        )]);
    }
    Ok((artifacts, diagnostics))
}

#[cfg(feature = "wasm-backends")]
fn apply_lock_pin(
    mut artifacts: Vec<Artifact>,
    existing_lock: Option<Lock>,
    pin: LockPin,
    template: &str,
    sources: &mut SourceMap,
) -> Result<(Vec<Artifact>, Option<Artifact>), Diagnostic> {
    let apply = |lock: &mut Lock, pin: LockPin| match pin {
        LockPin::OfficialBackend(Some(entry)) => {
            lock.backends.retain(|b| b.id != entry.id);
            lock.backends.push(entry);
            true
        }
        LockPin::OfficialBackend(None) => false,
        LockPin::Template(mut entry) => {
            template.clone_into(&mut entry.template);
            lock.templates
                .retain(|t| !(t.template == entry.template && t.package == entry.package));
            lock.templates.push(entry);
            true
        }
    };

    if matches!(pin, LockPin::OfficialBackend(None)) {
        return Ok((artifacts, None));
    }

    if let Some(existing_in_plan) = artifacts.iter_mut().find(|a| a.path == LOCK_FILE) {
        let text = String::from_utf8(existing_in_plan.contents.clone()).map_err(|_| {
            Diagnostic::new(
                codes::P055,
                Severity::Error,
                format!("template `{template}` wrote a non-UTF-8 `{LOCK_FILE}`"),
            )
        })?;
        let file = sources.add(LOCK_FILE, &text);
        let mut lock = Lock::parse(&text, file)?;
        apply(&mut lock, pin);
        existing_in_plan.contents = lock.render().into_bytes();
        return Ok((artifacts, None));
    }

    let mut lock = existing_lock.unwrap_or_default();
    if apply(&mut lock, pin) {
        Ok((
            artifacts,
            Some(Artifact::source(LOCK_FILE.to_owned(), lock.render())),
        ))
    } else {
        Ok((artifacts, None))
    }
}

#[cfg(feature = "wasm-backends")]
fn shown(dir: &Path) -> String {
    fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .display()
        .to_string()
}

/// Resolves the existing prefix of `path` canonically and joins any remaining
/// non-existent components, so relative paths stay consistent across symlinks
/// such as `/var` → `/private/var` on macOS.
#[cfg(feature = "wasm-backends")]
pub(super) fn resolve_path(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    let cwd = env::current_dir()
        .ok()
        .and_then(|d| fs::canonicalize(&d).ok().or(Some(d)))
        .unwrap_or_default();
    let full = project::normalize(&cwd.join(path));
    let mut tail = Vec::new();
    let mut cur = full.as_path();
    loop {
        if let Ok(real) = fs::canonicalize(cur) {
            return tail
                .into_iter()
                .rev()
                .fold(real, |acc, part| acc.join(part));
        }
        let Some(name) = cur.file_name() else {
            return full;
        };
        tail.push(name.to_os_string());
        let Some(parent) = cur.parent() else {
            return full;
        };
        cur = parent;
    }
}

#[cfg(feature = "wasm-backends")]
fn first_missing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut missing = None;
    let mut cur = path;
    while fs::symlink_metadata(cur).is_err() {
        missing = Some(cur.to_path_buf());
        let Some(parent) = cur.parent() else {
            break;
        };
        if parent.as_os_str().is_empty() {
            break;
        }
        cur = parent;
    }
    missing
}

/// A normalized package/project name from the directory: lowercase ASCII
/// letters, digits and `-`, defaulting to `"app"` when none remain.
#[cfg(feature = "wasm-backends")]
fn package_name(dir: &Path) -> String {
    let base = fs::canonicalize(dir)
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "app".to_owned());
    let name: String = base
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        "app".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(all(test, feature = "wasm-backends"))]
mod tests {
    use flick_backend::command::{FlickModule, Plan, RunFlickStep, Step};
    use flick_core::diagnostics::codes;

    use super::validate_template_plan;

    #[test]
    fn template_plan_refuses_run_flick_steps_with_p055() {
        let mut plan = Plan::empty();
        plan.steps.push(Step::RunFlick(RunFlickStep {
            id: "export".into(),
            entry: "Gen".into(),
            modules: vec![FlickModule {
                name: "Gen".into(),
                source: "module Gen exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n"
                    .into(),
            }],
            arguments: vec![],
            output: None,
            origin: None,
        }));
        let errs = validate_template_plan("app", &plan).expect_err("templates forbid run-flick");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, codes::P055);
    }
}
