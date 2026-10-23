//! `flick ffi import` and `flick ffi check`: the `.d.ts` of an installed
//! package read by the Node helper `@flick-lang/dts-importer`, turned into
//! `<out>/<Pkg>/Generated.fl` and checked against reviewed bindings.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::{env, fs};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::ffi::check::{self, Reviewed};
use flick_core::ffi::import::{self, Status};
use flick_core::ffi::{Policy, Surface, module_part};
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

use crate::args::{FfiCheckArguments, FfiImportArguments};
use crate::command::{Command, Outcome};
use crate::sources::{self, FsHost};

const GENERATED: &str = "Generated.fl";
const POLICY: &str = "policy.toml";

fn error(message: String) -> Diagnostic {
    Diagnostic::new(codes::F008, Severity::Error, message)
}

/// The project, if any, and the directory packages are resolved from.
fn context(
    explicit: Option<&str>,
    sources: &mut SourceMap,
) -> Result<(Option<Project>, PathBuf), Diagnostic> {
    let host = FsHost::new();
    let project = project::discover(&host, sources, &host.cwd, explicit.map(Path::new))?;
    let root = project
        .as_ref()
        .map_or(host.cwd.clone(), |p| p.root.clone());
    Ok((project, root))
}

/// Runs the helper: `FLICK_DTS_IMPORTER`, or the one installed in the project.
fn read_surface(root: &Path, package: &str) -> Result<Surface, Diagnostic> {
    let helper = env::var_os("FLICK_DTS_IMPORTER").map_or_else(
        || root.join("node_modules/@flick-lang/dts-importer/index.mjs"),
        PathBuf::from,
    );
    if !helper.is_file() {
        return Err(
            error(format!("no `.d.ts` reader at `{}`", helper.display())).with_fix(
                "install it: `pnpm add -D @flick-lang/dts-importer`",
                Vec::new(),
            ),
        );
    }
    let request = serde_json::json!({ "root": root, "package": package }).to_string();
    let mut child = Process::new("node")
        .arg(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| error(format!("cannot run `node`: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(request.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| error(format!("the `.d.ts` reader failed: {e}")))?;
    if !output.status.success() {
        return Err(error(format!(
            "cannot read the declarations of `{package}`: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Surface::parse(&String::from_utf8_lossy(&output.stdout)).map_err(error)
}

fn read_policy(dir: &Path) -> Result<Policy, Diagnostic> {
    let path = dir.join(POLICY);
    match fs::read_to_string(&path) {
        Ok(text) => Policy::parse(&text).map_err(|e| error(format!("`{}`: {e}", path.display()))),
        Err(_) => Ok(Policy::default()),
    }
}

/// Names in `policy` the package does not export.
fn unknown_policies(policy: &Policy, surface: &Surface, dir: &Path) -> Vec<Diagnostic> {
    policy
        .exports
        .keys()
        .filter(|name| surface.find(name).is_none())
        .map(|name| {
            error(format!(
                "`{}` sets a policy for `{name}`, which `{}` does not export",
                dir.join(POLICY).display(),
                surface.provenance()
            ))
        })
        .collect()
}

/// The module of `dir`'s files: `src/Ffi` of a project with source root
/// `src` → `Ffi`; empty outside the source root.
fn module_prefix(project: Option<&Project>, dir: &Path) -> Result<String, Diagnostic> {
    let Some(relative) = project.and_then(|p| dir.strip_prefix(p.source_root()).ok()) else {
        return Ok(String::new());
    };
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let name = parts.join(".");
    if !name.is_empty() && !project::is_module_name(&name) {
        return Err(error(format!(
            "`{}` is no module directory: each part must be a capitalized name",
            dir.display()
        )));
    }
    Ok(name)
}

pub struct FfiImportCommand {
    options: FfiImportArguments,
    project: Option<String>,
}

impl FfiImportCommand {
    pub fn new(options: FfiImportArguments, project: Option<String>) -> Self {
        Self { options, project }
    }

    fn import(&self, sources: &mut SourceMap) -> Result<Vec<Diagnostic>, Diagnostic> {
        let (project, root) = context(self.project.as_deref(), sources)?;
        let package = &self.options.package;
        let out = project::normalize(
            &env::current_dir()
                .unwrap_or_default()
                .join(&self.options.out),
        );
        let dir = out.join(module_part(package));
        let prefix = module_prefix(project.as_ref(), &out)?;
        let module = [prefix.as_str(), &module_part(package), "Generated"]
            .iter()
            .filter(|p| !p.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(".");
        let surface = read_surface(&root, package)?;
        let policy = read_policy(&dir)?;
        let select = self.options.select.as_deref();
        let generated = import::generate(&surface, &policy, select, &module);
        let path = dir.join(GENERATED);
        fs::create_dir_all(&dir)
            .and_then(|()| fs::write(&path, &generated.text))
            .map_err(|e| error(format!("cannot write `{}`: {e}", path.display())))?;
        print!("{}", generated.report(&surface));
        let mut diagnostics = unknown_policies(&policy, &surface, &dir);
        for name in &generated.missing {
            diagnostics.push(error(format!(
                "`{}` does not export `{name}`",
                surface.provenance()
            )));
        }
        // asking for a blocked export by name is an error; the others stay usable
        if select.is_some() {
            for entry in generated
                .entries
                .iter()
                .filter(|e| e.status != Status::Supported)
            {
                diagnostics.push(Diagnostic::new(
                    codes::F005,
                    Severity::Error,
                    format!(
                        "`{}` is blocked: {}: {}",
                        entry.export,
                        entry.status.label(),
                        entry.detail
                    ),
                ));
            }
        }
        Ok(diagnostics)
    }
}

impl Command for FfiImportCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let diagnostics = self.import(&mut sources).unwrap_or_else(|d| vec![d]);
        Outcome {
            sources,
            diagnostics,
        }
    }
}

pub struct FfiCheckCommand {
    options: FfiCheckArguments,
    project: Option<String>,
}

impl FfiCheckCommand {
    pub fn new(options: FfiCheckArguments, project: Option<String>) -> Self {
        Self { options, project }
    }

    fn check(&self, sources: &mut SourceMap) -> Result<Vec<Diagnostic>, Diagnostic> {
        let (project, root) = context(self.project.as_deref(), sources)?;
        let mut files = Vec::new();
        if self.options.paths.is_empty() {
            let Some(project) = &project else {
                return Err(project::no_project(format!(
                    "no `{}` in the working directory or any parent",
                    project::CONFIG_FILE
                )));
            };
            sources::source_files(&project.source_root().to_string_lossy(), &mut files);
        } else {
            for path in &self.options.paths {
                sources::source_files(path, &mut files);
            }
        }
        let mut diagnostics = Vec::new();
        let mut generated = Vec::new();
        let mut reviewed: Vec<Reviewed> = Vec::new();
        for path in &files {
            let file = match sources::load(sources, path) {
                Ok(file) => file,
                Err(d) => {
                    diagnostics.push(d);
                    continue;
                }
            };
            let text = sources.get(file).text.clone();
            match check::header(&text) {
                Some(header) if path.ends_with(GENERATED) => {
                    generated.push((file, path, header, text));
                }
                _ => reviewed.extend(check::reviewed(&text, file)),
            }
        }
        let mut surfaces: BTreeMap<String, Surface> = BTreeMap::new();
        let packages = generated
            .iter()
            .map(|(_, _, h, _)| h.package.clone())
            .chain(reviewed.iter().map(|r| r.package.clone()));
        for package in packages {
            if !surfaces.contains_key(&package) {
                surfaces.insert(package.clone(), read_surface(&root, &package)?);
            }
        }
        for (file, path, header, text) in &generated {
            let surface = &surfaces[&header.package];
            let dir = Path::new(path).parent().unwrap_or(Path::new("."));
            let policy = read_policy(dir)?;
            diagnostics.extend(unknown_policies(&policy, surface, dir));
            let module = text
                .lines()
                .find_map(|l| l.strip_prefix("module "))
                .and_then(|l| l.split_whitespace().next())
                .unwrap_or_default();
            let fresh = import::generate(surface, &policy, header.select.as_deref(), module);
            if &fresh.text != text {
                diagnostics.push(check::stale(*file, path, surface));
            }
        }
        for surface in surfaces.values() {
            diagnostics.extend(check::check_reviewed(&reviewed, surface));
        }
        Ok(diagnostics)
    }
}

impl Command for FfiCheckCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let diagnostics = self.check(&mut sources).unwrap_or_else(|d| vec![d]);
        Outcome {
            sources,
            diagnostics,
        }
    }
}
