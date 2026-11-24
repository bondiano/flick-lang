//! Decode only declared plain inputs, without compiler objects or callbacks.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{
    BuildRequest, Diagnostic, GoProjection, InputKind, Inputs, ModuleRole, Severity, Span,
};
use flick_ir::Program;

use super::{failure, value_abi};
use crate::codegen::{Bindings, is_relative};
use crate::{exports, foreign_meta, inline};

/// The Go binding tables of the implicit packages: resource inputs whose id
/// starts so, one table each.
pub(super) const BINDINGS: &str = "go/bindings/";
/// Declared `[target.go.dependencies]` pairs (`<module> <version>\n`).
pub(super) const GO_DEPENDENCIES: &str = "go/dependencies";
/// Optional project-level `go.sum` lockfile.
pub(super) const GO_SUM_INPUT: &str = "go/go.sum";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BuildKind {
    App,
    Library,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Build,
    Run,
    Dev,
    Test,
    Inspect,
    Check,
}

pub(super) struct Data {
    pub program: Program,
    /// By own module.
    pub facts: BTreeMap<String, GoProjection>,
    /// The line starts of each source, by its index in `Program.files`.
    pub lines: Vec<Vec<u32>>,
    /// The text of each source file, by its index in `Program.files`.
    pub sources: Vec<String>,
    /// By module and specifier: the name of a foreign Go file and its text.
    pub foreign: BTreeMap<(String, String), (String, String)>,
    pub bindings: Bindings,
    pub command: Command,
    /// The host runs Windows executables, named `.exe`.
    pub windows: bool,
    /// Enable cgo (`CGO_ENABLED=1`).
    pub cgo: bool,
    /// Build artifact kind (`app` or `library`).
    pub kind: BuildKind,
    /// Module selected for `inspect --emit output`.
    pub inspect_module: Option<String>,
    /// Third-party Go modules declared via `[target.go.dependencies]`.
    pub go_deps: BTreeMap<String, String>,
    /// Optional project `go.sum` supplied as a lockfile input.
    pub project_go_sum: Option<String>,
}

impl Data {
    pub fn read(request: &BuildRequest, inputs: &mut dyn Inputs) -> Result<Self, Vec<Diagnostic>> {
        let program =
            Program::from_cbor(&request.program).map_err(|e| failure("P012", e.to_string()))?;
        let mut read = |id: &str, what: &str| {
            inputs
                .read(id)
                .map_err(|e| failure("P012", format!("{what} input: {e:?}")))
        };
        let mut facts = BTreeMap::new();
        for projection in &request.projections {
            let schema = (
                projection.schema.name.as_str(),
                projection.schema.version.as_str(),
            );
            if schema != ("flick.go", "0.1.0") {
                return Err(failure(
                    "P012",
                    format!("unsupported projection {}@{}", schema.0, schema.1),
                ));
            }
            let bytes = read(&projection.input, "projection")?;
            let value = ciborium::from_reader(&bytes[..])
                .map_err(|e| failure("P012", format!("Go facts: {e}")))?;
            if facts.insert(projection.module.clone(), value).is_some() {
                return Err(failure("P012", "duplicate Go projection"));
            }
        }
        let mut foreign = BTreeMap::new();
        for binding in &request.foreign {
            if (binding.abi.name.as_str(), binding.abi.version.as_str())
                != (value_abi::ABI_NAME, value_abi::ABI_VERSION)
            {
                return Err(failure(
                    "F014",
                    format!(
                        "foreign binding ABI mismatch for `{}` in `{}`: expected `{}@{}`, got `{}@{}`",
                        binding.specifier,
                        binding.module,
                        value_abi::ABI_NAME,
                        value_abi::ABI_VERSION,
                        binding.abi.name,
                        binding.abi.version
                    ),
                ));
            }
            if is_relative(&binding.specifier) {
                let [input] = binding.inputs.as_slice() else {
                    return Err(failure("P012", "a foreign Go file is one input"));
                };
                let text = String::from_utf8(read(input, "foreign")?)
                    .map_err(|e| failure("P012", format!("foreign Go encoding: {e}")))?;
                let key = (binding.module.clone(), binding.specifier.clone());
                foreign.insert(key, (binding.resolved.clone(), text));
            } else if !binding.inputs.is_empty() {
                return Err(failure(
                    "P012",
                    "a direct Go package binding takes no sibling file input",
                ));
            }
        }
        let mut tables = Vec::new();
        for input in (request.inputs.iter())
            .filter(|i| i.kind == InputKind::Resource && i.id.starts_with(BINDINGS))
        {
            let text = String::from_utf8(read(&input.id, "binding table")?)
                .map_err(|e| failure("P012", format!("binding table encoding: {e}")))?;
            tables.push(text);
        }
        let mut sources = Vec::with_capacity(request.sources.len());
        for source in &request.sources {
            let text = String::from_utf8(read(&source.input, "source")?)
                .map_err(|e| failure("P012", format!("source encoding: {e}")))?;
            sources.push(text);
        }
        let GoDepInputs {
            deps: go_deps,
            go_sum: project_go_sum,
        } = read_go_deps_and_sum(request, inputs)?;
        let parsed = options(request)?;
        Ok(Self {
            program,
            facts,
            lines: (request.sources.iter())
                .map(|s| s.line_starts.clone())
                .collect(),
            sources,
            foreign,
            bindings: Bindings::parse(tables.iter().map(String::as_str)),
            command: parsed.command,
            windows: parsed.windows,
            cgo: parsed.cgo,
            kind: parsed.kind,
            inspect_module: parsed.inspect_module,
            go_deps,
            project_go_sum,
        })
    }

