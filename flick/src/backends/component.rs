//! A verified package invocation. No component failure calls native codegen.

use std::fs::File;
use std::io::Read;

use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{BackendId, BackendPin, Failure, Limits, Policy};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::BackendPackage;
use flick_core::project::{Compilation, Project};
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
    backend: WasmBackend,
    tools: Vec<launch::Tool>,
    runtimes: Vec<flick_backend::RuntimeDependency>,
}

impl Package {
    pub(super) fn pin(&self) -> BackendPin {
        self.pin.clone()
    }
    pub(super) fn load(project: &Project, package: &BackendPackage) -> Result<Self, Diagnostic> {
        let pin = BackendPin {
            identity: BackendId {
                id: package.id.clone(),
                version: package.version.clone(),
            },
            sha256: package.sha256.clone(),
        };
        let load = || {
            let limit = Limits::default().artifact_bytes;
            let mut bytes = Vec::new();
            File::open(project.root.join(&package.component))
                .and_then(|f| {
                    f.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
                        .read_to_end(&mut bytes)
                })
                .map_err(|e| format!("cannot read backend component: {e}"))?;
            if bytes.len() > limit {
                return Err("backend component exceeds its file size limit".into());
            }
            WasmBackend::load(component::engine()?, pin.clone(), &bytes, Budget::default())
        };
        let backend = load().map_err(|why| {
            failure(format!(
                "backend {}@{}#{}: {why}",
                package.id, package.version, package.sha256
            ))
        })?;
        let tools = launch::Tool::node(None).into_iter().collect();
        let runtimes = super::dependencies::installed(&project.root).map_err(failure)?;
        Ok(Self {
            pin,
            backend,
            tools,
            runtimes,
        })
    }

    pub(crate) fn artifacts(
        &self,
        host: &FsHost,
        sources: &SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
    ) -> Emission {
        if output.extras.schema_aot {
            return Emission {
                artifacts: vec![],
                diagnostics: vec![Diagnostic::new(
                    codes::P012,
                    Severity::Error,
                    "component schema AOT requires an approved pinned host tool adapter",
                )],
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
        prepared
            .request
            .runtime_dependencies
            .clone_from(&self.runtimes);
        prepared.request.toolchains = self.tools.iter().map(|t| t.pin.clone()).collect();
        let mut reserved: Vec<_> = prepared.shared.iter().map(|a| a.path.clone()).collect();
        reserved.push(launch::FILE.into());
        let policy = Policy {
            toolchains: &prepared.request.toolchains,
            environment: &[],
            reserved_paths: &reserved,
            limits: Limits::default(),
            authorize_step: &|_| Err("no host tool adapter is enabled for this backend".into()),
        };
        match flick_backend::generate(
            &self.backend,
            &prepared.request,
            &policy,
            |id| prepared.bytes.get(id).cloned(),
            true,
        ) {
            Ok(generated) => {
                let result = generated.result();
                for (name, kind) in output
                    .entries
                    .iter()
                    .map(|(name, _)| (name.as_str(), flick_backend::LaunchKind::Run))
                    .chain(
                        (output.purpose == Purpose::Test)
                            .then_some(("test", flick_backend::LaunchKind::Test)),
                    )
                {
                    if !result
                        .launches
                        .iter()
                        .any(|l| l.name == name && l.kind == kind)
                    {
                        return Emission {
                            artifacts: vec![],
                            diagnostics: vec![failure(format!(
                                "backend supplied no {kind:?} launch named `{name}`"
                            ))],
                        };
                    }
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
            Err(Failure::Rejected(diagnostics)) => self.rejected(&diagnostics, &prepared.files),
            Err(why) => Emission {
                artifacts: vec![],
                diagnostics: vec![failure(format!(
                    "backend {}@{}#{}: {why}",
                    self.pin.identity.id, self.pin.identity.version, self.pin.sha256
                ))],
            },
        }
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
            &self.runtimes,
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
