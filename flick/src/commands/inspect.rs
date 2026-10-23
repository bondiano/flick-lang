//! `flick inspect`, `flick doc` and `flick schema export`: tools over the
//! checked project that run none of its code, except that a schema export
//! loads the module that makes the codec, whose top level is pure.

use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::Command as Process;

use flick_core::build::Extras;
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::project::{self, Compilation, Project, cache};
use flick_core::source::SourceMap;
use flick_core::{doc, ir, syntax};

use super::build::{Output, artifacts, emit, write};
use super::check::select;
use crate::args::{InspectArguments, OutDirArguments, Representation, SchemaExportArguments};
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

fn output(dir: PathBuf, typescript: bool) -> Output {
    Output {
        out_dir: dir,
        entry: None,
        typescript,
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
        other => {
            let (file, typescript) = match other {
                Representation::Js => (format!("{path}.js"), false),
                Representation::Ts => (format!("{path}.ts"), true),
                Representation::Iface => (format!("{path}.iface"), false),
                _ => (format!("{path}.js.map"), false),
            };
            let out = output(checked.host.cwd.join("dist"), typescript);
            let (built, diagnostics) = artifacts(
                &checked.host,
                &checked.sources,
                checked.project.as_ref(),
                &checked.compilation,
                &out,
            );
            match built.into_iter().find(|a| a.path == file) {
                Some(a) => a.contents,
                None => {
                    return Outcome {
                        sources: checked.sources,
                        diagnostics,
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

pub fn doc(options: &OutDirArguments, explicit: Option<&str>) -> Outcome {
    let checked = match compile(explicit) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    let out = checked.host.cwd.join(&options.out_dir);
    let mut index = String::from("# API\n\n");
    for module in checked.compilation.modules.iter().filter(|m| !m.test) {
        let page = doc::page(&module.interface, &checked.sources.get(module.file).text);
        let file = format!("{}.md", module.name);
        if let Err(err) = write(&out.join(&file), &page) {
            return failure(
                checked.sources,
                codes::P013,
                format!("cannot write `{file}`: {err}"),
            );
        }
        let _ = writeln!(index, "- [{}]({file})", module.name);
    }
    if let Err(err) = write(&out.join("index.md"), &index) {
        return failure(
            checked.sources,
            codes::P013,
            format!("cannot write `index.md`: {err}"),
        );
    }
    Outcome::default()
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
    let (problems, _) = emit(
        &checked.host,
        &checked.sources,
        checked.project.as_ref(),
        &checked.compilation,
        &output(stage.clone(), false),
    );
    if !problems.is_empty() {
        return Outcome {
            sources: checked.sources,
            diagnostics: problems,
        };
    }
    let script = format!(
        "import {{ codecJsonSchema }} from \"@flick-lang/runtime\";\nimport {{ {value} as codec }} from \"./{}.js\";\nconst r = codecJsonSchema({value:?}, codec);\nif (r._tag === \"Success\") process.stdout.write(r.success); else {{ process.stderr.write(JSON.stringify(r.failure._0)); process.exitCode = 1; }}\n",
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
                "`{}` has no strict JSON Schema: {}",
                options.codec,
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
