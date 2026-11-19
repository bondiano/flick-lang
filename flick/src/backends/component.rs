//! A verified package invocation. No component failure calls native codegen.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{BackendId, BackendPin, Failure, Limits, Policy};
use flick_core::build::go::FILES;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::BackendPackage;
use flick_core::project::{Compilation, Project, js};
use flick_core::source::{FileId, SourceMap, Span};

use super::request::Prepared;
use super::{failure, launch};
use crate::artifact::Artifact;
use crate::commands::build::Purpose;
use crate::commands::build::{BuildPlan, Emission};
use crate::component;
use crate::sources::FsHost;

pub(crate) struct Package {
    pin: BackendPin,
    root: PathBuf,
    backend: WasmBackend,
    tools: Vec<launch::Tool>,
    runtimes: Vec<flick_backend::RuntimeDependency>,
}

impl Package {
    pub(super) fn load(
        project: &Project,
        package: &BackendPackage,
        rt: bool,
    ) -> Result<Self, Diagnostic> {
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
        Self::from_bytes(&project.root, pin, &bytes, rt)
    }

    pub(super) fn from_bytes(
        root: &Path,
        pin: BackendPin,
        bytes: &[u8],
        rt: bool,
    ) -> Result<Self, Diagnostic> {
        let backend = WasmBackend::load(
            component::engine().map_err(failure)?,
            pin.clone(),
            bytes,
            Budget::default(),
        )
        .map_err(|why| {
            failure(format!(
                "backend {}@{}#{}: {why}",
                pin.identity.id, pin.identity.version, pin.sha256
            ))
        })?;
        let tools = launch::Tool::node().into_iter().collect();
        let runtimes = super::dependencies::installed_for(root, !rt).map_err(failure)?;
        Ok(Self {
            pin,
            root: root.to_path_buf(),
            backend,
            tools,
            runtimes,
        })
    }

    pub(super) fn pin(&self) -> BackendPin {
        self.pin.clone()
    }

    pub(crate) fn artifacts(
        &self,
        host: &FsHost,
        sources: &mut SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
    ) -> Emission {
        let diagnostics = dependency_checks(host, project, compilation, output.go);
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
        let bridge = prepared
            .request
            .target
            .options
            .iter()
            .find(|o| o.key == "js.effect-bridge");
        let effect = prepared
            .request
            .target
            .options
            .iter()
            .any(|o| o.key == "js.runtime" && o.value == "effect")
            || bridge.is_some_and(|o| o.value != "absent");
        prepared.request.runtime_dependencies =
            match super::dependencies::installed_for(&self.root, effect) {
                Ok(pins) => pins,
                Err(why) => {
                    return Emission {
                        artifacts: vec![],
                        diagnostics: vec![failure(why)],
                    };
                }
            };
        prepared.request.toolchains = self.tools.iter().map(|t| t.pin.clone()).collect();
        let mut reserved: Vec<_> = prepared.shared.iter().map(|a| a.path.clone()).collect();
        reserved.push(launch::FILE.into());
        if project.is_none() {
            reserved.push(FILES.into());
        }
        let policy = Policy {
            toolchains: &prepared.request.toolchains,
            environment: &[],
            reserved_paths: &reserved,
            limits: Limits::default(),
            authorize_step: &|_| Err("no host tool adapter is enabled for this backend".into()),
        };
        let mut emission = match flick_backend::generate(
            &self.backend,
            &prepared.request,
            &policy,
            |id| prepared.bytes.get(id).cloned(),
            true,
        ) {
            Ok(generated) => {
                let result = generated.result();
                if let Err(why) = required_launches(result, output) {
                    return Emission {
                        artifacts: vec![],
                        diagnostics: vec![failure(why)],
                    };
                }
                match launch::artifact(&self.pin, result) {
                    Ok(manifest) => prepared.shared.push(manifest),
                    Err(why) => {
                        return Emission {
                            artifacts: vec![],
                            diagnostics: vec![failure(why)],
                        };
                    }
                }
                prepared
                    .shared
                    .extend(result.artifacts.iter().cloned().map(Artifact::from));
                Emission {
                    artifacts: prepared.shared,
                    diagnostics: result
                        .diagnostics
                        .iter()
                        .map(|d| self.diagnostic(d, &prepared.files))
                        .collect(),
                }
            }
            Err(Failure::Rejected(diagnostics)) => Emission {
                artifacts: vec![],
                diagnostics: diagnostics
                    .iter()
                    .map(|d| self.diagnostic(d, &prepared.files))
                    .collect(),
            },
            Err(why) => Emission {
                artifacts: vec![],
                diagnostics: vec![failure(format!(
                    "backend {}@{}#{}: {why}",
                    self.pin.identity.id, self.pin.identity.version, self.pin.sha256
                ))],
            },
        };
        if project.is_none() {
            standalone_files(&mut emission);
        }
        emission.diagnostics.splice(0..0, prepared.diagnostics);
        emission
    }

    pub(super) fn run_paths(&self, files: &[Artifact]) -> Vec<(String, String)> {
        launch::run_paths(files, &self.pin)
    }

    pub(super) fn cache_identity(&self) -> String {
        format!(
            "{:?} {:?} {:?}",
            self.pin,
            self.runtimes,
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

fn required_launches(
    result: &flick_backend::BuildResult,
    output: &BuildPlan,
) -> Result<(), String> {
    for (name, kind) in output
        .entries
        .iter()
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

fn dependency_checks(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
    go: bool,
) -> Vec<Diagnostic> {
    let Some(project) = project.filter(|_| !go) else {
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
