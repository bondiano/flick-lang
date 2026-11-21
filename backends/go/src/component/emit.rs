//! A Go module per program, with the runtime beside it, and the host step
//! that builds its executable.

use std::collections::BTreeMap;

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, BuildResult, Diagnostic, InputRef, Launch, LaunchKind,
    ModuleRole, OptionValue, Profile, Severity, Span, StepOutput, Toolchain, ToolchainStep,
};
use flick_backend_support::Known;
use flick_ir::Module;

use super::data::{Command, Data};
use super::{failure, optimize};
use crate::codegen::{self, GO_SUM, Input, PACKAGES, Package, RUNTIME, Unsupported, go_mod};

/// The name of the program that runs a project's tests.
const TEST: &str = "test";

fn inspect(
    request: &BuildRequest,
    data: &Data,
    own: &[&Module],
    builtins: &[&Module],
    paths: &BTreeMap<String, String>,
    lines: &[&[u32]],
    lang: &Known,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let module = data
        .inspect_module
        .as_deref()
        .or(request.module_root.as_deref())
        .ok_or_else(|| failure("P012", "inspect requires a module name"))?;
    let output = codegen::go_module(
        &Input {
            own,
            builtins,
            entry: "",
            suites: &[],
            test: false,
            paths,
            lines,
            foreign: &data.foreign,
            bindings: &data.bindings,
            lang,
        },
        module,
    );
    if !output.diagnostics.is_empty() {
        return Err(output.diagnostics.into_iter().map(diagnostic).collect());
    }
    Ok(BuildResult {
        artifacts: output
            .files
            .into_iter()
            .map(|(path, contents)| Artifact {
                id: format!("go:inspect/{path}"),
                path,
                kind: ArtifactKind::Source,
                bytes: contents.into_bytes(),
            })
            .collect(),
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    })
}

