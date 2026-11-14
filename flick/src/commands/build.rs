use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use flick_core::build::{self, Extras, Settings};
use flick_core::codegen::Labels;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{Backend, Build, Emit, Kind, MAIN_ENTRY, Profile};
use flick_core::project::{self, Compilation, Compiled, ModuleCache, Project, js};
use flick_core::source::{FileId, SourceMap};

use self::js_artifacts::js_artifacts;
use super::check::select;
use super::emit_ir;
use super::out_dir::overlap;
use super::summary::{self, Built};
use crate::args::{BuildArguments, EmitArgument, LabelsArgument, ProfileArgument};
use crate::artifact::Artifact;
use crate::command::{Command, Outcome};
use crate::package_cache;
use crate::progress::{Mode, Progress};
use crate::sources::FsHost;
use crate::{builtins, cache, toolchain};

mod backend;
mod cached;
pub(super) mod hooks;
mod js_artifacts;
mod progress;
mod publication;
pub use publication::write;
pub(super) use publication::write_artifacts;
#[cfg(test)]
mod publication_tests;
mod schemas;
mod shipped;
mod unwritten;

pub struct BuildCommand {
    pub options: BuildArguments,
    pub project: Option<String>,
    pub display: Option<Mode>,
    /// `run --entry`: the module whose `main` the runner of the entry `main`
    /// starts; empty for no runner.
    pub entry: Option<String>,
    /// A build into a run generation, which lives only while it runs: no
    /// manifest entry, and a cache key without its number.
    pub generation: bool,
    /// What the last run wrote, for its summary.
    pub built: RefCell<Option<Built>>,
    /// What the builds before this one compiled, for a command that builds
    /// again and again: `--watch` and `dev`. One build keeps nothing.
    pub modules: Option<Rc<RefCell<ModuleCache>>>,
}

impl BuildCommand {
    pub fn new(options: BuildArguments, project: Option<String>, display: Option<Mode>) -> Self {
        Self {
            modules: options.watch.then(Rc::default),
            options,
            project,
            display,
            entry: None,
            generation: false,
            built: RefCell::default(),
        }
    }

    fn record(&self, output: &BuildPlan, modules: Option<usize>, modules_of: &[(String, String)]) {
        let runners = modules_of
            .iter()
            .map(|(name, module)| {
                let file = if output.go {
                    build::go::executable(name)
                } else {
                    build::runner_file(name, output.typescript)
                };
                (module.clone(), output.out_dir.join(file))
            })
            .collect();
        *self.built.borrow_mut() = Some(Built {
            modules,
            out_dir: output.out_dir.clone(),
            runners,
        });
    }
}

impl Command for BuildCommand {
    fn summary(&self, outcome: &Outcome, took: Duration, color: bool) -> Option<String> {
        if self.options.inputs {
            return None;
        }
        let built = self.built.borrow();
        Some(summary::text(built.as_ref(), outcome, took, color))
    }

