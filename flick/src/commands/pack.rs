//! `flick pack`: a release build of the project, which must have no `todo`
//! or `echo` left. A library with `[package]` packs its sources, which
//! dependents fetch and compile (see `tarball`); an app checks completeness
//! and delegates packaging to the selected backend's `pack` command. Each
//! public module has its emitted module and `.iface`, with `.d.ts` when the
//! project emits them; its interface owns any alternate backend implementation.
//! Dependencies have the recorded fingerprints and manifest exports exist.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

use flick_core::build::BUILTIN_DIR;
#[cfg(feature = "wasm-backends")]
use flick_core::build::runner_file;
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface;
use flick_core::implicit_packages::is_implicit_package;
use flick_core::project::cache;
use flick_core::source::SourceMap;

use super::build::BuildCommand;
#[cfg(test)]
use super::build::write;
#[cfg(feature = "wasm-backends")]
use super::contributed;
use super::diff;
use super::run::find_project;
use super::tarball;
use crate::args::{BuildArguments, GlobalArguments, OutDirArguments, ProfileArgument};
use crate::backends::options;
use crate::cache::files_under;
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
    let dts = project.config.js_flag("declarations");
    let mut backend_options = BTreeMap::new();
    backend_options.insert("typescript".into(), vec!["false".into()]);
    if dts {
        backend_options.insert("declarations".into(), vec!["true".into()]);
    }
    if project.config.js_flag("promise-facade") {
        backend_options.insert("promise-facade".into(), vec!["true".into()]);
    }
    let build = BuildCommand {
        options: BuildArguments {
            paths: Vec::new(),
            target: Some("js".into()),
            out_dir: Some(stage.join("dist").to_string_lossy().into_owned()),
            emit_ir: None,
            profile: Some(ProfileArgument::Release),
            labels: None,
            watch: false,
            inputs: false,
            backend_options,
        },
        project: Some(root.to_string_lossy().into_owned()),
        display: None,
        // an app packs its runners, a library none
        entry: None,
        generation: false,
        dev: false,
        built: RefCell::default(),
        modules: None,
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
    // a warning, not a refusal: the version bump is the author's call
    if let Some(tag) = diff::latest_tag(root)
        && let Ok(changes) = diff::changes_since(&project, &tag)
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

/// Packs the built app staged in `stage` through the project's backend, then
/// moves the produced tarball(s) into `out_dir`.
fn app(
    project: &Project,
    stage: &Path,
    dts: bool,
    runners: &[(String, String)],
    out_dir: &str,
    global: &GlobalArguments,
) -> Outcome {
    let problems = incomplete(&stage.join("dist"), dts);
    if !problems.is_empty() {
        return Outcome {
            sources: SourceMap::default(),
            diagnostics: problems,
        };
    }
    #[cfg(feature = "wasm-backends")]
    {
        use flick_backend::command::Argument;

        let mut staged_files = Vec::new();
        files_under(stage, &mut staged_files);
        let file_paths: Vec<String> = staged_files
            .into_iter()
            .filter_map(|p| {
                Some(
                    p.strip_prefix(stage)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            })
            .collect();
        let mut arguments = Vec::new();
        if !runners.is_empty() {
            arguments.push(Argument {
                name: "runner".into(),
                values: runners
                    .iter()
                    .map(|(name, _)| format!("{name}={}", runner_file(name, false)))
                    .collect(),
            });
        }
        if !file_paths.is_empty() {
            arguments.push(Argument {
                name: "file".into(),
                values: file_paths,
            });
        }
        let Some(ran) = contributed::delegate(project, "pack", arguments, Vec::new(), global)
        else {
            let backend = options::resolve_target(None, Some(project)).map_or_else(
                |_| project.config.default_target().to_owned(),
                |(choice, _)| choice.id().to_owned(),
            );
            return failure(
                codes::P063,
                format!("the backend `{backend}` has no `pack` command for apps"),
            );
        };
        if ran
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return Outcome {
                sources: ran.sources,
                diagnostics: ran.diagnostics,
            };
        }
        if let Some(failed) = move_tarballs(stage, out_dir) {
            return failed;
        }
        Outcome {
            sources: ran.sources,
            diagnostics: ran.diagnostics,
        }
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = (
            project,
            stage,
            runners,
            out_dir,
            global,
            options::short_name,
        );
        failure(
            codes::P063,
            "this build has no backend component to pack apps".into(),
        )
    }
}

#[cfg(feature = "wasm-backends")]
fn move_tarballs(stage: &Path, out_dir: &str) -> Option<Outcome> {
    let out = global_path(out_dir);
    if let Err(err) = fs::create_dir_all(&out) {
        return Some(failure(
            codes::P013,
            format!("cannot create `{}`: {err}", out.display()),
        ));
    }
    let mut moved = false;
    if let Ok(entries) = fs::read_dir(stage) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "tgz")
                && let Some(name) = path.file_name()
            {
                let dest = out.join(name);
                if let Err(err) = fs::rename(&path, &dest)
                    .or_else(|_| fs::copy(&path, &dest).and_then(|_| fs::remove_file(&path)))
                {
                    return Some(failure(
                        codes::P013,
                        format!(
                            "cannot move `{}` to `{}`: {err}",
                            path.display(),
                            dest.display()
                        ),
                    ));
                }
                moved = true;
            }
        }
    }
    if !moved {
        return Some(failure(
            codes::P020,
            "the backend's `pack` command produced no `.tgz` archive".into(),
        ));
    }
    None
}

/// The source tarball of the library `project`, with its sha256 on stdout.
fn library(project: &Project, out_dir: &str) -> Outcome {
    let Some(package) = &project.config.package else {
        return failure(
            codes::P032,
            "`flick pack` packs a library with `[package] name` and `version`".to_owned(),
        );
    };
    if let Some(name) = super::plugins::inherited(project).first() {
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
    let own = |p: &Path| !p.starts_with(&builtins);
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
            "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\nruntime = \"effect\"\n",
        )
        .expect("config");
        write(
            &root.join("src/Lib/Greeting.fl"),
            "module Lib.Greeting exposing (greet)\ngreet : String -> String\nlet greet name = \"Hello, \" ++ name\n",
        )
        .expect("module");
        let mut backend_options = BTreeMap::new();
        backend_options.insert("declarations".into(), vec!["true".into()]);
        let built = BuildCommand::new(
            BuildArguments {
                paths: Vec::new(),
                target: Some("js".into()),
                out_dir: None,
                emit_ir: None,
                profile: Some(ProfileArgument::Release),
                labels: None,
                inputs: false,
                watch: false,
                backend_options,
            },
            Some(root.to_string_lossy().into_owned()),
            None,
        )
        .run();
        assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
        let dist = root.join("dist");
        assert!(!dist.join("_core").exists());
        assert!(incomplete(&dist, true).is_empty());
        fs::remove_file(dist.join("Lib/Greeting.iface")).expect("remove interface");
        assert!(
            incomplete(&dist, true)
                .iter()
                .any(|d| d.code == codes::P020 && d.message.contains("Lib/Greeting.iface")),
            "a module needs its interface"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
