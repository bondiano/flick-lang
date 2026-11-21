//! A verified package invocation. No component failure calls native codegen.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Command;

use flick_backend::command;
use flick_backend::host::runtime::Cancellation;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{
    Backend, BackendId, BackendPin, BuildRequest, Failure, Generated, Limits, Policy,
    RuntimeDependency,
};
use flick_core::build::{go::FILES, schema_aot_file};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::BackendPackage;
use flick_core::project::{Compilation, Project, js};
use flick_core::source::{FileId, SourceMap, Span};

use super::request::Prepared;
use super::tools::{self, Tool};
use super::{execute, failure, launch};
use crate::artifact::Artifact;
use crate::commands::build::Purpose;
use crate::commands::build::{BuildPlan, Emission};
use crate::commands::contributed::Contributor;
use crate::component;
use crate::sources::FsHost;

pub(crate) struct Package {
    pin: BackendPin,
    root: PathBuf,
    backend: WasmBackend,
    tools: Vec<Tool>,
    /// The tools `[tools]` lets the steps of this backend run.
    allowed: Vec<String>,
    /// An official component, which the compiler's own host integrations
    /// approve; a third-party descriptor cannot grant itself host authority.
    official: bool,
}

impl Package {
    pub(super) fn load(project: &Project, package: &BackendPackage) -> Result<Self, Diagnostic> {
        let pin = BackendPin {
            identity: BackendId {
                id: package.id.clone(),
                version: package.version.clone(),
            },
            sha256: package.sha256.clone(),
        };
        let limit = Limits::default().artifact_bytes;
        let mut bytes = Vec::new();
        File::open(project.root.join(&package.component))
            .and_then(|f| {
                f.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(|e| failure(format!("cannot read backend component: {e}")))?;
        if bytes.len() > limit {
            return Err(failure("backend component exceeds its file size limit"));
        }
        let official = super::official::matches(package, project);
        Self::from_bytes(
            &project.root,
            pin,
            &bytes,
            Some(&project.config.tools),
            official,
        )
    }

    pub(super) fn from_bytes(
        root: &Path,
        pin: BackendPin,
        bytes: &[u8],
        allowed: Option<&BTreeMap<String, Vec<String>>>,
        official: bool,
    ) -> Result<Self, Diagnostic> {
        let backend = WasmBackend::load_with_cancellation(
            component::engine().map_err(failure)?,
            pin.clone(),
            bytes,
            Budget::default(),
            component::cancellation().map_err(failure)?.clone(),
        )
        .map_err(|why| {
            failure(format!(
                "backend {}@{}#{}: {why}",
                pin.identity.id, pin.identity.version, pin.sha256
            ))
        })?;
        super::options::save_descriptor(&pin.sha256, &backend.describe());
        // outside a project only `--allow-tool` grants a tool
        let allowed = allowed.map_or_else(
            || super::allowed_tools().to_vec(),
            |tools| tools.get(&pin.identity.id).cloned().unwrap_or_default(),
        );
        // a launch runs on Node without `[tools]`; a step needs its tool allowed
        let ids = (allowed.iter().map(String::as_str))
            .filter(|&id| id != "node")
            .chain(["node"]);
        let tools = ids.filter_map(|id| Tool::find(id, None).ok()).collect();
        Ok(Self {
            pin,
            root: root.to_path_buf(),
            backend,
            tools,
            allowed,
            official,
        })
    }

    pub(super) fn pin(&self) -> BackendPin {
        self.pin.clone()
    }

    pub(super) fn descriptor(&self) -> flick_backend::Descriptor {
        self.backend.describe()
    }

    pub(super) fn is_official_js(&self) -> bool {
        self.official && self.pin.identity.id == super::options::OFFICIAL_JS
    }

    /// These compiler-owned host integrations are approved for the exact shipped
    /// image. A third-party descriptor cannot grant itself host tool authority.
    pub(super) fn supports_host(&self, capability: &str) -> bool {
        self.official
            && self
                .backend
                .describe()
                .capabilities
                .iter()
                .any(|c| c.name == capability && c.version == "1.0.0")
    }

    pub(super) fn schema_aot_command(&self, root: &Path) -> Result<Command, String> {
        if !self.supports_host("host-schema-aot") {
            return Err(
                "component schema AOT requires an approved pinned host tool adapter".into(),
            );
        }
        launch::verify_runtime(root, &self.pin)?;
        let expected = self
            .tools
            .iter()
            .find(|t| t.pin.id == "node")
            .ok_or("schema AOT requires Node")?;
        Ok(Tool::find("node", Some(&expected.pin))?.command())
    }

    fn preflight(
        &self,
        host: &FsHost,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = host_preflight(host, project, compilation, output);
        if self.supports_host("host-source-build")
            && self.tools.is_empty()
            && (!matches!(output.purpose, Purpose::Build | Purpose::Inspect)
                || output.extras.schema_aot)
        {
            diagnostics.push(failure(
                "the selected JavaScript operation requires an installed Node runtime",
            ));
        }
        diagnostics
    }

    pub(crate) fn artifacts(
        &self,
        host: &FsHost,
        sources: &mut SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
    ) -> Emission {
        let diagnostics = self.preflight(host, project, compilation, output);
        if !diagnostics.is_empty() {
            return Emission {
                artifacts: vec![],
                diagnostics,
            };
        }
        let mut prepared = Prepared::new(
            host,
            sources,
            project,
            compilation,
            output,
            self.pin.clone(),
        );
        if prepared
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return Emission {
                artifacts: vec![],
                diagnostics: prepared.diagnostics,
            };
        }
        prepared.request.runtime_dependencies = if output.purpose == Purpose::Inspect {
            Vec::new()
        } else {
            match self.runtime_dependencies(&prepared.request) {
                Ok(pins) => pins,
                Err(diagnostic) => {
                    return Emission {
                        artifacts: vec![],
                        diagnostics: vec![diagnostic],
                    };
                }
            }
        };
        prepared.request.toolchains = self.tools.iter().map(|t| t.pin.clone()).collect();
        let reserved = if output.purpose == Purpose::Inspect {
            Vec::new()
        } else {
            reserved_outputs(&prepared, output, project)
        };
        let environment = tools::settable();
        let policy = Policy {
            toolchains: &prepared.request.toolchains,
            environment: &environment,
            reserved_paths: &reserved,
            limits: Limits::default(),
            authorize_step: &tools::authorize,
        };
        component::cancel_on_signals();
        let mut emission = self.finish_generation(
            flick_backend::generate(
                &self.backend,
                &prepared.request,
                &policy,
                |id| prepared.bytes.get(id).cloned(),
                true,
            ),
            &mut prepared,
            output,
        );
        if project.is_none() && output.purpose != Purpose::Inspect {
            standalone_files(&mut emission);
        }
        emission.diagnostics.splice(0..0, prepared.diagnostics);
        emission
    }

    fn finish_generation(
        &self,
        result: Result<Generated, Failure>,
        prepared: &mut Prepared,
        output: &BuildPlan,
    ) -> Emission {
        match result {
            Ok(generated) if output.purpose == Purpose::Inspect => {
                let result = generated.result();
                Emission {
                    artifacts: result
                        .artifacts
                        .iter()
                        .cloned()
                        .map(Artifact::from)
                        .collect(),
                    diagnostics: (result.diagnostics.iter())
                        .map(|d| self.diagnostic(d, &prepared.files))
                        .collect(),
                }
            }
            Ok(generated) => {
                if let Err(diagnostic) = self.outputs(&generated, prepared, output) {
                    return Emission {
                        artifacts: vec![],
                        diagnostics: vec![*diagnostic],
                    };
                }
                Emission {
                    artifacts: mem::take(&mut prepared.shared),
                    diagnostics: (generated.result().diagnostics.iter())
                        .map(|d| self.diagnostic(d, &prepared.files))
                        .collect(),
                }
            }
            Err(Failure::Rejected(diagnostics)) => self.rejected(&diagnostics, &prepared.files),
            Err(why) => Emission {
                artifacts: vec![],
                diagnostics: vec![failure(format!(
                    "backend {}@{}#{} on {}/{}: {why}",
                    self.pin.identity.id,
                    self.pin.identity.version,
                    self.pin.sha256,
                    prepared.request.target.environment.backend,
                    prepared.request.target.environment.platform,
                ))],
            },
        }
    }

    /// Adds the launch manifest, the artifacts of a generation and the
    /// outputs of its steps to the shared files.
    fn outputs(
        &self,
        generated: &Generated,
        prepared: &mut Prepared,
        output: &BuildPlan,
    ) -> Result<(), Box<Diagnostic>> {
        let result = generated.result();
        let require_runs = !(output.purpose == Purpose::Build
            && self.tools.is_empty()
            && self.supports_host("host-source-build"));
        required_launches(result, output, require_runs).map_err(|why| Box::new(failure(why)))?;
        let manifest = launch::artifact(&self.pin, result).map_err(|why| Box::new(failure(why)))?;
        let outputs = self.steps(generated, &prepared.bytes, output)?;
        prepared.shared.push(manifest);
        (prepared.shared).extend(result.artifacts.iter().cloned().map(Artifact::from));
        prepared.shared.extend(outputs);
        Ok(())
    }

    /// Runs the steps of a generation once every tool they name is allowed.
    fn steps(
        &self,
        generated: &Generated,
        sources: &BTreeMap<String, Vec<u8>>,
        output: &BuildPlan,
    ) -> Result<Vec<Artifact>, Box<Diagnostic>> {
        let result = generated.result();
        let cancel = component::cancellation().map_err(|why| Box::new(failure(why)))?;
        let id = &self.pin.identity.id;
        if let Some(step) = (result.steps.iter()).find(|s| !self.allowed.contains(&s.toolchain.id))
        {
            let tool = &step.toolchain.id;
            return Err(Box::new(
                Diagnostic::new(
                    codes::P051,
                    Severity::Error,
                    format!(
                        "backend `{id}` runs `{tool}` in step `{}`, which `[tools]` does not allow",
                        step.id
                    ),
                )
                .with_fix(|| {
                    (
                        format!("allow it in `flick.toml`: `[tools]` `\"{id}\" = [\"{tool}\"]`"),
                        Vec::new(),
                    )
                }),
            ));
        }
        execute::run(
            result,
            generated.execution_order(),
            sources,
            &execute::Reads::new(),
            &self.tools,
            &output.out_dir,
            cancel,
        )
        .map_err(|failure| {
            let message = match failure {
                execute::Failure::Cancelled => "the build was cancelled".to_owned(),
                execute::Failure::Step(why) => why,
            };
            Box::new(Diagnostic::new(codes::P050, Severity::Error, message))
        })
    }

    fn runtime_dependencies(
        &self,
        request: &BuildRequest,
    ) -> Result<Vec<RuntimeDependency>, Diagnostic> {
        let effect = request.target.options.iter().any(|o| {
            (o.key == "option.runtime" && o.value == "effect")
                || (o.key == "js.effect-bridge" && o.value != "absent")
                || (o.key == "option.schema-aot" && o.value == "true")
        });
        super::dependencies::installed_for(&self.root, effect).map_err(failure)
    }

    fn rejected(&self, diagnostics: &[flick_backend::Diagnostic], files: &[FileId]) -> Emission {
        let mut diagnostics: Vec<_> = diagnostics
            .iter()
            .map(|d| self.diagnostic(d, files))
            .collect();
        if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
            diagnostics.push(failure("backend refused the build"));
        }
        Emission {
            artifacts: vec![],
            diagnostics,
        }
    }

