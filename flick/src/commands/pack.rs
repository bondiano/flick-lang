//! `flick pack`: a release build of the project, which must have no `todo`
//! or `echo` left. A library with `[package]` packs its sources, which
//! dependents fetch and compile (see `tarball`); an app packs a staging npm
//! package, checked for completeness, made into a tarball by pnpm. Each public module has its ESM and `.iface`, with `.d.ts` when the
//! project emits them; its interface owns any alternate backend implementation.
//! Dependencies have the recorded fingerprints and manifest exports exist.
//! Importing a module runs nothing, as its top level is pure; an app also
//! packs the runner of each entry, which the manifest's `bin` names.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::{env, fs};

use flick_core::build::{BUILTIN_DIR, effects::CORE_DIR, runner_file};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface;
use flick_core::implicit_packages::is_implicit_package;
use flick_core::project::cache;
use flick_core::project::config::MAIN_ENTRY;
use flick_core::project::{self as project_files, config};
use flick_core::source::{FileId, SourceMap};

use super::build::{BuildCommand, write};
use super::diff;
use super::run::find_project;
use super::tarball;
use crate::args::{
    BuildArguments, EmitArgument, GlobalArguments, OutDirArguments, ProfileArgument,
};
use crate::cache::files_under;
use crate::child;
use crate::command::{Command, Outcome};
use crate::report;
use flick_core::project::Project;
use flick_core::project::config::Kind;
use flick_core::types;

fn failure(code: Code, message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
    }
}

pub fn pack(options: &OutDirArguments, global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let root = &project.root;
    // an implicit package's modules are built in, so a build of them as a
    // project's would clash with themselves: checking them as built in is the build
    if project
        .config
        .package
        .as_ref()
        .is_some_and(|p| is_implicit_package(&p.name))
    {
        let _ = types::builtins();
        return library(&project, &options.out_dir);
    }
    let stage = root.join(cache::DIR).join("pack");
    let _ = fs::remove_dir_all(&stage);
    let dts = project.config.build.emit_dts;
    let build = BuildCommand {
        options: BuildArguments {
            paths: Vec::new(),
            out_dir: Some(stage.join("dist").to_string_lossy().into_owned()),
            emit: Some(EmitArgument::Js),
            emit_dts: dts,
            promise_facade: project.config.build.promise_facade,
            emit_ir: None,
            schema_aot: false,
            profile: Some(ProfileArgument::Release),
            labels: None,
            watch: false,
            inputs: false,
        },
        project: Some(root.to_string_lossy().into_owned()),
        display: None,
        // an app packs its runners, a library none
        entry: None,
        generation: false,
        built: RefCell::default(),
    };
    let runners = build.entries(&project);
    let outcome = build.run();
    if report::code(&outcome.diagnostics) != 0 {
        return outcome;
    }
    if let Some(diagnostics) = debugging_left(&outcome.diagnostics) {
        return Outcome {
            sources: outcome.sources,
            diagnostics,
        };
    }
    report::render(global, &outcome.sources, &outcome.diagnostics);
    // a warning, not a refusal: the version in `package.json` is the author's call
    if let Some(tag) = diff::latest_tag(root)
        && let Ok(changes) = diff::changes_since(root, &tag)
        && diff::breaking(&changes)
    {
        eprintln!(
            "warning: the public API breaks importers since {tag}; `flick diff {tag}` lists the changes"
        );
    }
    if project.config.kind == Kind::Lib {
        return library(&project, &options.out_dir);
    }
    app(&project, &stage, dts, &runners, &options.out_dir, global)
}

/// The staged npm package of the app `project`, built into `stage`, as a
/// tarball by pnpm.
fn app(
    project: &Project,
    stage: &Path,
    dts: bool,
    runners: &[(String, String)],
    out_dir: &str,
    global: &GlobalArguments,
) -> Outcome {
    let root = &project.root;
    let problems = incomplete(&stage.join("dist"), dts);
    if !problems.is_empty() {
        return Outcome {
            sources: SourceMap::default(),
            diagnostics: problems,
        };
    }
    let manifest = match staged_manifest(root, runners) {
        Ok(m) => m,
        Err(outcome) => return outcome,
    };
    if let Some(missing) = missing_exports(&manifest, stage) {
        return failure(codes::P020, missing);
    }
    if let Err(err) = write(&stage.join("package.json"), &format!("{manifest:#}\n")) {
        return failure(
            codes::P013,
            format!("cannot write the staged `package.json`: {err}"),
        );
    }
    let out = global_path(out_dir);
    if let Err(err) = fs::create_dir_all(&out) {
        return failure(
            codes::P013,
            format!("cannot create `{}`: {err}", out.display()),
        );
    }
    let mut process = Process::new("pnpm");
    process
        .args(["pack", "--pack-destination"])
        .arg(&out)
        .current_dir(stage);
    match child::spawn_tool(&mut process, global, "package-manager-output")
        .and_then(|r| r.wait(&child::signals()))
    {
        Ok(s) if s.success() => Outcome::default(),
        Ok(s) => failure(
            codes::P017,
            format!("pnpm pack failed with exit code {}", child::code(s)),
        ),
        Err(err) => failure(codes::P016, format!("cannot run pnpm: {err}")),
    }
}

