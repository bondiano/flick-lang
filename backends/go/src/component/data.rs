//! Decode only declared plain inputs, without compiler objects or callbacks.

use std::collections::BTreeMap;

use flick_backend::{BuildRequest, Diagnostic, GoProjection, InputKind, Inputs};
use flick_ir::Program;

use super::failure;
use crate::codegen::Bindings;

/// The Go binding tables of the implicit packages: resource inputs whose id
/// starts so, one table each.
pub(super) const BINDINGS: &str = "go/bindings/";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Build,
    Run,
    Dev,
    Test,
    Inspect,
}

pub(super) struct Data {
    pub program: Program,
    /// By own module.
    pub facts: BTreeMap<String, GoProjection>,
    /// The line starts of each source, by its index in `Program.files`.
    pub lines: Vec<Vec<u32>>,
    /// By module and specifier: the name of a foreign Go file and its text.
    pub foreign: BTreeMap<(String, String), (String, String)>,
    pub bindings: Bindings,
    pub command: Command,
    /// The host runs Windows executables, named `.exe`.
    pub windows: bool,
    /// Enable cgo (`CGO_ENABLED=1`).
    pub cgo: bool,
    /// Module selected for `inspect --emit output`.
    pub inspect_module: Option<String>,
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
                != ("flick.go-ffi", "0.1.0")
            {
                return Err(failure(
                    "P012",
                    format!("unsupported foreign ABI {}", binding.abi.name),
                ));
            }
            let [input] = binding.inputs.as_slice() else {
                return Err(failure("P012", "a foreign Go file is one input"));
            };
            let text = String::from_utf8(read(input, "foreign")?)
                .map_err(|e| failure("P012", format!("foreign Go encoding: {e}")))?;
            let key = (binding.module.clone(), binding.specifier.clone());
            foreign.insert(key, (binding.resolved.clone(), text));
        }
        let mut tables = Vec::new();
        for input in (request.inputs.iter())
            .filter(|i| i.kind == InputKind::Resource && i.id.starts_with(BINDINGS))
        {
            let text = String::from_utf8(read(&input.id, "binding table")?)
                .map_err(|e| failure("P012", format!("binding table encoding: {e}")))?;
            tables.push(text);
        }
        let (command, windows, cgo, inspect_module) = options(request)?;
        Ok(Self {
            program,
            facts,
            lines: (request.sources.iter())
                .map(|s| s.line_starts.clone())
                .collect(),
            foreign,
            bindings: Bindings::parse(tables.iter().map(String::as_str)),
            command,
            windows,
            cgo,
            inspect_module,
        })
    }
}

fn options(
    request: &BuildRequest,
) -> Result<(Command, bool, bool, Option<String>), Vec<Diagnostic>> {
    let mut command = None;
    let mut windows = None;
    let mut cgo = None;
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
    Ok((
        command.unwrap_or(Command::Build),
        windows.unwrap_or(false),
        cgo.unwrap_or(false),
        inspect_module,
    ))
}
