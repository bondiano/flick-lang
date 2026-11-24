//! A Go module per program, with the runtime beside it, and the host step
//! that builds its executable.

use std::collections::{BTreeMap, BTreeSet};

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
    });
    let mut files = vec![(
        "go.mod".to_owned(),
        go_mod_with_deps(&packages, &data.go_deps),
    )];
    let seed_sum = merge_go_sum(
        (!packages.is_empty()).then_some(GO_SUM),
        data.project_go_sum.as_deref(),
    );
    let capture_generated_go_sum = if let Some(sum) = seed_sum {
        files.push(("go.sum".to_owned(), sum));
        false
    } else {
        // `go build -mod=mod` generates `go.sum` when imported `[target.go.dependencies]`
        // are present and no runtime or project `go.sum` was seeded.
        uses_go_deps
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

#[cfg(test)]
mod tests {
    use std::{iter, str};

    use super::*;
    use crate::codegen::Bindings;
    use flick_ir::Program;

    fn test_data(go_deps: &[(&str, &str)], project_go_sum: Option<&str>) -> Data {
        Data {
            program: Program::new("test", vec![], vec![], &|_| String::new()),
            facts: BTreeMap::new(),
            lines: vec![],
            sources: vec![],
            foreign: BTreeMap::new(),
            bindings: Bindings::parse(iter::empty()),
            command: Command::Build,
            windows: false,
            cgo: false,
            inspect_module: None,
            go_deps: go_deps
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
            project_go_sum: project_go_sum.map(str::to_owned),
        }
    }

    fn test_go_tool() -> Toolchain {
        Toolchain {
            id: "go".into(),
            version: "1.23.0".into(),
            sha256: "0".repeat(64),
        }
    }

    fn run_add(
        data: &Data,
        code: Vec<(String, String)>,
        reached: &[&str],
        go_imports: &[&str],
    ) -> BuildResult {
        let mut result = BuildResult {
            artifacts: vec![],
            steps: vec![],
            runtime_dependencies: vec![],
            launches: vec![],
            diagnostics: vec![],
        };
        let imports: BTreeSet<String> = go_imports.iter().map(|&s| s.to_owned()).collect();
        add(
            &mut result,
            data,
            &test_go_tool(),
            "main",
            code,
            reached,
            &imports,
        );
        result
    }

    #[test]
    fn unimported_dep_string_literal_does_not_declare_go_sum_step_output() {
        let data = test_data(&[("example.com/pkg", "v1.0.0")], None);
        let code = vec![(
            "m_App.go".to_owned(),
            "package main\nvar s = \"example.com/pkg\"\n".to_owned(),
        )];
        let res = run_add(&data, code, &[], &[]);
        assert!(
            !res.steps[0]
                .outputs
                .iter()
                .any(|o| o.path.ends_with("go.sum")),
            "unimported string literal must not declare go.sum StepOutput: {:?}",
            res.steps[0].outputs
        );
    }

    #[test]
    fn imported_dep_without_runtime_packages_declares_go_sum_step_output() {
        let data = test_data(&[("example.com/pkg", "v1.0.0")], None);
        let res = run_add(&data, vec![], &[], &["example.com/pkg"]);
        assert!(
            res.steps[0]
                .outputs
                .iter()
                .any(|o| o.path == "go/main/go.sum"),
            "expected go/main/go.sum StepOutput: {:?}",
            res.steps[0].outputs
        );
        assert!(
            !res.artifacts.iter().any(|a| a.path == "go/main/go.sum"),
            "go/main/go.sum must not collide with artifacts"
        );
    }

    #[test]
    fn optional_runtime_package_with_go_deps_preserves_pinned_runtime_go_sum() {
        let pkg_name = PACKAGES.first().expect("optional runtime package").name;
        let data = test_data(&[("example.com/pkg", "v1.0.0")], None);
        let res = run_add(&data, vec![], &[pkg_name], &["example.com/pkg"]);
        let sum_artifact = res
            .artifacts
            .iter()
            .find(|a| a.path == "go/main/go.sum")
            .expect("pinned runtime GO_SUM artifact must be preserved");
        let text = str::from_utf8(&sum_artifact.bytes).expect("utf-8 go.sum");
        assert!(
            text.contains("h1:"),
            "expected pinned runtime checksums in go.sum: {text}"
        );
        assert!(
            !res.steps[0]
                .outputs
                .iter()
                .any(|o| o.path == "go/main/go.sum"),
            "go/main/go.sum must not be duplicated in StepOutput"
        );
    }
}
