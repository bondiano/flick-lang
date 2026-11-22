//! Command contributor forwarding and host build policy helpers for a
//! loaded backend component.

use flick_backend::host::runtime::Cancellation;
use flick_backend::{Backend, command};
use flick_core::build::{go::FILES, schema_aot_file};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{Compilation, Project, target};

use super::super::launch;
use super::super::request::Prepared;
use super::Package;
use crate::artifact::Artifact;
use crate::commands::build::{BuildPlan, Emission, Purpose};
use crate::commands::contributed::Contributor;

impl Contributor for Package {
    fn describe(&self) -> Result<Vec<command::Descriptor>, String> {
        Ok(self.backend.commands())
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::run(&*self.backend, invocation)
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::resume(&*self.backend, state, outputs, answers)
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        Backend::open(&*self.backend, invocation)
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
        Backend::close(&*self.backend, state)
    }

    fn templates(&self) -> Result<Vec<command::TemplateDescriptor>, String> {
        self.backend.try_templates()
    }

    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::scaffold(&*self.backend, request)
    }
}

pub(super) fn reserved_outputs(
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

pub(super) fn required_launches(
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

pub(super) fn host_preflight(
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
    let Some(project) = project else {
        return vec![];
    };
    let mut diagnostics = target::conflicts(&compilation.target_dependencies);
    diagnostics.extend(super::super::verify_for_target(
        project,
        Some(&output.target),
        &compilation.target_dependencies,
        true,
    ));
    diagnostics
}

pub(super) fn standalone_files(emission: &mut Emission) {
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
