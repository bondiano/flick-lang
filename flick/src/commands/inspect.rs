//! `flick inspect`, `flick doc` and `flick schema export`: tools over the
//! checked project that run none of its code, except that a schema export
//! loads the module that makes the codec, whose top level is pure.

mod doc;
pub use doc::doc;
mod doc_run;
mod example_host;

use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};
use std::{fs, io, mem};

use flick_core::build::Extras;
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface::{self, Export, Interface};
use flick_core::project::config::Backend;
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host, Project, cache};
use flick_core::projection::Projected;
use flick_core::source::{FileId, SourceMap};
use flick_core::types::{
    builtin_sources, builtins, builtins_of, decl_param_names, format_interface_summary,
};
use flick_core::{ir, projection, syntax};

use super::build::{BuildPlan, Purpose, artifacts, emit, write};
use super::check::select;
use crate::args::{InspectArguments, Representation, SchemaExportArguments, SchemaTarget};
use crate::backends::Selected;
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
    compile_with_extra(explicit, &[])
}

fn compile_with_extra(explicit: Option<&str>, extra: &[PathBuf]) -> Result<Checked, Outcome> {
    let host = FsHost::new();
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
    let rt = project::stages_by_class(project.as_ref(), false);
    if let Err(diagnostic) = builtin_cache::prepare_in(rt) {
        return Err(Outcome {
            sources,
            diagnostics: vec![diagnostic],
        });
    }
    let (diagnostics, compilation) =
        package_cache::compile(&host, &mut sources, project.as_ref(), &roots, rt, None);
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

fn output(checked: &Checked, dir: PathBuf, typescript: bool) -> Result<BuildPlan, Diagnostic> {
    let rt = checked
        .project
        .as_ref()
        .is_some_and(|p| p.config.build.effect == Backend::Rt);
    let backend = Selected::load(&checked.host, checked.project.as_ref(), false, rt)?;
    Ok(BuildPlan {
        purpose: Purpose::Build,
        backend,
        out_dir: dir,
        entries: Vec::new(),
        typescript,
        go: false,
        suites: Vec::new(),
        extras: Extras {
            rt,
            ..Extras::default()
        },
        release: false,
    })
}

pub fn inspect(options: &InspectArguments, explicit: Option<&str>) -> Outcome {
    let mut checked = match compile(explicit) {
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
            checked = match compile_with_extra(explicit, &[path]) {
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
            print_representation(&text);
            return Outcome::default();
        }
        return failure(
            checked.sources,
            codes::P018,
            format!("the project has no module `{}`", options.module),
        );
    };
    let text = match inspect_module(&mut checked, index, options, explicit) {
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

fn inspect_builtin(checked: &Checked, options: &InspectArguments) -> Option<String> {
    if !matches!(
        options.emit,
        Representation::Iface | Representation::Variance
    ) {
        return None;
    }
    let rt = project::stages_by_class(checked.project.as_ref(), false);
    let builtin = builtins_of(rt)
        .iter()
        .find(|i| i.module == options.module && !i.private)?;
    Some(match options.emit {
        Representation::Iface => iface::write(builtin),
        Representation::Variance => {
            format_interface_summary(builtin, &|export| builtin_param_names(export))
        }
        _ => unreachable!(),
    })
}

fn inspect_module(
    checked: &mut Checked,
    index: usize,
    options: &InspectArguments,
    explicit: Option<&str>,
) -> Result<String, Outcome> {
    let module = &checked.compilation.modules[index];
    let path = module.name.replace('.', "/");
    match options.emit {
        Representation::Ast => {
            let source = &checked.sources.get(module.file).text;
            Ok(syntax::parse(source, module.file).tree.debug_dump(source))
        }
        Representation::TypedIr => Ok(ir::print(&module.ir)),
        Representation::Variance => Ok(format_interface_summary(&module.interface, &|export| {
            compiled_param_names(checked, export).or_else(|| builtin_param_names(export))
        })),
        Representation::Mapping => projected(explicit, &options.module).ok_or_else(|| {
            failure(
                mem::take(&mut checked.sources),
                codes::P018,
                format!("no projection of `{}`", options.module),
            )
        }),
        other => {
            let (file, typescript) = match other {
                Representation::Js => (format!("{path}.js"), false),
                Representation::Ts => (format!("{path}.ts"), true),
                _ => (format!("{path}.iface"), false),
            };
            let out = output(checked, checked.host.cwd.join("dist"), typescript).map_err(|d| {
                Outcome {
                    sources: mem::take(&mut checked.sources),
                    diagnostics: vec![d],
                }
            })?;
            let emission = artifacts(
                &checked.host,
                &mut checked.sources,
                checked.project.as_ref(),
                &checked.compilation,
                &out,
            );
            match emission.artifacts.into_iter().find(|a| a.path == file) {
                Some(a) => String::from_utf8(a.contents).map_err(|_| {
                    failure(
                        mem::take(&mut checked.sources),
                        codes::P013,
                        format!("the emitted representation `{file}` is not UTF-8"),
                    )
                }),
                None => Err(Outcome {
                    sources: mem::take(&mut checked.sources),
                    diagnostics: emission.diagnostics,
                }),
            }
        }
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
fn projected(explicit: Option<&str>, module: &str) -> Option<String> {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (project, roots) = select(&host, &mut sources, explicit, &[]).ok()?;
    let analysis = project::analyze(&host, &mut sources, project.as_ref(), &roots);
    let analyzed = analysis.module(module)?;
    let root = project.as_ref().map(|p| p.root.as_path());
    let text = &sources.get(analyzed.file).text;
    let specifier = |m: &str| projection::relative(module, m);
    let projected = projection_of(&host, &analysis, root, analyzed, text, &specifier);
    serde_json::to_string_pretty(&projected).ok()
}

/// The projection of `module` of `analysis`, whose project is at `root`:
/// the modules it refers to by their interfaces, its imports at
/// `specifier`'s paths.
pub fn projection_of(
    host: &impl Host,
    analysis: &Analysis,
    root: Option<&Path>,
    module: &Analyzed,
    text: &str,
    specifier: &dyn Fn(&str) -> String,
) -> Projected {
    let fingerprint = project::analysis_fingerprint(&analysis.compilation);
    let lookup = |m: &str| interface_of(analysis, m);
    let package = |m: &str| {
        let own = analysis.module(m).is_some();
        let name = root.and_then(|r| project::project_name(host, r));
        own.then_some(name).flatten()
    };
    projection::project(module, text, &fingerprint, &lookup, &package, specifier)
}

/// The interface of `module`: the analysis's own or a built-in.
pub fn interface_of(analysis: &Analysis, module: &str) -> Option<Interface> {
    analysis
        .module(module)
        .map(|a| a.interface.clone())
        .or_else(|| builtins().iter().find(|i| i.module == module).cloned())
}

fn schema_exporter(target: SchemaTarget, value: &str, backend: Backend) -> String {
    match target {
        SchemaTarget::JsonSchema => {
            let runtime = if backend == Backend::Rt {
                "@flick-lang/runtime/rt"
            } else {
                "@flick-lang/effect-ts"
            };
            format!(
                "import {{ codecJsonSchema }} from \"{runtime}\";\nconst r = codecJsonSchema({value:?}, exported);"
            )
        }
        SchemaTarget::OpenApi => "import { httpApiOpenApi } from \"@flick-lang/effect-ts/effect/http-api/HttpApi\";\nconst r = httpApiOpenApi(exported);".to_owned(),
    }
}

pub fn schema_export(options: &SchemaExportArguments, explicit: Option<&str>) -> Outcome {
    let mut checked = match compile(explicit) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    let Some((module, value)) = options.codec.rsplit_once('.') else {
        return failure(
            checked.sources,
            codes::P018,
            format!("`{}` is not `Module.value`", options.codec),
        );
    };
    let found = checked
        .compilation
        .modules
        .iter()
        .find(|m| m.name == module && m.interface.exports_value(value));
    if found.is_none() {
        return failure(
            checked.sources,
            codes::P018,
            format!("no module `{module}` exports `{value}`"),
        );
    }
    let root = checked
        .project
        .as_ref()
        .map_or_else(|| checked.host.cwd.clone(), |p| p.root.clone());
    let stage = root.join(cache::DIR).join("schema");
    let plan = match output(&checked, stage.clone(), false) {
        Ok(plan) => plan,
        Err(diagnostic) => {
            return Outcome {
                sources: checked.sources,
                diagnostics: vec![diagnostic],
            };
        }
    };
    let _ = fs::remove_dir_all(&stage);
    let emission = emit(
        &checked.host,
        &mut checked.sources,
        checked.project.as_ref(),
        &checked.compilation,
        &plan,
    );
    if !emission.diagnostics.is_empty() {
        return Outcome {
            sources: checked.sources,
            diagnostics: emission.diagnostics,
        };
    }
    let backend = checked
        .project
        .as_ref()
        .map_or(Backend::Effect, |p| p.config.build.effect);
    let export = schema_exporter(options.target, value, backend);
    let script = format!(
        "{export}\nimport {{ {value} as exported }} from \"./{}.js\";\nif (r._tag === \"Success\") process.stdout.write(r.success); else {{ process.stderr.write(JSON.stringify(r.failure._0)); process.exitCode = 1; }}\n",
        module.replace('.', "/")
    );
    let runner = stage.join("export.mjs");
    if let Err(err) = write(&runner, &script) {
        return failure(
            checked.sources,
            codes::P013,
            format!("cannot write the exporter: {err}"),
        );
    }
    let result = Process::new("node")
        .arg(&runner)
        .current_dir(&stage)
        .output();
    let _ = fs::remove_dir_all(&stage);
    save_schema(checked, options, result)
}

fn save_schema(
    checked: Checked,
    options: &SchemaExportArguments,
    result: io::Result<Output>,
) -> Outcome {
    match result {
        Ok(out) if out.status.success() => {
            let path = checked.host.cwd.join(&options.out);
            match write(&path, &out.stdout) {
                Ok(()) => Outcome::default(),
                Err(err) => failure(
                    checked.sources,
                    codes::P013,
                    format!("cannot write `{}`: {err}", options.out),
                ),
            }
        }
        Ok(out) => failure(
            checked.sources,
            codes::D008,
            format!(
                "`{}` has no strict {}: {}",
                options.codec,
                match options.target {
                    SchemaTarget::JsonSchema => "JSON Schema",
                    SchemaTarget::OpenApi => "OpenAPI document",
                },
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ),
        Err(err) => failure(
            checked.sources,
            codes::P015,
            format!("cannot run `node`: {err}"),
        ),
    }
}
