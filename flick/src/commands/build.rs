use std::path::{Path, PathBuf};
use std::{fs, io, process};

use flick_core::build::{self, Extras, Settings};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{Emit, Kind};
use flick_core::project::{self, Compilation, Compiled, Project};
use flick_core::source::{FileId, SourceMap};

use super::check::select;
use crate::args::{BuildArguments, EmitArgument};
use crate::command::{Command, Outcome};
use crate::sources::FsHost;

pub struct BuildCommand {
    options: BuildArguments,
    project: Option<String>,
}

impl BuildCommand {
    pub fn new(options: BuildArguments, project: Option<String>) -> Self {
        Self { options, project }
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
        let entry = match &project {
            Some(p) if p.config.kind == Kind::App => p.config.entry.as_ref().map(|e| e.0.clone()),
            Some(_) => None,
            None => {
                let mains: Vec<&Compiled> = modules
                    .iter()
                    .filter(|m| roots.contains(&m.path) && m.interface.exports_value("main"))
                    .collect();
                match mains[..] {
                    [only] => Some(only.name.clone()),
                    _ => None,
                }
            }
        };
        let out_dir = match (&self.options.out_dir, &project) {
            (Some(dir), _) => host.cwd.join(dir),
            (None, Some(p)) => p.root.join(&p.config.build.out_dir),
            (None, None) => host.cwd.join("dist"),
        };
        let typescript = match (self.options.emit, &project) {
            (Some(emit), _) => emit == EmitArgument::Ts,
            (None, Some(p)) => p.config.build.emit == Emit::Ts,
            (None, None) => false,
        };
        let config = project.as_ref().map(|p| &p.config.build);
        let output = Output {
            out_dir: &out_dir,
            entry: entry.as_deref(),
            typescript,
            extras: Extras {
                dts: self.options.emit_dts || config.is_some_and(|b| b.emit_dts),
                promise_facade: self.options.promise_facade
                    || config.is_some_and(|b| b.promise_facade),
                schema_aot: self.options.schema_aot || config.is_some_and(|b| b.schema_aot),
            },
        };
        diagnostics.extend(emit(
            &host,
            &sources,
            project.as_ref(),
            &compilation,
            &output,
        ));
        Outcome {
            sources,
            diagnostics,
        }
    }
}

/// Where and what a build writes.
pub struct Output<'a> {
    pub out_dir: &'a Path,
    pub entry: Option<&'a str>,
    pub typescript: bool,
    pub extras: Extras,
}

/// Writes the build of `compilation`; what could not be written or has no
/// TypeScript form is reported.
pub fn emit(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &Output,
) -> Vec<Diagnostic> {
    let out_dir = output.out_dir;
    let modules = &compilation.modules;
    let package = project.and_then(|p| project::project_name(host, &p.root));
    let paths: Vec<(FileId, PathBuf)> = modules.iter().map(|m| (m.file, m.path.clone())).collect();
    let source_path = |file: FileId| {
        paths
            .iter()
            .find(|(f, _)| *f == file)
            .map_or_else(String::new, |(_, p)| format!("file://{}", p.display()))
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
        entry: output.entry,
        typescript: output.typescript,
        source_path: &source_path,
        extras: output.extras,
    };
    let (artifacts, mut diagnostics) = build::emit(compilation, sources, &settings);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return diagnostics;
    }
    for artifact in artifacts {
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
    if let (true, Some(entry)) = (output.extras.schema_aot, output.entry)
        && diagnostics.is_empty()
    {
        let ext = if output.typescript { "ts" } else { "js" };
        let module = out_dir.join(format!("{}.{ext}", entry.replace('.', "/")));
        if let Err(reason) = compile_schemas(out_dir, &module) {
            diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!(
                    "cannot write the compiled decoders `{}`: {reason}",
                    build::SCHEMA_AOT
                ),
            ));
        }
    }
    diagnostics
}

/// Loads the app's modules on Node and writes the compiled decoders of the
/// codecs they make, which the runner installs.
fn compile_schemas(out_dir: &Path, module: &Path) -> Result<(), String> {
    let script = "import { writeSchemaAot } from \"@flick-lang/runtime\";\nawait writeSchemaAot(process.argv[1], process.argv[2]);\n";
    let output = process::Command::new("node")
        .args(["--input-type=module", "-e", script])
        .arg(module)
        .arg(out_dir.join(build::SCHEMA_AOT))
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
