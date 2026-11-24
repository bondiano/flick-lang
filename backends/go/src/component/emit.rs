//! A Go module per program, with the runtime beside it, and the host step
//! that builds its executable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, BuildResult, Diagnostic, InputRef, Launch, LaunchKind,
    ModuleRole, OptionValue, Profile, Severity, Span, StepOutput, Toolchain, ToolchainStep,
};
use flick_backend_support::Known;
use flick_ir::Module;

use super::data::{Command, Data};
use super::{failure, optimize};
use crate::codegen::{
    self, GO_SUM, Input, PACKAGES, Package, RUNTIME, Unsupported, go_mod_with_deps, merge_go_sum,
};

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
    let sources: Vec<&str> = data.sources.iter().map(String::as_str).collect();
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
            sources: &sources,
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
    let primitives = release.then(|| optimize::primitives(request, data, &lang));
    let own: Vec<Module> = (data.program.modules.iter())
        .filter(|m| role(m) == Some(ModuleRole::Own))
        .map(|m| {
            if let Some(prims) = primitives.as_ref() {
                optimize::module(request, data, m, &lang, prims)
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
    let sources: Vec<&str> = data.sources.iter().map(String::as_str).collect();
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
            sources: &sources,
            foreign: &data.foreign,
            bindings: &data.bindings,
            lang: &lang,
        });
        unsupported.extend(output.diagnostics);
        add(
            &mut result,
            data,
            go,
            name,
            output.files,
            &output.packages,
            &output.go_imports,
        );
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
    go_imports: &BTreeSet<String>,
) {
    let dir = format!("go/{name}");
    let packages: Vec<&Package> = (PACKAGES.iter())
        .filter(|p| reached.contains(&p.name))
        .collect();
    let uses_go_deps = data.go_deps.keys().any(|dep| {
        let prefix = format!("{dep}/");
        go_imports
            .iter()
            .any(|imp| imp == dep || imp.starts_with(&prefix))
            || code.iter().any(|(path, text)| {
                Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("go"))
                    && (text.contains(&format!("\"{dep}\""))
                        || text.contains(&format!("\"{prefix}")))
            })
    });
    let mut files = vec![(
        "go.mod".to_owned(),
        go_mod_with_deps(&packages, &data.go_deps),
    )];
    let capture_generated_go_sum = if let Some(proj_sum) = data.project_go_sum.as_deref() {
        if let Some(merged) = merge_go_sum((!packages.is_empty()).then_some(GO_SUM), Some(proj_sum))
        {
            files.push(("go.sum".to_owned(), merged));
        }
        false
    } else if uses_go_deps {
        // `go build -mod=mod` generates `go.sum` for imported `[target.go.dependencies]`.
        true
    } else {
        if !packages.is_empty() {
            files.push(("go.sum".to_owned(), GO_SUM.to_owned()));
        }
        false
    };
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
    let mut outputs = vec![StepOutput {
        id: format!("exe:{name}"),
        path: executable,
        kind: ArtifactKind::Executable,
    }];
    if capture_generated_go_sum {
        outputs.push(StepOutput {
            id: format!("gosum:{name}"),
            path: format!("{dir}/go.sum"),
            kind: ArtifactKind::Source,
        });
    }
    // -trimpath keeps the build directory out of Go's cache keys and the binary
    result.steps.push(ToolchainStep {
        id: format!("build:{name}"),
        dependencies: vec![],
        toolchain: go.clone(),
        arguments: ["build", "-C", &dir, "-trimpath", "-buildvcs=false", "-o"]
            .into_iter()
            .map(str::to_owned)
            .chain([format!("../../{}", outputs[0].path), ".".to_owned()])
            .collect(),
        // only optional packages and declared Go modules download once
        environment: vec![
            env("GOFLAGS", "-mod=mod"),
            env("GOTOOLCHAIN", "local"),
            env("CGO_ENABLED", if data.cgo { "1" } else { "0" }),
        ],
        inputs,
        outputs,
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
        runtime_arguments: vec![],
        arguments: vec![],
        environment: vec![],
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
