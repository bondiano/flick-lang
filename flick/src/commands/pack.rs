//! `flick pack`: checks or builds the project in release mode, refuses
//! remaining `todo` or `echo` (`FLICK-P024`), warns on breaking public API
//! changes since the latest git tag, writes the deterministic Flick source
//! tarball when `[package]` is declared, and optionally runs the selected
//! backend's `pack` plan command to emit target-ecosystem artifacts.

mod artifacts;

use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(feature = "wasm-backends")]
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "wasm-backends")]
use std::sync::Arc;
use std::{env, fs};

use flick_backend::Verb;
#[cfg(feature = "wasm-backends")]
use flick_backend::command::{self, Argument, Style};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::implicit_packages::is_implicit_package;
use flick_core::project::config::Kind;
use flick_core::project::{self, Project, cache};
use flick_core::source::SourceMap;
use flick_core::types;

use self::artifacts::{collect_backend_artifacts, move_backend_artifacts, verify_interfaces};
use super::build::{self, BuildCommand};
#[cfg(feature = "wasm-backends")]
use super::contributed::{self, Contributor};
use super::diff;
use super::run::find_project;
use super::tarball;
use crate::args::{BuildArguments, GlobalArguments, PackArguments, ProfileArgument};
#[cfg(feature = "wasm-backends")]
use crate::backends::Package;
use crate::backends::options::{self, ResolvedOptions, TargetChoice};
use crate::backends::{self, Selected};
use crate::builtins;
#[cfg(feature = "wasm-backends")]
use crate::cache::files_under;
use crate::command::{Command, Outcome};
use crate::package_cache;
use crate::report;
use crate::sources::{self, FsHost};

#[cfg(feature = "wasm-backends")]
type BackendPack = (Arc<Package>, command::Descriptor);

fn failure(code: Code, message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
    }
}

