//! `flick inspect` and `flick doc`: tools over the checked project.

mod doc;
pub use doc::doc;
mod doc_run;

use std::mem;
use std::path::PathBuf;

use flick_backend::{ArtifactKind, Verb};
use flick_core::build::{self, Extras};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface::{self, Export, Interface, Target};
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host, Project, TargetOverride};
use flick_core::projection::Projected;
use flick_core::source::{FileId, SourceMap};
use flick_core::types::{
    builtin_sources, builtins, builtins_of, decl_param_names, format_interface_summary,
};
use flick_core::{ir, projection, syntax};

use super::build::{BuildPlan, Purpose, artifacts, target_override, write};
use super::check::select;
use crate::args::{InspectArguments, Representation};
use crate::backends::Selected;
use crate::backends::options::{self, ResolvedOptions, TargetChoice};
use crate::builtins as builtin_cache;
use crate::command::Outcome;
use crate::package_cache;
use crate::sources::FsHost;

fn failure(sources: SourceMap, code: Code, message: String) -> Outcome {
    Outcome {
        sources,
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
    }
}

/// The checked and lowered project.
pub struct Checked {
    pub host: FsHost,
    pub sources: SourceMap,
    pub project: Option<Project>,
    pub compilation: Compilation,
}

pub fn compile(explicit: Option<&str>) -> Result<Checked, Outcome> {
    compile_with_extra(explicit, &[], None, true)
}

fn compile_with_extra(
    explicit: Option<&str>,
    extra: &[PathBuf],
    target: Option<&TargetOverride>,
    allow_backends: bool,
) -> Result<Checked, Outcome> {
    let host = if allow_backends {
        FsHost::new()
    } else {
        FsHost::without_backends()
    };
    let mut sources = SourceMap::default();
    let (project, mut roots) = match select(&host, &mut sources, explicit, &[]) {
        Ok(selected) => selected,
        Err(d) => {
            return Err(Outcome {
                sources,
                diagnostics: vec![d],
            });
        }
    };
    roots.extend(extra.iter().cloned());
    let rt = project::stages_by_class(&host, project.as_ref(), target.and_then(|t| t.rt));
    if let Err(diagnostic) = builtin_cache::prepare_in(rt) {
        return Err(Outcome {
            sources,
            diagnostics: vec![diagnostic],
        });
    }
    let (diagnostics, compilation) =
        package_cache::compile_for(&host, &mut sources, project.as_ref(), &roots, target, None);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err(Outcome {
            sources,
            diagnostics,
        });
    }
    Ok(Checked {
        host,
        sources,
        project,
        compilation,
    })
}

struct InspectTarget {
    backend: Selected,
    choice: TargetChoice,
    target_overridden: bool,
    resolved: ResolvedOptions,
    stages_by_class: bool,
}

fn prepare_inspect_target(
    explicit: Option<&str>,
    options: &InspectArguments,
) -> Result<InspectTarget, Outcome> {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (project, _) = match select(&host, &mut sources, explicit, &[]) {
        Ok(selected) => selected,
        Err(d) => {
            return Err(Outcome {
                sources,
                diagnostics: vec![d],
            });
        }
    };
    let (backend, choice, target_overridden) =
        Selected::load_target(&host, project.as_ref(), options.target.as_deref()).map_err(|d| {
            Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![d],
            }
        })?;
    let descriptor = backend.descriptor();
    let resolved = match &descriptor {
        Some(descriptor) => options::resolve(
            descriptor,
            Verb::Inspect,
            project.as_ref(),
            &options.backend_options,
        )
        .map_err(|diagnostics| Outcome {
            sources,
            diagnostics,
        })?,
        None => ResolvedOptions::default(),
    };
    let stages_by_class = options::stages_by_class(descriptor.as_ref(), &resolved);
    Ok(InspectTarget {
        backend,
        choice,
        target_overridden,
        resolved,
        stages_by_class,
    })
}

