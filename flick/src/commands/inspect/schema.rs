//! `flick schema export`: emits a temporary JS build of the codec module and
//! evaluates its pure schema exporter under Node.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command as Process, Output};
use std::{fs, io};

use flick_backend::Verb;
use flick_core::build::{Backend, Extras};
use flick_core::diagnostics::codes;
use flick_core::project::cache;
use flick_core::source::SourceMap;

use super::{Checked, InspectTarget, compile_with_extra, failure};
use crate::args::{SchemaExportArguments, SchemaTarget};
use crate::backends::Selected;
use crate::backends::options::{self, ResolvedOptions};
use crate::command::Outcome;
use crate::commands::build::{BuildPlan, Purpose, emit, write};
use crate::commands::check::select;
use crate::sources::FsHost;

fn prepare_schema_target(explicit: Option<&str>) -> Result<InspectTarget, Outcome> {
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
        Selected::load_target(&host, project.as_ref(), Some("js")).map_err(|d| Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![d],
        })?;
    let mut overrides = BTreeMap::new();
    for flag in ["typescript", "declarations", "promise-facade", "schema-aot"] {
        overrides.insert(flag.to_owned(), vec!["false".to_owned()]);
    }
    let resolved = match backend.descriptor() {
        Some(descriptor) => {
            options::resolve(&descriptor, Verb::Build, project.as_ref(), &overrides).map_err(
                |diagnostics| Outcome {
                    sources,
                    diagnostics,
                },
            )?
        }
        None => ResolvedOptions::default(),
    };
    let rt = resolved.is_rt();
    Ok(InspectTarget {
        backend,
        choice,
        target_overridden,
        resolved,
        rt,
    })
}

fn schema_output(target: InspectTarget, dir: PathBuf) -> (BuildPlan, Backend) {
    let rt = target.rt;
    let plan = BuildPlan {
        purpose: Purpose::Build,
        backend: target.backend,
        target: target.choice.short().to_owned(),
        target_overridden: target.target_overridden,
        options: target.resolved,
        inspect_module: None,
        out_dir: dir,
        entries: Vec::new(),
        typescript: false,
        go: false,
        suites: Vec::new(),
        extras: Extras {
            rt,
            ..Extras::default()
        },
        release: false,
    };
    let mode = if rt { Backend::Rt } else { Backend::Effect };
    (plan, mode)
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
    let target = match prepare_schema_target(explicit) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let mut checked = match compile_with_extra(explicit, &[], Some(target.rt)) {
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
    let (plan, backend) = schema_output(target, stage.clone());
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