pub(super) fn generate(
    request: &BuildRequest,
    data: &Data,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let lang = Known::of(&data.program.lang);
    let role = |m: &Module| {
        (request.modules.iter())
            .find(|i| i.name == m.name)
            .map(|i| i.role)
    };
    let release = request.target.profile == Profile::Release;
    let own: Vec<Module> = (data.program.modules.iter())
        .filter(|m| role(m) == Some(ModuleRole::Own))
        .map(|m| {
            if release {
                optimize::module(request, data, m, &lang)
            } else {
                m.clone()
            }
        })
        .collect();
    let own: Vec<&Module> = own.iter().collect();
    let builtins: Vec<&Module> = (data.program.modules.iter())
        .filter(|m| role(m) == Some(ModuleRole::Builtin))
        .collect();
    // a built-in module has no source of the build to name
    let paths: BTreeMap<String, String> = (own.iter())
        .filter_map(|m| Some((m.name.clone(), data.facts.get(&m.name)?.line_path.clone())))
        .collect();
    let lines: Vec<&[u32]> = data.lines.iter().map(Vec::as_slice).collect();
    if data.command == Command::Inspect {
        return inspect(request, data, &own, &builtins, &paths, &lines, &lang);
    }
    let programs: Vec<(&str, &str)> = if data.command == Command::Test {
        vec![(TEST, "")]
    } else {
        (request.entries.iter())
            .map(|e| (e.name.as_str(), e.symbol.module.as_str()))
            .collect()
    };
    if programs.is_empty() {
        return Err(failure(
            "P012",
            "the go target builds an app: there is no entry whose `main` it runs",
        ));
    }
    let Some(go) = request.toolchains.iter().find(|t| t.id == "go") else {
        return Err(failure(
            "P029",
            "the go target needs the host tool `go`: install Go 1.23 or later and allow it, with `[tools]` `\"flick/go\" = [\"go\"]` in a project or `--allow-tool go` outside one",
        ));
    };
    let suites: Vec<String> = if data.command == Command::Test {
        (request.test_roots.iter())
            .map(|s| s.module.clone())
            .collect()
    } else {
        vec![]
    };
    let mut result = BuildResult {
        artifacts: vec![],
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    let mut unsupported = Vec::new();
    for (name, entry) in programs {
        let output = codegen::go(&Input {
            own: &own,
            builtins: &builtins,
            entry,
            suites: &suites,
            test: data.command == Command::Test,
            paths: &paths,
            lines: &lines,
            foreign: &data.foreign,
            bindings: &data.bindings,
            lang: &lang,
        });
        unsupported.extend(output.diagnostics);
        add(&mut result, data, go, name, output.files, &output.packages);
    }
    if unsupported.is_empty() {
        Ok(result)
    } else {
        Err(unsupported.into_iter().map(diagnostic).collect())
    }
}

/// The Go module `go/<name>` of a program, the step that builds it and the
/// launch of its executable.
fn add(
    result: &mut BuildResult,
    data: &Data,
    go: &Toolchain,
    name: &str,
    code: Vec<(String, String)>,
    reached: &[&str],
) {
    let dir = format!("go/{name}");
    let packages: Vec<&Package> = (PACKAGES.iter())
        .filter(|p| reached.contains(&p.name))
        .collect();
    let mut files = vec![("go.mod".to_owned(), go_mod(&packages))];
    // the modules of a package go build downloads once; go.sum pins them
    if !packages.is_empty() {
        files.push(("go.sum".to_owned(), GO_SUM.to_owned()));
    }
    let package_files = packages.iter().flat_map(|p| p.files);
    files.extend(package_files.map(|(p, c)| ((*p).to_owned(), (*c).to_owned())));
    files.extend(code);
    files.extend((RUNTIME.iter()).map(|(p, c)| (format!("flickrt/{p}"), (*c).to_owned())));
    let inputs = (files.iter())
        .map(|(path, _)| InputRef::Artifact(format!("go:{dir}/{path}")))
        .collect();
    result
        .artifacts
        .extend(files.into_iter().map(|(path, contents)| Artifact {
            id: format!("go:{dir}/{path}"),
            path: format!("{dir}/{path}"),
            kind: ArtifactKind::Source,
            bytes: contents.into_bytes(),
        }));
    let executable = format!("flick-{name}{}", if data.windows { ".exe" } else { "" });
    let env = |key: &str, value: &str| OptionValue {
        key: key.into(),
        value: value.into(),
    };
    // -trimpath keeps the build directory out of Go's cache keys and the binary
    result.steps.push(ToolchainStep {
        id: format!("build:{name}"),
        dependencies: vec![],
        toolchain: go.clone(),
        arguments: ["build", "-C", &dir, "-trimpath", "-buildvcs=false", "-o"]
            .into_iter()
            .map(str::to_owned)
            .chain([format!("../../{executable}"), ".".to_owned()])
            .collect(),
        // only optional packages need modules, which go downloads once
        environment: vec![
            env("GOFLAGS", "-mod=mod"),
            env("GOTOOLCHAIN", "local"),
            env("CGO_ENABLED", if data.cgo { "1" } else { "0" }),
        ],
        inputs,
        outputs: vec![StepOutput {
            id: format!("exe:{name}"),
            path: executable,
            kind: ArtifactKind::Executable,
        }],
    });
    result.launches.push(Launch {
        name: name.to_owned(),
        kind: if data.command == Command::Test {
            LaunchKind::Test
        } else {
            LaunchKind::Run
        },
        artifact: format!("exe:{name}"),
        runtime: None,
        arguments: vec![],
    });
}

fn diagnostic(unsupported: Unsupported) -> Diagnostic {
    Diagnostic {
        code: "P012".into(),
        severity: Severity::Error,
        message: unsupported.message,
        span: unsupported.span.map(|s| Span {
            file: s.file.index(),
            start: s.start,
            end: s.end,
        }),
        notes: vec![],
    }
}
