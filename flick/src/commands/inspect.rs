//! `flick inspect`, `flick doc` and `flick schema export`: tools over the
//! checked project that run none of its code, except that a schema export
//! loads the module that makes the codec, whose top level is pure.

mod doc;
pub use doc::doc;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use flick_core::build::Extras;
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::project::config::Backend;
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host, Project, cache};
use flick_core::projection::Projected;
use flick_core::source::SourceMap;
use flick_core::types::builtins;
use flick_core::{ir, projection, syntax};

use super::build::{BuildPlan, artifacts, emit, write};
use super::check::select;
use crate::args::{InspectArguments, Representation, SchemaExportArguments, SchemaTarget};
use crate::command::Outcome;
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
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (project, roots) = match select(&host, &mut sources, explicit, &[]) {
        Ok(selected) => selected,
        Err(d) => {
            return Err(Outcome {
                sources,
                diagnostics: vec![d],
            });
        }
    };
    let (diagnostics, compilation) =
        project::compile(&host, &mut sources, project.as_ref(), &roots);
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

fn output(dir: PathBuf, typescript: bool) -> BuildPlan {
    BuildPlan {
        out_dir: dir,
        entries: Vec::new(),
        typescript,
        go: false,
        suites: Vec::new(),
        extras: Extras::default(),
        release: false,
    }
}

pub fn inspect(options: &InspectArguments, explicit: Option<&str>) -> Outcome {
    let checked = match compile(explicit) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    let Some(module) = checked
        .compilation
        .modules
        .iter()
        .find(|m| m.name == options.module)
    else {
        return failure(
            checked.sources,
            codes::P018,
            format!("the project has no module `{}`", options.module),
        );
    };
    let path = module.name.replace('.', "/");
    let text = match options.emit {
        Representation::Ast => {
            let source = &checked.sources.get(module.file).text;
            syntax::parse(source, module.file).tree.debug_dump(source)
        }
        Representation::TypedIr => ir::print(&module.ir),
        Representation::Mapping => match projected(explicit, &options.module) {
            Some(json) => json,
            None => {
                return failure(
                    checked.sources,
                    codes::P018,
                    format!("no projection of `{}`", options.module),
                );
            }
        },
        other => {
            let (file, typescript) = match other {
                Representation::Js => (format!("{path}.js"), false),
                Representation::Ts => (format!("{path}.ts"), true),
                _ => (format!("{path}.iface"), false),
            };
            let out = output(checked.host.cwd.join("dist"), typescript);
            let emission = artifacts(
                &checked.host,
                &checked.sources,
                checked.project.as_ref(),
                &checked.compilation,
                &out,
            );
            match emission.artifacts.into_iter().find(|a| a.path == file) {
                Some(a) => a.contents,
                None => {
                    return Outcome {
                        sources: checked.sources,
                        diagnostics: emission.diagnostics,
                    };
                }
            }
        }
    };
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Outcome::default()
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
    let checked = match compile(explicit) {
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
    let _ = fs::remove_dir_all(&stage);
    let emission = emit(
        &checked.host,
        &checked.sources,
        checked.project.as_ref(),
        &checked.compilation,
        &output(stage.clone(), false),
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
    match result {
        Ok(out) if out.status.success() => {
            let path = checked.host.cwd.join(&options.out);
            match write(&path, &String::from_utf8_lossy(&out.stdout)) {
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
