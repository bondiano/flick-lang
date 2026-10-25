use std::path::{Path, PathBuf};
use std::{fs, io, process};

use flick_core::build::{self, Artifact, Extras, Settings};
use flick_core::codegen::Labels;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{Backend, Build, Emit, Kind, MAIN_ENTRY, Profile};
use flick_core::project::{self, Compilation, Compiled, Project};
use flick_core::source::{FileId, SourceMap};

use super::check::select;
use crate::args::{BuildArguments, EmitArgument, LabelsArgument, ProfileArgument};
use crate::command::{Command, Outcome};
use crate::sources::FsHost;
use crate::{cache, toolchain};

pub struct BuildCommand {
    pub options: BuildArguments,
    pub project: Option<String>,
    /// `run --entry`: the module whose `main` the runner of the entry `main`
    /// starts; empty for no runner.
    pub entry: Option<String>,
    /// A build into a run generation, which lives only while it runs: no
    /// manifest entry, and a cache key without its number.
    pub generation: bool,
}

impl BuildCommand {
    pub fn new(options: BuildArguments, project: Option<String>) -> Self {
        Self {
            options,
            project,
            entry: None,
            generation: false,
        }
    }
}

impl Command for BuildCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let host = FsHost::new();
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
        if let Some(problem) = project
            .as_ref()
            .and_then(|p| toolchain::check_runtime(&p.root))
        {
            return Outcome {
                sources,
                diagnostics: vec![problem],
            };
        }
        let cached = project.as_ref().and_then(|p| self.cached(p, &roots, &host));
        if let Some(diagnostics) = cached {
            return Outcome {
                sources,
                diagnostics,
            };
        }
        let (mut diagnostics, mut compilation) =
            project::compile(&host, &mut sources, project.as_ref(), &roots);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Outcome {
                sources,
                diagnostics,
            };
        }
        compilation.modules.retain(|c| !c.test);
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
        let output = self.output(&host, project.as_ref(), entries);
        let (problems, artifacts) = emit(&host, &sources, project.as_ref(), &compilation, &output);
        let clean = diagnostics.is_empty() && problems.is_empty();
        diagnostics.extend(problems);
        if let Some(p) = &project {
            if clean && !output.extras.schema_aot {
                cache::store(&p.root, &self.key(p, &roots, &output), &artifacts);
            }
            if !self.generation {
                cache::record(&p.root, &output.out_dir, &artifacts);
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
    fn output(
        &self,
        host: &FsHost,
        project: Option<&Project>,
        entries: Vec<(String, String)>,
    ) -> Output {
        let out_dir = match (&self.options.out_dir, project) {
            (Some(dir), _) => host.cwd.join(dir),
            (None, Some(p)) => p.root.join(&p.config.build.out_dir),
            (None, None) => host.cwd.join("dist"),
        };
        let typescript = match (self.options.emit, project) {
            (Some(emit), _) => emit == EmitArgument::Ts,
            (None, Some(p)) => p.config.build.emit == Emit::Ts,
            (None, None) => false,
        };
        let config = project.map(|p| &p.config.build);
        Output {
            out_dir: project::normalize(&out_dir),
            entries,
            typescript,
            extras: Extras {
                dts: self.options.emit_dts || config.is_some_and(|b| b.emit_dts),
                promise_facade: self.options.promise_facade
                    || config.is_some_and(|b| b.promise_facade),
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

    fn key(&self, project: &Project, roots: &[PathBuf], output: &Output) -> String {
        // generations sit at one depth, so their artifacts are the same
        let out_dir = if self.generation {
            project.root.join(".flick/run/*")
        } else {
            output.out_dir.clone()
        };
        let package = project::project_name(&FsHost::new(), &project.root).unwrap_or_default();
        let settings = format!(
            "{} {:?} {} {:?} {} {package}",
            out_dir.display(),
            output.entries,
            output.typescript,
            output.extras,
            output.release
        );
        cache::key(project, roots, &settings)
    }

    /// The artifacts of an earlier build with the same key, written again.
    fn cached(
        &self,
        project: &Project,
        roots: &[PathBuf],
        host: &FsHost,
    ) -> Option<Vec<Diagnostic>> {
        let output = self.output(host, Some(project), self.entries(project));
        if output.extras.schema_aot {
            return None;
        }
        let artifacts = cache::load(&project.root, &self.key(project, roots, &output))?;
        for artifact in &artifacts {
            let path = output.out_dir.join(&artifact.path);
            if let Err(err) = write(&path, &artifact.contents) {
                return Some(vec![Diagnostic::new(
                    codes::P013,
                    Severity::Error,
                    format!("cannot write `{}`: {err}", path.display()),
                )]);
            }
        }
        if !self.generation {
            cache::record(&project.root, &output.out_dir, &artifacts);
        }
        Some(Vec::new())
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
pub struct Output {
    pub out_dir: PathBuf,
    /// The entries a runner is written for, by name and module.
    pub entries: Vec<(String, String)>,
    pub typescript: bool,
    pub extras: Extras,
    pub release: bool,
}

/// Writes the build of `compilation`; what could not be written or has no
/// TypeScript form is reported.
pub fn emit(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &Output,
) -> (Vec<Diagnostic>, Vec<Artifact>) {
    let out_dir = output.out_dir.as_path();
    let (artifacts, mut diagnostics) = artifacts(host, sources, project, compilation, output);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return (diagnostics, Vec::new());
    }
    for artifact in &artifacts {
        let path = out_dir.join(&artifact.path);
        if let Err(err) = write(&path, &artifact.contents) {
            diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot write `{}`: {err}", path.display()),
            ));
            break;
        }
    }
    let aot = if output.extras.schema_aot && diagnostics.is_empty() {
        output.entries.as_slice()
    } else {
        &[]
    };
    for (name, entry) in aot {
        let ext = if output.typescript { "ts" } else { "js" };
        let module = out_dir.join(format!("{}.{ext}", entry.replace('.', "/")));
        let file = build::schema_aot_file(name);
        if let Err(reason) = compile_schemas(out_dir, &module, &file) {
            diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot write the compiled decoders `{file}`: {reason}"),
            ));
            break;
        }
    }
    (diagnostics, artifacts)
}

/// Loads the app's modules on Node and writes the compiled decoders of the
/// codecs they make, which the runner installs.
fn compile_schemas(out_dir: &Path, module: &Path, file: &str) -> Result<(), String> {
    let script = "import { writeSchemaAot } from \"@flick-lang/runtime\";\nawait writeSchemaAot(process.argv[1], process.argv[2]);\n";
    let output = process::Command::new("node")
        .args(["--input-type=module", "-e", script])
        .arg(module)
        .arg(out_dir.join(file))
        .current_dir(out_dir)
        .output()
        .map_err(|err| format!("cannot run node: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

pub fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}

/// [`emit`] without writing anything.
pub fn artifacts(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &Output,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
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
        backend: project.map_or(Backend::Effect, |p| p.config.build.effect),
        ui: project.and_then(|p| p.config.build.ui.as_deref()),
        runner: project.and_then(|p| p.config.runner.as_ref().map(|r| r.0.as_str())),
    };
    build::emit(compilation, sources, &settings)
}