#[allow(clippy::too_many_lines)]
pub fn pack(options: &PackArguments, global: &GlobalArguments) -> Outcome {
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

    // An implicit package's modules are built in, so building them as a user
    // project would clash with themselves: checking them as built-in is the build.
    if project
        .config
        .package
        .as_ref()
        .is_some_and(|p| is_implicit_package(&p.name))
    {
        if let Err(diagnostic) = builtins::prepare() {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![diagnostic],
            };
        }
        let _ = types::builtins();
        return implicit_tarball(&project, &options.output.out_dir);
    }

    let host = FsHost::new();
    let (backend, choice, target_overridden) =
        match load_pack_target(&host, &project, options.target.as_deref()) {
            Ok(loaded) => loaded,
            Err(outcome) => return outcome,
        };

    #[cfg(feature = "wasm-backends")]
    let backend_pack = match probe_backend_pack(&backend, &choice) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    #[cfg(not(feature = "wasm-backends"))]
    let backend_pack: Option<()> = None;

    // Phase A: compile/build (for backends with `pack`) or target-aware frontend
    // check (for backends without `pack`), enforcing FLICK-P024 before `[package]`
    // checks so compile/type/declaration errors report first.
    let stage = root.join(cache::DIR).join("pack");
    let mut entries = Vec::new();
    let mut built_launches = Vec::new();
    let mut built_options = Vec::new();

    if backend_pack.is_some() {
        let _ = fs::remove_dir_all(&stage);
        let build = BuildCommand {
            options: BuildArguments {
                paths: Vec::new(),
                target: options.target.clone(),
                out_dir: Some(stage.join("dist").to_string_lossy().into_owned()),
                emit_ir: None,
                profile: Some(ProfileArgument::Release),
                labels: None,
                watch: false,
                inputs: false,
                no_native_overrides: false,
                backend_options: BTreeMap::new(),
            },
            project: Some(root.to_string_lossy().into_owned()),
            display: None,
            entry: None,
            generation: false,
            dev: false,
            built: RefCell::default(),
            modules: None,
        };
        entries = build.entries(&project);
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
        if let Some(built) = build.built.into_inner() {
            built_launches = built.launches;
            built_options = built.options;
        }
        let problems = verify_interfaces(&stage.join("dist"));
        if !problems.is_empty() {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: problems,
            };
        }
    } else {
        let outcome = check_for_pack(&host, &project, &backend, &choice, target_overridden);
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
    }

    // Phase B: API-diff breaking-change warning (a warning, not a refusal).
    if let Some(tag) = diff::latest_tag(root)
        && let Ok(changes) = diff::changes_since(&project, &tag)
        && diff::breaking(&changes)
    {
        eprintln!(
            "warning: the public API breaks importers since {tag}; `flick diff {tag}` lists the changes"
        );
    }

    // Phase C: Core source-tarball preflight before touching `out_dir` or running
    // any backend `pack` command.
    let has_backend_pack = backend_pack.is_some();
    if let Some(failed) = preflight_package(&project, has_backend_pack) {
        return failed;
    }

    // Phase D: Optional backend `pack` delegation into `.flick/pack/out/`.
    let stage_out = stage.join("out");
    #[cfg(feature = "wasm-backends")]
    let (backend_sources, backend_diagnostics) = if let Some((package, descriptor)) = backend_pack {
        if let Err(err) = fs::create_dir_all(&stage_out) {
            return failure(
                codes::P013,
                format!("cannot create `{}`: {err}", stage_out.display()),
            );
        }
        let arguments = pack_arguments(
            &project,
            &stage,
            &stage_out,
            &entries,
            &built_launches,
            &built_options,
        );
        let ran = contributed::run_pack_command(&project, &package, &descriptor, arguments, global);
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
        if !ran.stdout.is_empty() {
            let text = String::from_utf8_lossy(&ran.stdout);
            for line in text.lines().filter(|l| !l.is_empty()) {
                report::log(global, line);
            }
        }
        (ran.sources, ran.diagnostics)
    } else {
        (SourceMap::default(), Vec::new())
    };
    #[cfg(not(feature = "wasm-backends"))]
    let (backend_sources, backend_diagnostics) = {
        let _ = (&entries, &built_launches, &built_options);
        (SourceMap::default(), Vec::new())
    };

    // Phase E: Validate staged backend artifacts (only when a backend `pack`
    // ran) and materialize outputs into `out_dir`.
    let backend_artifacts = if has_backend_pack {
        match collect_backend_artifacts(root, &stage_out) {
            Ok(files) => files,
            Err(failed) => return failed,
        }
    } else {
        Vec::new()
    };

    if project.config.package.is_none() && backend_artifacts.is_empty() {
        return failure(
            codes::P020,
            "the backend's `pack` command produced no package artifact".into(),
        );
    }

    let out = global_path(&options.output.out_dir);
    let source_written = if let Some(package) = &project.config.package {
        match tarball::write(&project, &package.name, &package.version, &out) {
            Ok(written) => Some(written),
            Err(err) => return failure(codes::P013, format!("cannot write the tarball: {err}")),
        }
    } else {
        None
    };

    if !backend_artifacts.is_empty()
        && let Some(failed) = move_backend_artifacts(
            &backend_artifacts,
            &out,
            source_written.as_ref().map(|(path, _)| path.as_path()),
            choice.short(),
            global,
        )
    {
        return failed;
    }

    if let Some((source_file, sha)) = source_written {
        println!("{sha}  {}", source_file.display());
    }

    Outcome {
        sources: backend_sources,
        diagnostics: backend_diagnostics,
    }
}

fn load_pack_target(
    host: &FsHost,
    project: &Project,
    cli_target: Option<&str>,
) -> Result<(Selected, TargetChoice, bool), Outcome> {
    #[cfg(feature = "wasm-backends")]
    {
        Selected::load_target(host, Some(project), cli_target).map_err(|diagnostic| Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        })
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = host;
        let (choice, overridden) =
            options::resolve_target(cli_target, Some(project)).map_err(|diagnostic| Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![diagnostic],
            })?;
        Ok((Selected::Native, choice, overridden))
    }
}

#[cfg(feature = "wasm-backends")]
fn probe_backend_pack(
    backend: &Selected,
    choice: &TargetChoice,
) -> Result<Option<BackendPack>, Outcome> {
    let Selected::Component(package) = backend else {
        return Ok(None);
    };
    let descriptors = package.describe().map_err(|why| {
        failure(
            codes::P013,
            format!("the commands of `{}` are unavailable: {why}", choice.id()),
        )
    })?;
    Ok(descriptors
        .into_iter()
        .find(|d| d.name == "pack" && d.style == Style::Plan)
        .map(|descriptor| (Arc::clone(package), descriptor)))
}