    #[allow(clippy::too_many_lines)]
    fn run(&self) -> Outcome {
        let progress = self.progress();
        let mut sources = SourceMap::default();
        let host = FsHost::new();
        progress.stage("Resolving project");
        let selected = select(
            &host,
            &mut sources,
            self.project.as_deref(),
            &self.options.paths,
        );
        let (project, roots) = match selected {
            Ok(selected) => selected,
            Err(diagnostic) => {
                return Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                };
            }
        };
        if self.options.inputs {
            return super::inputs::describe(&host, sources, project.as_ref(), &roots);
        }
        let plan = self.plan(&host, project.as_ref(), Vec::new());
        let go = plan.go;
        if plan.extras.rt && go {
            let message = "Flick's own runtime does not support Go output";
            return Outcome {
                sources,
                diagnostics: vec![Diagnostic::new(codes::F004, Severity::Error, message)],
            };
        }
        if let Some(problem) = project
            .as_ref()
            .filter(|_| !go)
            .and_then(|p| toolchain::check_runtime(&p.root, plan.extras.rt))
        {
            return Outcome {
                sources,
                diagnostics: vec![problem],
            };
        }
        *self.built.borrow_mut() = None;
        if let Some(p) = &project
            && let Some(problem) = overlap(p, &self.plan(&host, Some(p), Vec::new()).out_dir)
        {
            return Outcome {
                sources,
                diagnostics: vec![problem],
            };
        }
        let initial_key = project.as_ref().map(|p| {
            let output = self.plan(&host, Some(p), self.entries(p));
            self.key(p, &roots, &output)
        });
        // a cached build has no IR to write
        let cached = project
            .as_ref()
            .zip(initial_key.as_ref())
            .filter(|_| self.options.emit_ir.is_none())
            .and_then(|(p, key)| self.cached(p, &host, &progress, key));
        if let Some(diagnostics) = cached {
            return Outcome {
                sources,
                diagnostics,
            };
        }
        // only a build that compiles reads the built-in modules: a cache hit
        // restored its artifacts above
        if let Err(outcome) = self.prepare(&progress) {
            return outcome;
        }
        if project::stages_by_class(project.as_ref(), plan.extras.rt)
            && let Err(diagnostic) = builtins::prepare_in(true)
        {
            return Outcome {
                sources,
                diagnostics: vec![diagnostic],
            };
        }
        progress.stage("Compiling Flick modules");
        let mut kept = self.modules.as_ref().map(|modules| modules.borrow_mut());
        let (mut diagnostics, mut compilation) = package_cache::compile(
            &host,
            &mut sources,
            project.as_ref(),
            &roots,
            plan.extras.rt,
            kept.as_deref_mut(),
        );
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Outcome {
                sources,
                diagnostics,
            };
        }
        compilation.modules.retain(|c| !c.test);
        hooks::ir(project.as_ref(), &mut compilation, &mut diagnostics);
        let modules = &compilation.modules;
        let entries = match &project {
            Some(p) => self.entries(p),
            None if self.entry.is_some() => self.entries_given(),
            // files built alone: the one of them that exports `main`
            None => {
                let mains: Vec<&Compiled> = modules
                    .iter()
                    .filter(|m| roots.contains(&m.path) && m.interface.exports_value("main"))
                    .collect();
                match mains[..] {
                    [only] => vec![(MAIN_ENTRY.to_owned(), only.name.clone())],
                    _ => Vec::new(),
                }
            }
        };
        let output = self.plan(&host, project.as_ref(), entries);
        self.record(&output, Some(modules.len()), &output.entries);
        progress.stage(match (output.go, output.typescript) {
            (true, _) => "Emitting Go",
            (false, true) => "Emitting TypeScript",
            (false, false) => "Emitting JavaScript",
        });
        let mut emitted = artifacts(&host, &sources, project.as_ref(), &compilation, &output);
        let extra = self
            .options
            .emit_ir
            .map(|format| emit_ir::artifact(&compilation, &sources, &output, format));
        self.publish(project.as_ref(), &output, &mut emitted, extra, &progress);
        let clean = diagnostics.is_empty() && emitted.diagnostics.is_empty();
        diagnostics.extend(emitted.diagnostics);
        if let Some(p) = &project
            && clean
            && !output.extras.schema_aot
            && self.options.emit_ir.is_none()
        {
            progress.stage("Saving build cache");
            let key = self.key(p, &roots, &output);
            let read = (&roots[..], &*host.cwd, &sources);
            let stored = initial_key.as_ref().is_some_and(|before| {
                cache::store_built(p, read, before, &key, &emitted.artifacts)
            });
            if stored && output.go {
                for (name, _) in &output.entries {
                    cache::mark_go(&p.root, &key, &output.out_dir, &build::go::executable(name));
                }
            }
        }
        Outcome {
            sources,
            diagnostics,
        }
    }
}

impl BuildCommand {
    /// The entries of the project's build, by name and module: `--entry`,
    /// else those of an app; a library has none.
    pub fn entries(&self, project: &Project) -> Vec<(String, String)> {
        if self.entry.is_some() {
            return self.entries_given();
        }
        match project.config.kind {
            Kind::App => project
                .config
                .entries
                .iter()
                .map(|e| (e.name.clone(), e.module.clone()))
                .collect(),
            Kind::Lib => Vec::new(),
        }
    }

    /// `--entry M` as the entry `main`; an empty one builds no runner.
    fn entries_given(&self) -> Vec<(String, String)> {
        self.entry
            .iter()
            .filter(|e| !e.is_empty())
            .map(|e| (MAIN_ENTRY.to_owned(), e.clone()))
            .collect()
    }

    /// What the build writes where.
    fn plan(
        &self,
        host: &FsHost,
        project: Option<&Project>,
        entries: Vec<(String, String)>,
    ) -> BuildPlan {
        let out_dir = match (&self.options.out_dir, project) {
            (Some(dir), _) => host.cwd.join(dir),
            (None, Some(p)) => p.root.join(&p.config.build.out_dir),
            (None, None) => host.cwd.join("dist"),
        };
        let emit = match (self.options.emit, project) {
            (Some(EmitArgument::Ts), _) => Emit::Ts,
            (Some(EmitArgument::Go), _) => Emit::Go,
            (None, Some(p)) => p.config.build.emit,
            (Some(EmitArgument::Js), _) | (None, None) => Emit::Js,
        };
        let config = project.map(|p| &p.config.build);
        BuildPlan {
            out_dir: project::normalize(&out_dir),
            entries,
            typescript: emit == Emit::Ts,
            go: emit == Emit::Go,
            suites: Vec::new(),
            extras: Extras {
                dts: self.options.emit_dts || config.is_some_and(|b| b.emit_dts),
                promise_facade: self.options.promise_facade
                    || config.is_some_and(|b| b.promise_facade),
                rt: self.options.rt
                    || (emit != Emit::Go && config.is_some_and(|b| b.effect == Backend::Rt)),
                schema_aot: self.options.schema_aot || config.is_some_and(|b| b.schema_aot),
                labels: match self.options.labels {
                    Some(LabelsArgument::All) => Labels::All,
                    Some(LabelsArgument::Components) => Labels::Components,
                    Some(LabelsArgument::None) => Labels::None,
                    None => config.map_or_else(Labels::default, |b| b.labels),
                },
            },
            release: release(self.options.profile, config),
        }
    }

