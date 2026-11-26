//! Browser-side `flick:backend@0.1.0` component protocol preparation and result validation.
//!
//! `flick-wasm` compiles the frontend (parse, resolve, type-check, lower, `.d.ts` projection)
//! and constructs a verified `BuildRequest` plus content-addressed inputs for the pinned official
//! `flick/js` component (`backends/js/package/component.wasm`). On `wasm32`, the playground runs
//! the component in an isolated worker (`backend.worker.ts`) and feeds its `BuildResult` back to
//! `finish_emit`. On native hosts (`cargo test -p flick-wasm` and `wasm_demo`), `Session::emit`
//! drives the exact same `prepare_emit` -> `flick_backend::generate(&flick_backend_js::Javascript)`
//! -> `finish_emit` contract.

mod finish;
mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use flick_backend::{
    Abi, BackendId, BackendPin, BuildRequest, BuildResult, Diagnostic as BackendDiagnostic, Entry,
    Environment, ForeignBinding, InputKind, JavascriptEntry, JavascriptFile, JavascriptProjection,
    Mode, ModuleInput, ModuleRole, OptionValue, Profile, Projection, Source, Symbol, TargetRequest,
};
#[cfg(not(target_arch = "wasm32"))]
use flick_backend::{DeclaredInputs, Descriptor, Limits, validate_request};
use flick_backend_support::kernel;
use flick_core::build::{self, Artifact as FrontendArtifact, Backend, Extras, Settings};
use flick_core::codegen::{self, EffectBridge};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Namespace;
use flick_core::project::Analysis;
use flick_core::source::{FileId, SourceMap};
use flick_core::types;
use serde::{Deserialize, Serialize};

use crate::{DISPLAYED, path};
pub(crate) use finish::finish;
pub(crate) use support::base64_encode;
use support::{
    build_program, build_shared_dts, collect_inline_foreign, collect_own_host_exports,
    declare_input,
};

/// The checked-in pin of the official `flick/js` component.
const JS_BACKEND_PIN_JSON: &str = include_str!("../../../backends/js/package/backend.json");

#[derive(Deserialize)]
struct PinFile {
    identity: PinIdentity,
    sha256: String,
}

#[derive(Deserialize)]
struct PinIdentity {
    id: String,
    version: String,
}

pub(crate) fn js_backend_pin() -> BackendPin {
    let parsed: PinFile =
        serde_json::from_str(JS_BACKEND_PIN_JSON).expect("checked-in js backend.json is valid");
    BackendPin {
        identity: BackendId {
            id: parsed.identity.id,
            version: parsed.identity.version,
        },
        sha256: parsed.sha256,
    }
}

/// Prepared state kept across `prepare_emit` -> component invocation -> `finish_emit`.
pub(crate) struct PreparedEmit {
    pub(crate) profile: String,
    pub(crate) include_declarations: bool,
    pub(crate) request: BuildRequest,
    pub(crate) inputs: BTreeMap<String, Vec<u8>>,
    pub(crate) files: Vec<FileId>,
    pub(crate) shared_dts: Vec<FrontendArtifact>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) dts_diagnostics: Vec<Diagnostic>,
}

/// JSON payload returned by `Session::prepare_emit`.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum PrepareReply {
    /// Frontend diagnostics already failed; no backend component call is needed.
    Done { emitted: serde_json::Value },
    /// Ready to invoke the pinned `flick/js` component with `request` and `inputs`.
    Ready {
        pin: BackendPin,
        request: Box<BuildRequest>,
        program_b64: String,
        inputs: BTreeMap<String, String>,
    },
}

/// JSON payload passed into `Session::finish_emit` from the backend worker.
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ComponentReply {
    Ok {
        #[cfg(not(target_arch = "wasm32"))]
        descriptor: Option<Descriptor>,
        result: BuildResult,
        /// Base64-encoded bytes aligned 1:1 with `result.artifacts` (so JS avoids giant u8 arrays).
        #[serde(default)]
        artifact_bytes_b64: Option<Vec<String>>,
    },
    Err {
        #[cfg(not(target_arch = "wasm32"))]
        descriptor: Option<Descriptor>,
        diagnostics: Vec<BackendDiagnostic>,
    },
    TransportError {
        message: String,
    },
}