    pub fn check(&self, request: &BuildRequest) -> Result<(), Vec<Diagnostic>> {
        let own_modules: BTreeSet<&str> = request
            .modules
            .iter()
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect();
        let mut diagnostics: Vec<Diagnostic> =
            foreign_meta::validate_program(&self.program, &own_modules)
                .into_iter()
                .chain(inline::validate_program(
                    &self.program,
                    &self.sources,
                    &own_modules,
                ))
                .map(|d| Diagnostic {
                    code: d.code.into(),
                    severity: Severity::Error,
                    message: d.message,
                    span: Some(Span {
                        file: d.span.file.index(),
                        start: d.span.start,
                        end: d.span.end,
                    }),
                    notes: vec![],
                })
                .collect();
        diagnostics.extend(exports::validate_request(&self.program, &request.exports));
        if !diagnostics.is_empty() {
            diagnostics.sort_by_key(|d| {
                d.span
                    .as_ref()
                    .map_or((u32::MAX, u32::MAX, u32::MAX), |s| (s.file, s.start, s.end))
            });
            return Err(diagnostics);
        }
        Ok(())
    }
}

struct ParsedOptions {
    command: Command,
    windows: bool,
    cgo: bool,
    kind: BuildKind,
    inspect_module: Option<String>,
}

fn options(request: &BuildRequest) -> Result<ParsedOptions, Vec<Diagnostic>> {
    let mut command = None;
    let mut windows = None;
    let mut cgo = None;
    let mut kind = None;
    let mut inspect_module = None;
    for option in &request.target.options {
        let duplicate = match option.key.as_str() {
            "command" => command
                .replace(match option.value.as_str() {
                    "build" => Command::Build,
                    "run" => Command::Run,
                    "dev" => Command::Dev,
                    "test" => Command::Test,
                    "inspect" => Command::Inspect,
                    "check" => Command::Check,
                    other => return Err(failure("P012", format!("unknown command `{other}`"))),
                })
                .is_some(),
            "host.os" => windows.replace(option.value == "windows").is_some(),
            "labels" => false,
            "inspect.module" => inspect_module.replace(option.value.clone()).is_some(),
            "option.cgo" => {
                let value = match option.value.as_str() {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(failure(
                            "P012",
                            format!("invalid boolean `{other}` for option `cgo`"),
                        ));
                    }
                };
                cgo.replace(value).is_some()
            }
            "option.kind" => {
                let value = match option.value.as_str() {
                    "app" => BuildKind::App,
                    "library" => BuildKind::Library,
                    other => {
                        return Err(failure("P012", format!("unknown Go build kind `{other}`")));
                    }
                };
                kind.replace(value).is_some()
            }
            other => {
                return Err(failure(
                    "P012",
                    format!("unknown Go backend option `{other}`"),
                ));
            }
        };
        if duplicate {
            return Err(failure(
                "P012",
                format!("duplicate option `{}`", option.key),
            ));
        }
    }
    Ok(ParsedOptions {
        command: command.unwrap_or(Command::Build),
        windows: windows.unwrap_or(false),
        cgo: cgo.unwrap_or(false),
        kind: kind.unwrap_or(BuildKind::App),
        inspect_module,
    })
}

struct GoDepInputs {
    deps: BTreeMap<String, String>,
    go_sum: Option<String>,
}

fn read_go_deps_and_sum(
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
) -> Result<GoDepInputs, Vec<Diagnostic>> {
    let mut deps = BTreeMap::new();
    if request
        .inputs
        .iter()
        .any(|i| i.kind == InputKind::TargetFacts && i.id == GO_DEPENDENCIES)
    {
        let raw = inputs
            .read(GO_DEPENDENCIES)
            .map_err(|e| failure("P012", format!("Go dependencies input: {e:?}")))?;
        let text = String::from_utf8(raw)
            .map_err(|e| failure("P012", format!("Go dependencies encoding: {e}")))?;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some((module, version)) = trimmed.split_once(' ') else {
                return Err(failure(
                    "P012",
                    format!("malformed Go dependency entry `{trimmed}`"),
                ));
            };
            deps.insert(module.to_owned(), version.trim().to_owned());
        }
    }
    let go_sum = if request
        .inputs
        .iter()
        .any(|i| i.kind == InputKind::Lockfile && i.id == GO_SUM_INPUT)
    {
        let raw = inputs
            .read(GO_SUM_INPUT)
            .map_err(|e| failure("P012", format!("go.sum input: {e:?}")))?;
        let text =
            String::from_utf8(raw).map_err(|e| failure("P012", format!("go.sum encoding: {e}")))?;
        Some(text)
    } else {
        None
    };
    Ok(GoDepInputs { deps, go_sum })
}