pub fn inspect(options: &InspectArguments, explicit: Option<&str>) -> Outcome {
    let target = if options.emit == Representation::Output {
        match prepare_inspect_target(explicit, options) {
            Ok(target) => Some(target),
            Err(outcome) => return outcome,
        }
    } else {
        None
    };
    let allow_backends = options.emit == Representation::Output;
    let override_cfg = target.as_ref().map(|t| {
        let mut cfg = target_override(
            &t.backend,
            &t.choice,
            t.target_overridden,
            &t.resolved,
            t.stages_by_class,
            false,
            Vec::new(),
        );
        cfg.defer_backend_check = true;
        cfg
    });
    let mut checked = match compile_with_extra(explicit, &[], override_cfg.as_ref(), allow_backends)
    {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    if !checked
        .compilation
        .modules
        .iter()
        .any(|m| m.name == options.module)
    {
        let rel = project::module_path(&options.module);
        if let Some(path) = checked
            .compilation
            .libraries
            .iter()
            .map(|(_, dir)| dir.join(&rel))
            .find(|path| path.is_file())
        {
            checked = match compile_with_extra(
                explicit,
                &[path],
                override_cfg.as_ref(),
                allow_backends,
            ) {
                Ok(c) => c,
                Err(outcome) => return outcome,
            };
        }
    }
    let Some(index) = checked
        .compilation
        .modules
        .iter()
        .position(|m| m.name == options.module)
    else {
        if let Some(text) = inspect_builtin(&checked, options) {
            return match text {
                Ok(text) => {
                    print_representation(&text);
                    Outcome::default()
                }
                Err(error) => failure(checked.sources, codes::P010, error),
            };
        }
        return failure(
            checked.sources,
            codes::P018,
            format!("the project has no module `{}`", options.module),
        );
    };
    let text = match inspect_module(&mut checked, index, options, explicit, target) {
        Ok(text) => text,
        Err(outcome) => return outcome,
    };
    print_representation(&text);
    Outcome::default()
}

fn print_representation(text: &str) {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
}

fn inspect_builtin(
    checked: &Checked,
    options: &InspectArguments,
) -> Option<Result<String, String>> {
    if !matches!(
        options.emit,
        Representation::Iface | Representation::Variance
    ) {
        return None;
    }
    let rt = project::stages_by_class(&checked.host, checked.project.as_ref(), None);
    let builtin = builtins_of(rt)
        .iter()
        .find(|i| i.module == options.module && !i.private)?;
    Some(match options.emit {
        Representation::Iface => iface::write(builtin),
        Representation::Variance => Ok(format_interface_summary(builtin, &|export| {
            builtin_param_names(export)
        })),
        _ => unreachable!(),
    })
}

fn inspect_module(
    checked: &mut Checked,
    index: usize,
    options: &InspectArguments,
    explicit: Option<&str>,
    target: Option<InspectTarget>,
) -> Result<String, Outcome> {
    let module = &checked.compilation.modules[index];
    let module_name = module.name.clone();
    let path = module_name.replace('.', "/");
    match options.emit {
        Representation::Ast => {
            let source = &checked.sources.get(module.file).text;
            Ok(syntax::parse(source, module.file).tree.debug_dump(source))
        }
        Representation::TypedIr => Ok(ir::print(&module.ir)),
        Representation::Variance => Ok(format_interface_summary(&module.interface, &|export| {
            compiled_param_names(checked, export).or_else(|| builtin_param_names(export))
        })),
        Representation::Mapping => projected(explicit, &options.module)
            .map_err(|error| failure(mem::take(&mut checked.sources), codes::P010, error))?
            .ok_or_else(|| {
                failure(
                    mem::take(&mut checked.sources),
                    codes::P018,
                    format!("no projection of `{}`", options.module),
                )
            }),
        Representation::Iface => {
            let package = checked
                .project
                .as_ref()
                .and_then(|p| project::project_name(&checked.host, p));
            let target_name = options
                .target
                .as_deref()
                .or_else(|| checked.project.as_ref().map(|p| p.config.default_target()))
                .unwrap_or("js");
            let filter: Option<&[Target]> = match target_name {
                "js" | "flick/js" => Some(&[Target::Js]),
                "go" | "flick/go" => Some(&[Target::Go]),
                _ => None,
            };
            let interfaces = build::published_interfaces(
                &checked.compilation,
                package.as_deref(),
                checked.project.as_ref().map(|p| p.root.as_path()),
                filter,
            )
            .map_err(|error| failure(mem::take(&mut checked.sources), codes::P010, error))?;
            iface::write(&interfaces[module_name.as_str()])
                .map_err(|error| failure(mem::take(&mut checked.sources), codes::P010, error))
        }
        Representation::Output => inspect_output(
            checked,
            module_name,
            &path,
            options,
            target.expect("inspect output target prepared"),
        ),
    }
}

fn inspect_output(
    checked: &mut Checked,
    module_name: String,
    path: &str,
    options: &InspectArguments,
    target: InspectTarget,
) -> Result<String, Outcome> {
    let typescript = target.resolved.flag("typescript");
    let labels = checked
        .project
        .as_ref()
        .map_or_else(Default::default, |p| p.config.build.labels);
    let out = BuildPlan {
        purpose: Purpose::Inspect,
        backend: target.backend,
        target: target.choice.short().to_owned(),
        target_overridden: target.target_overridden,
        options: target.resolved,
        inspect_module: Some(module_name),
        out_dir: checked.host.cwd.join("dist"),
        entries: Vec::new(),
        typescript,
        stages_by_class: target.stages_by_class,
        suites: Vec::new(),
        extras: Extras {
            rt: target.stages_by_class,
            labels,
            ..Extras::default()
        },
        release: false,
    };
    let emission = artifacts(
        &checked.host,
        &mut checked.sources,
        checked.project.as_ref(),
        &checked.compilation,
        &out,
    );
    if emission
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(Outcome {
            sources: mem::take(&mut checked.sources),
            diagnostics: emission.diagnostics,
        });
    }
    let stem = format!("{path}.");
    let primary = emission
        .artifacts
        .iter()
        .find(|a| {
            a.kind == ArtifactKind::Source
                && a.path.starts_with(&stem)
                && !a.path[stem.len()..].contains('.')
        })
        .or_else(|| {
            emission
                .artifacts
                .iter()
                .find(|a| a.kind == ArtifactKind::Source)
        });
    match primary {
        Some(a) => String::from_utf8(a.contents.clone()).map_err(|_| {
            failure(
                mem::take(&mut checked.sources),
                codes::P013,
                format!("the emitted representation `{}` is not UTF-8", a.path),
            )
        }),
        None => Err(Outcome {
            sources: mem::take(&mut checked.sources),
            diagnostics: if emission.diagnostics.is_empty() {
                vec![Diagnostic::new(
                    codes::P013,
                    Severity::Error,
                    format!(
                        "backend emitted no source artifact for `{}`",
                        options.module
                    ),
                )]
            } else {
                emission.diagnostics
            },
        }),
    }
}