    pub(super) fn run_paths(&self, files: &[Artifact]) -> Vec<(String, String)> {
        launch::run_paths(files, &self.pin)
    }

    pub(super) fn cache_identity(&self, effect: bool) -> String {
        let runtimes = super::dependencies::installed_for(&self.root, effect).unwrap_or_default();
        format!(
            "{:?} {:?} {:?}",
            self.pin,
            runtimes,
            self.tools.iter().map(|t| &t.pin).collect::<Vec<_>>()
        )
    }

    pub(super) fn accepts_cached(&self, files: &[Artifact]) -> bool {
        launch::validate(
            files,
            &self.pin,
            &self.tools.iter().map(|t| t.pin.clone()).collect::<Vec<_>>(),
            &self.root,
        )
        .is_ok()
    }

    fn diagnostic(&self, diagnostic: &flick_backend::Diagnostic, files: &[FileId]) -> Diagnostic {
        let code = codes::lookup(&format!("FLICK-{}", diagnostic.code))
            .map(|info| info.code)
            .or_else(|| codes::plugin(&self.pin.identity.id, &diagnostic.code))
            .unwrap_or(codes::P013);
        let severity = match diagnostic.severity {
            flick_backend::Severity::Error => Severity::Error,
            flick_backend::Severity::Warning => Severity::Warning,
            flick_backend::Severity::Note => Severity::Note,
        };
        let mut message = format!(
            "{}\nbackend {}@{}#{}",
            diagnostic.message, self.pin.identity.id, self.pin.identity.version, self.pin.sha256
        );
        for note in &diagnostic.notes {
            message.push_str("\nnote: ");
            message.push_str(note);
        }
        let mut mapped = Diagnostic::new(code, severity, message);
        if let Some(span) = diagnostic.span {
            mapped.span = Some(Span::new(files[span.file as usize], span.start, span.end));
        }
        mapped
    }
}

