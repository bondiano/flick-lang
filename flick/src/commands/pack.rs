//! `flick pack`: a release build of the project's modules into a staging
//! package, checked for completeness, and an npm tarball of it made by
//! pnpm. Every module has its ESM and `.iface`, with `.d.ts` when the
//! project emits them; each interface's dependencies have the fingerprints
//! it was built against; every path the manifest's `exports` names exists.
//! Importing a module runs nothing, as its top level is pure; an app also
//! packs the runner of each entry, which the manifest's `bin` names.

use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::{env, fs};

use flick_core::build::{BUILTIN_DIR, runner_file};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface;
use flick_core::project::cache;
use flick_core::project::config::MAIN_ENTRY;
use flick_core::source::SourceMap;

use super::build::{BuildCommand, write};
use super::diff;
use super::run::find_project;
use crate::args::{
    BuildArguments, EmitArgument, GlobalArguments, OutDirArguments, ProfileArgument,
};
use crate::cache::files_under;
use crate::child;
use crate::command::{Command, Outcome};
use crate::report;

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
            schema_aot: false,
            profile: Some(ProfileArgument::Release),
            labels: None,
            watch: false,
        },
        project: Some(root.to_string_lossy().into_owned()),
        // an app packs its runners, a library none
        entry: None,
        generation: false,
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
    let problems = incomplete(&stage.join("dist"), dts);
    if !problems.is_empty() {
        return Outcome {
            sources: SourceMap::default(),
            diagnostics: problems,
        };
    }
    let manifest = match staged_manifest(root, &runners) {
        Ok(m) => m,
        Err(outcome) => return outcome,
    };
    if let Some(missing) = missing_exports(&manifest, &stage) {
        return failure(codes::P020, missing);
    }
    if let Err(err) = write(&stage.join("package.json"), &format!("{manifest:#}\n")) {
        return failure(
            codes::P013,
            format!("cannot write the staged `package.json`: {err}"),
        );
    }
    let out = global_path(&options.out_dir);
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
        .current_dir(&stage);
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

/// The `todo`s and `echo`s a package must not ship, as errors.
fn debugging_left(built: &[Diagnostic]) -> Option<Vec<Diagnostic>> {
    let mut diagnostics: Vec<Diagnostic> = built
        .iter()
        .filter(|d| [codes::T022, codes::T023].contains(&d.code))
        .map(|d| Diagnostic {
            severity: Severity::Error,
            ..d.clone()
        })
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

/// Modules without their `.iface` or `.d.ts`, and interfaces whose
/// dependencies in the package have other fingerprints.
fn incomplete(dist: &Path, dts: bool) -> Vec<Diagnostic> {
    let mut files = Vec::new();
    files_under(dist, &mut files);
    let mut out = Vec::new();
    let own = |p: &Path| !p.starts_with(dist.join(BUILTIN_DIR));
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
    object.insert("flick".into(), serde_json::json!({ "iface-dir": "dist" }));
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