    fn key(&self, project: &Project, roots: &[PathBuf], output: &BuildPlan) -> cache::Signed {
        // generations sit at one depth, so their artifacts are the same
        let out_dir = if self.generation {
            project.root.join(".flick/run/*")
        } else {
            output.out_dir.clone()
        };
        let package = project::project_name(&FsHost::new(), &project.root).unwrap_or_default();
        let settings = format!(
            "{} {:?} {} {} {:?} {} {package}",
            out_dir.display(),
            output.entries,
            output.typescript,
            output.go,
            output.extras,
            output.release
        );
        cache::key(project, roots, &settings, !output.go && !output.extras.rt)
    }
}

/// Whether the build optimizes: `--profile`, else `build.profile`.
pub fn release(profile: Option<ProfileArgument>, config: Option<&Build>) -> bool {
    match profile {
        Some(p) => p == ProfileArgument::Release,
        None => config.is_some_and(|b| b.profile == Profile::Release),
    }
}

/// Where and what a build writes.
pub struct BuildPlan {
    pub out_dir: PathBuf,
    /// The entries a runner is written for, by name and module.
    pub entries: Vec<(String, String)>,
    pub typescript: bool,
    /// The go target: a Go module per entry, built into an executable.
    pub go: bool,
    /// For the go target: the test modules whose tests the one executable
    /// `flick-test` runs, instead of the entries.
    pub suites: Vec<String>,
    pub extras: Extras,
    pub release: bool,
}

/// Native emission keeps diagnostics and artifacts in named fields.
pub struct Emission {
    pub artifacts: Vec<Artifact>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Writes the build of `compilation`; what could not be written or has no
/// TypeScript form is reported.
pub fn emit(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Emission {
    let mut emitted = artifacts(host, sources, project, compilation, output);
    write_artifacts(output, &mut emitted, &Progress::new(Mode::Hidden));
    emitted
}

/// [`emit`] without writing anything.
pub fn artifacts(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Emission {
    let out_dir = output.out_dir.as_path();
    let modules = &compilation.modules;
    let package = project.and_then(|p| project::project_name(host, &p.root));
    let paths: Vec<(FileId, PathBuf)> = modules.iter().map(|m| (m.file, m.path.clone())).collect();
    let source_path = |file: FileId| {
        paths
            .iter()
            .find(|(f, _)| *f == file)
            .map_or_else(String::new, |(_, p)| p.display().to_string())
    };
    let dirs: Vec<(String, PathBuf)> = modules
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                m.path.parent().map(Path::to_path_buf).unwrap_or_default(),
            )
        })
        .collect();
    let source_dir = |module: &str| {
        dirs.iter()
            .find(|(name, _)| name == module)
            .map(|(_, dir)| dir.clone())
    };
    let runners: Vec<(String, String)> = project
        .iter()
        .flat_map(|p| &p.config.entries)
        .filter_map(|e| Some((e.name.clone(), e.runner.as_ref()?.0.clone())))
        .collect();
    let backend = if output.extras.rt {
        Backend::Rt
    } else {
        project.map_or(Backend::Effect, |p| p.config.build.effect)
    };

    let settings = Settings {
        source_dir: &source_dir,
        out_dir,
        package: package.as_deref(),
        entries: &output.entries,
        typescript: output.typescript,
        source_path: &source_path,
        extras: output.extras,
        release: output.release,
        root: project.map(|p| p.root.as_path()),
        backend,
        effect_bridge: backend::effect_bridge(host, project, compilation),
        ui: project.and_then(|p| p.config.build.ui.as_deref()),
        runners: &runners,
    };
    if output.go {
        let (artifacts, diagnostics) =
            build::go::emit(compilation, sources, &settings, &output.suites, &|path| {
                fs::read_to_string(path).ok()
            });
        return Emission {
            artifacts: artifacts.into_iter().map(Artifact::from).collect(),
            diagnostics,
        };
    }
    let mut emission = js_artifacts(sources, compilation, &settings);
    if let Some(project) = project {
        let wanted = &compilation.js_dependencies;
        let needs = &mut emission.diagnostics;
        needs.extend(js::conflicts(host, &project.root, wanted));
        needs.extend(js::unmet(host, &project.root, wanted));
    } else {
        // files built alone have no manifest of the outputs: the list names
        // them, so the next build removes those it does not write
        let contents = (emission.artifacts.iter()).fold(String::new(), |mut list, a| {
            list.push_str(&a.path);
            list.push('\n');
            list
        });
        emission
            .artifacts
            .push(Artifact::source(build::go::FILES.to_owned(), contents));
    }
    emission
}