impl Contributor for Package {
    fn describe(&self) -> Result<Vec<command::Descriptor>, String> {
        Ok(self.backend.commands())
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::run(&self.backend, invocation)
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::resume(&self.backend, state, outputs, answers)
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        Backend::open(&self.backend, invocation)
    }

    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: &[command::Answer],
        cancel: &Cancellation,
    ) -> Result<command::Handled, String> {
        self.backend
            .handle_cancellable(&state, &request, answers, cancel)
    }

    fn close(&self, state: Vec<u8>) -> command::Plan {
        Backend::close(&self.backend, state)
    }
}

fn reserved_outputs(
    prepared: &Prepared,
    output: &BuildPlan,
    project: Option<&Project>,
) -> Vec<String> {
    let mut reserved: Vec<_> = prepared.shared.iter().map(|a| a.path.clone()).collect();
    reserved.push(launch::FILE.into());
    if output.extras.schema_aot {
        reserved.extend(output.entries.iter().map(|(name, _)| schema_aot_file(name)));
    }
    if project.is_none() {
        reserved.push(FILES.into());
    }
    reserved
}

fn required_launches(
    result: &flick_backend::BuildResult,
    output: &BuildPlan,
    require_runs: bool,
) -> Result<(), String> {
    for (name, kind) in output
        .entries
        .iter()
        .filter(|_| require_runs)
        .map(|(name, _)| (name.as_str(), flick_backend::LaunchKind::Run))
        .chain(
            (output.purpose == Purpose::Test).then_some(("test", flick_backend::LaunchKind::Test)),
        )
    {
        if !result
            .launches
            .iter()
            .any(|l| l.name == name && l.kind == kind)
        {
            return Err(format!(
                "backend supplied no {kind:?} launch named `{name}`"
            ));
        }
    }
    Ok(())
}

fn host_preflight(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Vec<Diagnostic> {
    if output.extras.schema_aot && !output.backend.supports_host("host-schema-aot") {
        return vec![Diagnostic::new(
            codes::P012,
            Severity::Error,
            "component schema AOT requires an approved pinned host tool adapter",
        )];
    }
    let Some(project) = project.filter(|_| !output.go) else {
        return vec![];
    };
    let mut diagnostics = js::conflicts(host, &project.root, &compilation.js_dependencies);
    diagnostics.extend(js::unmet(host, &project.root, &compilation.js_dependencies));
    diagnostics
}

fn standalone_files(emission: &mut Emission) {
    if !emission.diagnostics.is_empty() {
        return;
    }
    let files = emission
        .artifacts
        .iter()
        .fold(String::new(), |mut files, artifact| {
            files.push_str(&artifact.path);
            files.push('\n');
            files
        });
    emission
        .artifacts
        .push(Artifact::source(FILES.into(), files));
}