/// The source tarball of the library `project`, with its sha256 on stdout.
fn library(project: &Project, out_dir: &str) -> Outcome {
    let Some(package) = &project.config.package else {
        return failure(
            codes::P032,
            "`flick pack` packs a library with `[package] name` and `version`".to_owned(),
        );
    };
    if let Some(name) = inherited(project).first() {
        return failure(
            codes::P032,
            format!(
                "the library gets `{name}` from `[[workspace.plugins]]`, which its published \
                 manifest would not have: name it in `[dependencies]`"
            ),
        );
    }
    match tarball::write(
        project,
        &package.name,
        &package.version,
        &global_path(out_dir),
    ) {
        Ok((file, sha)) => {
            println!("{sha}  {}", file.display());
            Outcome::default()
        }
        Err(err) => failure(codes::P013, format!("cannot write the tarball: {err}")),
    }
}

/// The `todo`s and `echo`s a package must not ship, as errors.
fn debugging_left(built: &[Diagnostic]) -> Option<Vec<Diagnostic>> {
    let mut diagnostics: Vec<Diagnostic> = built
        .iter()
        .filter(|d| [codes::T022, codes::T023].contains(&d.code))
        .map(|d| d.clone().with_severity(Severity::Error))
        .collect();
    if diagnostics.is_empty() {
        return None;
    }
    diagnostics.push(Diagnostic::new(
        codes::P024,
        Severity::Error,
        "a package cannot have `todo` or `echo` left in it",
    ));
    Some(diagnostics)
}

fn global_path(dir: &str) -> PathBuf {
    env::current_dir().unwrap_or_default().join(dir)
}

/// Primary module companions, declared backend implementations and interface
/// dependency fingerprints must all agree.
fn incomplete(dist: &Path, dts: bool) -> Vec<Diagnostic> {
    let mut files = Vec::new();
    files_under(dist, &mut files);
    let mut out = Vec::new();
    let builtins = dist.join(BUILTIN_DIR);
    let core = dist.join(CORE_DIR);
    let own = |p: &Path| !p.starts_with(&builtins) && !p.starts_with(&core);
    for js in files
        .iter()
        .filter(|f| own(f) && f.extension().is_some_and(|e| e == "js"))
    {
        if js.to_string_lossy().ends_with(".promise.js") || js.parent() == Some(dist) {
            continue;
        }
        let mut wanted = vec!["iface"];
        if dts {
            wanted.push("d.ts");
        }
        for ext in wanted {
            let path = js.with_extension(ext);
            if !path.exists() {
                out.push(Diagnostic::new(
                    codes::P020,
                    Severity::Error,
                    format!(
                        "`{}` has no `{}`",
                        relative(js, dist),
                        relative(&path, dist)
                    ),
                ));
            }
        }
    }
    for path in files
        .iter()
        .filter(|f| own(f) && f.extension().is_some_and(|e| e == "iface"))
    {
        let module = relative(path, dist)
            .trim_end_matches(".iface")
            .replace('/', ".");
        let Some(interface) = fs::read(path)
            .ok()
            .and_then(|b| iface::read(&b, &module).ok())
        else {
            out.push(Diagnostic::new(
                codes::P020,
                Severity::Error,
                format!("`{module}` has an unreadable interface"),
            ));
            continue;
        };
        if let Some(core) = interface
            .abi
            .as_ref()
            .and_then(|abi| abi.core.as_deref())
            .filter(|core| !core.is_empty())
        {
            let implementation = dist
                .join(core)
                .join(format!("{}.js", module.replace('.', "/")));
            if !implementation.is_file() {
                out.push(Diagnostic::new(
                    codes::P020,
                    Severity::Error,
                    format!(
                        "`{module}` declares a build without Effect at `{}`, which the package does not have",
                        relative(&implementation, dist)
                    ),
                ));
            }
        }
        for (dependency, fingerprint) in &interface.dependencies {
            let dep_path = dist.join(format!("{}.iface", dependency.replace('.', "/")));
            let Some(dep) = fs::read(&dep_path)
                .ok()
                .and_then(|b| iface::read(&b, dependency).ok())
            else {
                continue;
            };
            if &iface::fingerprint(&dep) != fingerprint {
                out.push(Diagnostic::new(
                    codes::P020,
                    Severity::Error,
                    format!("`{module}` was built against another interface of `{dependency}`"),
                ));
            }
        }
    }
    out
}

fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The project's `package.json` with the fields of a Flick package; the
/// `bin` of an app names the runner of each entry, the only entry `main`
/// under the package's name without its scope.
fn staged_manifest(
    root: &Path,
    runners: &[(String, String)],
) -> Result<serde_json::Value, Outcome> {
    let text = fs::read_to_string(root.join("package.json"))
        .map_err(|err| failure(codes::P020, format!("cannot read `package.json`: {err}")))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| failure(codes::P020, format!("`package.json` is not JSON: {err}")))?;
    let Some(object) = manifest.as_object_mut() else {
        return Err(failure(
            codes::P020,
            "`package.json` is not an object".to_owned(),
        ));
    };
    object.insert("type".into(), "module".into());
    object.insert("files".into(), serde_json::json!(["dist"]));
    object
        .entry("exports")
        .or_insert(serde_json::json!({ "./*": "./dist/*" }));
    if !runners.is_empty() {
        let package = object
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default();
        let unscoped = package.rsplit('/').next().unwrap_or(package).to_owned();
        let bin: serde_json::Map<String, serde_json::Value> = runners
            .iter()
            .map(|(name, _)| {
                let command = if runners.len() == 1 && name == MAIN_ENTRY && !unscoped.is_empty() {
                    unscoped.clone()
                } else {
                    name.clone()
                };
                (
                    command,
                    format!("./dist/{}", runner_file(name, false)).into(),
                )
            })
            .collect();
        object
            .entry("bin")
            .or_insert(serde_json::Value::Object(bin));
    }
    object.remove("devDependencies");
    object.remove("scripts");
    Ok(manifest)
}

/// The first path `exports` names that the package lacks.
fn missing_exports(manifest: &serde_json::Value, stage: &Path) -> Option<String> {
    fn targets(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Object(map) => map.values().for_each(|v| targets(v, out)),
            serde_json::Value::Array(items) => items.iter().for_each(|v| targets(v, out)),
            _ => {}
        }
    }
    let mut paths = Vec::new();
    targets(manifest.get("exports")?, &mut paths);
    paths
        .into_iter()
        .find(|p| !p.contains('*') && !stage.join(p).exists())
        .map(|p| format!("`exports` names `{p}`, which the package does not have"))
}

/// The dependencies `project` has from its workspace, not from its manifest.
fn inherited(project: &Project) -> Vec<String> {
    if project.workspace.is_none() {
        return Vec::new();
    }
    let written = fs::read_to_string(project.root.join(project_files::CONFIG_FILE))
        .ok()
        .and_then(|text| config::parse(&text, FileId::default()).ok());
    let own = |name: &str| {
        (written.iter())
            .flat_map(|c| &c.dependencies)
            .any(|d| d.name == name)
    };
    (project.config.dependencies.iter())
        .filter(|d| !own(&d.name))
        .map(|d| d.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::process;

    use super::*;

    #[test]
    fn completeness_follows_primary_interfaces_and_their_backend_implementations() {
        let root = env::temp_dir().join(format!("flick-package-completeness-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
        )
        .expect("config");
        write(
            &root.join("src/Lib/Greeting.fl"),
            "module Lib.Greeting exposing (greet)\ngreet : String -> String\nlet greet name = \"Hello, \" ++ name\n",
        )
        .expect("module");
        let built = BuildCommand::new(
            BuildArguments {
                paths: Vec::new(),
                out_dir: None,
                emit: Some(EmitArgument::Js),
                emit_dts: true,
                promise_facade: false,
                emit_ir: None,
                schema_aot: false,
                profile: Some(ProfileArgument::Release),
                labels: None,
                inputs: false,
                watch: false,
            },
            Some(root.to_string_lossy().into_owned()),
            None,
        )
        .run();
        assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
        let dist = root.join("dist");
        let core = dist.join(CORE_DIR).join("Lib/Greeting.js");
        assert!(core.is_file());
        assert!(!core.with_extension("iface").exists());
        assert!(!core.with_extension("d.ts").exists());
        assert!(incomplete(&dist, true).is_empty());
        let implementation = fs::read(&core).expect("alternate implementation");
        fs::remove_file(&core).expect("remove alternate");
        assert!(
            incomplete(&dist, true)
                .iter()
                .any(|d| d.code == codes::P020 && d.message.contains("_core/Lib/Greeting.js")),
            "the authoritative interface must not advertise a missing implementation"
        );
        fs::write(&core, implementation).expect("restore alternate");
        fs::remove_file(dist.join("Lib/Greeting.iface")).expect("remove interface");
        assert!(
            incomplete(&dist, true)
                .iter()
                .any(|d| d.code == codes::P020 && d.message.contains("Lib/Greeting.iface")),
            "a backend alternative does not waive the primary interface"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