fn compiled_param_names(checked: &Checked, export: &Export) -> Option<Vec<String>> {
    let origin = checked
        .compilation
        .modules
        .iter()
        .find(|m| m.name == export.entity.module)?;
    let source = &checked.sources.get(origin.file).text;
    let parsed = syntax::parse(source, origin.file);
    decl_param_names(&parsed.tree, source, &export.entity.name)
}

fn builtin_param_names(export: &Export) -> Option<Vec<String>> {
    let (_, source) = builtin_sources()
        .iter()
        .find(|(name, _)| *name == export.entity.module)?;
    let parsed = syntax::parse(source, FileId::default());
    decl_param_names(&parsed.tree, source, &export.entity.name)
}

/// The editor projection of `module` and its span maps, as JSON.
fn projected(explicit: Option<&str>, module: &str) -> Result<Option<String>, String> {
    let host = FsHost::without_backends();
    let mut sources = SourceMap::default();
    let Ok((project, roots)) = select(&host, &mut sources, explicit, &[]) else {
        return Ok(None);
    };
    let analysis = project::analyze(&host, &mut sources, project.as_ref(), &roots);
    let Some(analyzed) = analysis.module(module) else {
        return Ok(None);
    };
    let text = &sources.get(analyzed.file).text;
    let specifier = |m: &str| projection::relative(module, m);
    let projected = projection_of(
        &host,
        &analysis,
        project.as_ref(),
        analyzed,
        text,
        &specifier,
    )?;
    Ok(serde_json::to_string_pretty(&projected).ok())
}

/// The projection of `module` of `analysis`, in `project`:
/// the modules it refers to by their interfaces, its imports at
/// `specifier`'s paths.
pub fn projection_of(
    host: &impl Host,
    analysis: &Analysis,
    project: Option<&Project>,
    module: &Analyzed,
    text: &str,
    specifier: &dyn Fn(&str) -> String,
) -> Result<Projected, String> {
    let fingerprint = project::analysis_fingerprint(&analysis.compilation)?;
    let lookup = |m: &str| interface_of(analysis, m);
    let package = |m: &str| {
        let own = analysis.module(m).is_some();
        let name = project.and_then(|p| project::project_name(host, p));
        own.then_some(name).flatten()
    };
    Ok(projection::project(
        module,
        text,
        &fingerprint,
        &lookup,
        &package,
        specifier,
    ))
}

/// The interface of `module`: the analysis's own or a built-in.
pub fn interface_of(analysis: &Analysis, module: &str) -> Option<Interface> {
    analysis
        .module(module)
        .map(|a| a.interface.clone())
        .or_else(|| builtins().iter().find(|i| i.module == module).cloned())
}