/// Build the complete `BuildRequest`, declared input bytes, and shared `.d.ts` artifacts.
#[allow(clippy::too_many_lines)]
pub(crate) fn prepare(
    profile: &str,
    include_declarations: bool,
    analysis: &Analysis,
    sources: &mut SourceMap,
) -> Result<PreparedEmit, (Vec<Diagnostic>, Vec<Diagnostic>)> {
    let mut diagnostics = analysis.diagnostics.clone();
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err((diagnostics, Vec::new()));
    }

    let compilation = &analysis.compilation;
    let module = compilation.modules.iter().find(|m| m.path == path());
    let entries_list: Vec<(String, String)> = module
        .filter(|m| m.interface.exports_value("main"))
        .map(|m| ("main".to_owned(), m.name.clone()))
        .into_iter()
        .collect();
    let source_path = |_: FileId| format!("file://{}", path().display());
    let source_dir = |_: &str| path().parent().map(Path::to_path_buf);
    let settings = Settings {
        package: None,
        entries: &entries_list,
        typescript: false,
        release: profile == "release",
        extras: Extras {
            dts: true,
            ..Extras::default()
        },
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend: Backend::Rt,
        effect_bridge: EffectBridge::Available,
        ui: None,
        runners: &[],
        environment: None,
    };

    let entry_facts = build::javascript_entries(&settings, &compilation.modules, &mut diagnostics);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err((diagnostics, Vec::new()));
    }

    let published_interfaces = match build::published_interfaces(compilation, None, None, None) {
        Ok(interfaces) => interfaces,
        Err(error) => {
            diagnostics.push(Diagnostic::new(codes::P010, Severity::Error, error));
            return Err((diagnostics, Vec::new()));
        }
    };

    let (program, files) = build_program(sources, compilation);
    let inline_foreign = collect_inline_foreign(&program);
    let host_exports = collect_own_host_exports(&program, compilation);

    let option = |key: &str, value: &str| OptionValue {
        key: key.into(),
        value: value.into(),
    };
    let options = vec![
        option("command", "build"),
        option("host.os", "browser"),
        option("labels", "all"),
        option("option.typescript", "false"),
        option("option.declarations", "true"),
        option("option.promise-facade", "false"),
        option("option.schema-aot", "false"),
        option("option.runtime", "rt"),
        option("option.kind", "app"),
        option("option.format", "esm"),
    ];

    let target = TargetRequest {
        backend_api: flick_backend::VERSION.into(),
        ir_version: flick_ir::VERSION.into(),
        language_version: flick_core::VERSION.into(),
        runtime_abis: vec![
            Abi {
                name: "flickrt".into(),
                version: flick_core::VERSION.into(),
            },
            Abi {
                name: kernel::ABI.into(),
                version: kernel::VERSION.into(),
            },
        ],
        environment: Environment {
            backend: "js".into(),
            platform: "browser".into(),
        },
        tags: vec!["js".into(), "rt".into()],
        host_package: None,
        profile: if profile == "release" {
            Profile::Release
        } else {
            Profile::Debug
        },
        mode: Mode::WholeProgram,
        required_capabilities: vec![],
        options,
    };

    let entries: Vec<Entry> = entries_list
        .iter()
        .map(|(name, module_name)| Entry {
            name: name.clone(),
            symbol: Symbol {
                module: module_name.clone(),
                name: "main".into(),
            },
            runner: None,
        })
        .collect();

    let mut request = BuildRequest {
        target,
        backend: js_backend_pin(),
        program: program.to_cbor(),
        module_root: None,
        modules: Vec::new(),
        sources: Vec::new(),
        entries,
        test_roots: Vec::new(),
        foreign: Vec::new(),
        inline_foreign,
        projections: Vec::new(),
        package_targets: Vec::new(),
        root_handlers: Vec::new(),
        inputs: Vec::new(),
        toolchains: Vec::new(),
        runtime_dependencies: Vec::new(),
        declared_dependencies: None,
        exports: host_exports,
        headers: None,
    };
    let mut input_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for (i, name) in program.files.into_iter().enumerate() {
        let file = sources.get(files[i]);
        let id = declare_input(
            &mut request.inputs,
            &mut input_bytes,
            format!("source/{i}"),
            InputKind::Source,
            file.text.as_bytes().to_vec(),
        );
        request.sources.push(Source {
            name,
            input: id,
            line_starts: file.line_starts().to_vec(),
        });
    }

    let builtin_names: Vec<&str> = types::builtin_sources().iter().map(|(n, _)| *n).collect();
    for m in &compilation.modules {
        request.modules.push(ModuleInput {
            name: m.name.clone(),
            role: ModuleRole::Own,
            package: m.ir.package.clone().or_else(|| m.package.clone()),
            output: codegen::module_path(&builtin_names, false, &m.name),
            interface: None,
        });
    }
    for interface in types::builtins_of(true) {
        if request.modules.iter().any(|m| m.name == interface.module) {
            continue;
        }
        request.modules.push(ModuleInput {
            name: interface.module.clone(),
            role: ModuleRole::Builtin,
            package: None,
            output: codegen::module_path(&builtin_names, false, &interface.module),
            interface: None,
        });
    }

    // Attach flick.javascript@0.1.0 projections for every module (own + builtins).
    for meta in request.modules.clone() {
        let own = compilation.modules.iter().find(|m| m.name == meta.name);
        let interface = published_interfaces.get(meta.name.as_str()).or_else(|| {
            types::builtins_of(true)
                .iter()
                .find(|i| i.module == meta.name)
        });
        let Some(interface) = interface else {
            continue;
        };
        let mut physical = vec![meta.name.clone()];
        if let Some(m) = own {
            physical.extend(
                m.ir.initialization
                    .prefixes
                    .iter()
                    .map(|p| codegen::prefix_key(&meta.name, p.before)),
            );
        }
        let facts = JavascriptProjection {
            builtin: builtin_names.contains(&meta.name.as_str()),
            exports: interface
                .exports
                .iter()
                .filter(|e| e.entity.namespace == Namespace::Value && e.entity.module == meta.name)
                .map(|e| e.entity.name.clone())
                .collect(),
            entries: entry_facts
                .iter()
                .filter(|entry| {
                    request
                        .entries
                        .iter()
                        .any(|e| e.name == entry.name && e.symbol.module == meta.name)
                })
                .map(|e| JavascriptEntry {
                    name: e.name.clone(),
                    module: e.defining.clone(),
                    pure: e.pure,
                    host: e.host.clone(),
                })
                .collect(),
            files: physical
                .into_iter()
                .map(|module_name| {
                    let output = codegen::module_path(&builtin_names, false, &module_name);
                    let source_paths = files
                        .iter()
                        .map(|&f| {
                            let displayed = &sources.get(f).path;
                            if displayed == DISPLAYED {
                                format!("file://{}", path().display())
                            } else {
                                displayed.clone()
                            }
                        })
                        .collect();
                    JavascriptFile {
                        module: module_name,
                        output,
                        source_paths,
                    }
                })
                .collect(),
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&facts, &mut bytes).expect("plain JS facts serialize");
        let id = declare_input(
            &mut request.inputs,
            &mut input_bytes,
            format!("javascript/{}", meta.name),
            InputKind::TargetFacts,
            bytes,
        );
        request.projections.push(Projection {
            module: meta.name,
            schema: Abi {
                name: "flick.javascript".into(),
                version: "0.1.0".into(),
            },
            input: id,
        });
    }

    // Foreign bindings and shared `.d.ts` files.
    let resolved = RefCell::new(build::javascript_foreign_inputs(compilation, &settings));
    let (shared_dts, dts_issues) = build_shared_dts(
        compilation,
        &settings,
        &published_interfaces,
        &builtin_names,
        &resolved,
    );
    let (dts_diagnostics, other_issues): (Vec<_>, Vec<_>) =
        dts_issues.into_iter().partition(|d| d.code == codes::F004);
    diagnostics.extend(other_issues);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err((diagnostics, dts_diagnostics));
    }

    let binding_abi = Abi {
        name: "flick.javascript-ffi".into(),
        version: "0.1.0".into(),
    };
    request.foreign = resolved
        .into_inner()
        .into_iter()
        .map(|r| ForeignBinding {
            module: r.module,
            specifier: r.spec,
            resolved: r.specifier,
            abi: binding_abi.clone(),
            inputs: vec![],
        })
        .collect();

    // Validate the request locally on native targets (on wasm32, avoiding `Program::from_cbor`
    // keeps the IR CBOR deserializer out of the browser bundle; the component itself validates
    // the decoded program and source spans on entry).
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut declared = DeclaredInputs::new(&request.inputs, |id| input_bytes.get(id).cloned())
            .map_err(|why| {
                let mut diags = diagnostics.clone();
                diags.push(Diagnostic::new(codes::P013, Severity::Error, why));
                (diags, dts_diagnostics.clone())
            })?;
        if let Err(why) = validate_request(&request, &mut declared, Limits::default()) {
            diagnostics.push(Diagnostic::new(codes::P013, Severity::Error, why));
            return Err((diagnostics, dts_diagnostics));
        }
    }

    Ok(PreparedEmit {
        profile: profile.to_owned(),
        include_declarations,
        request,
        inputs: input_bytes,
        files,
        shared_dts,
        diagnostics,
        dts_diagnostics,
    })
}