/// Target-aware frontend check when the selected backend declares no `pack`
/// command: checks all source modules for parsing, types, effects, regions,
/// entry signatures, `@when`/`[host]` reachability, and `todo`/`echo` without
/// invoking any host toolchain.
fn check_for_pack(
    host: &FsHost,
    project: &Project,
    backend: &Selected,
    choice: &TargetChoice,
    target_overridden: bool,
) -> Outcome {
    let mut sources = SourceMap::default();
    let descriptor = backend.descriptor();
    let resolved = match &descriptor {
        Some(descriptor) => {
            match options::resolve(descriptor, Verb::Build, Some(project), &BTreeMap::new()) {
                Ok(resolved) => resolved,
                Err(diagnostics) => {
                    return Outcome {
                        sources,
                        diagnostics,
                    };
                }
            }
        }
        None => ResolvedOptions::default(),
    };
    let stages_by_class = options::stages_by_class(descriptor.as_ref(), &resolved);
    if let Some(problem) = backends::check_runtime(project, backend, &resolved) {
        return Outcome {
            sources,
            diagnostics: vec![problem],
        };
    }
    if let Err(diagnostic) = builtins::prepare_in(stages_by_class) {
        return Outcome {
            sources,
            diagnostics: vec![diagnostic],
        };
    }
    let skip = host.skip_dirs(Some(project));
    let mut files = Vec::new();
    let source_root = project.source_root();
    if source_root.is_dir() {
        sources::source_files_skipping(&source_root.to_string_lossy(), &skip, &mut files);
    }
    let roots: Vec<PathBuf> = files
        .iter()
        .map(|f| project::normalize(&host.cwd.join(f)))
        .collect();
    let override_cfg = build::target_override(
        backend,
        choice,
        target_overridden,
        &resolved,
        stages_by_class,
        false,
        Vec::new(),
    );
    let (diagnostics, _) = package_cache::compile_for(
        host,
        &mut sources,
        Some(project),
        &roots,
        Some(&override_cfg),
        None,
    );
    Outcome {
        sources,
        diagnostics,
    }
}

fn preflight_package(project: &Project, has_backend_pack: bool) -> Option<Outcome> {
    match project.config.kind {
        Kind::Lib => {
            if project.config.package.is_none() {
                return Some(failure(
                    codes::P032,
                    "`flick pack` packs a library with `[package] name` and `version`".to_owned(),
                ));
            }
            if let Some(name) = super::plugins::inherited(project).first() {
                return Some(failure(
                    codes::P032,
                    format!(
                        "the library gets `{name}` from `[[workspace.plugins]]`, which its published \
                         manifest would not have: name it in `[dependencies]`"
                    ),
                ));
            }
        }
        Kind::App => {
            if project.config.package.is_none() && !has_backend_pack {
                return Some(failure(
                    codes::P032,
                    "`flick pack` needs `[package]` with `name` and `version` in `flick.toml` to write a Flick source tarball".to_owned(),
                ));
            }
        }
    }
    None
}

#[cfg(feature = "wasm-backends")]
fn pack_arguments(
    project: &Project,
    stage: &Path,
    stage_out: &Path,
    entries: &[(String, String)],
    launches: &[(String, String)],
    options: &[(String, String)],
) -> Vec<Argument> {
    let mut staged_files = Vec::new();
    files_under(stage, &mut staged_files);
    let mut file_paths: Vec<String> = staged_files
        .into_iter()
        .filter(|p| !p.starts_with(stage_out))
        .filter_map(|p| {
            Some(
                p.strip_prefix(stage)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
        })
        .collect();
    file_paths.sort();

    let mut arguments = vec![Argument {
        name: "kind".into(),
        values: vec![
            if project.config.kind == Kind::Lib {
                "lib"
            } else {
                "app"
            }
            .into(),
        ],
    }];
    if project.config.package.is_some() {
        arguments.push(Argument {
            name: "has-package".into(),
            values: vec!["true".into()],
        });
    }
    if !entries.is_empty() {
        arguments.push(Argument {
            name: "entry".into(),
            values: entries.iter().map(|(name, _)| name.clone()).collect(),
        });
    }
    if !launches.is_empty() {
        arguments.push(Argument {
            name: "runner".into(),
            values: launches
                .iter()
                .map(|(name, path)| format!("{name}={path}"))
                .collect(),
        });
    }
    if !file_paths.is_empty() {
        arguments.push(Argument {
            name: "file".into(),
            values: file_paths,
        });
    }
    if !options.is_empty() {
        arguments.push(Argument {
            name: "option".into(),
            values: options.iter().map(|(k, v)| format!("{k}={v}")).collect(),
        });
    }
    arguments
}

/// Writes the deterministic source tarball of an implicit package.
fn implicit_tarball(project: &Project, out_dir: &str) -> Outcome {
    let Some(package) = &project.config.package else {
        return failure(
            codes::P032,
            "`flick pack` packs a library with `[package] name` and `version`".to_owned(),
        );
    };
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
