//! The step cases: a pinned `node` copies the output of an own module. A
//! request without `node` gets a step whose pin the host never approved.

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, InputRef, StepOutput, Toolchain, ToolchainStep,
};

pub(super) fn steps(
    request: &BuildRequest,
    case: &str,
    artifacts: &[Artifact],
) -> Vec<ToolchainStep> {
    let output = match case {
        "steps" => "copied.txt",
        "steps-escape" => "../escape.txt",
        _ => return vec![],
    };
    let node = (request.toolchains.iter())
        .find(|t| t.id == "node")
        .cloned()
        .unwrap_or_else(|| Toolchain {
            id: "node".into(),
            version: "0.0.0".into(),
            sha256: "0".repeat(64),
        });
    let Some(source) = artifacts.iter().find(|a| a.kind == ArtifactKind::Source) else {
        return vec![];
    };
    vec![ToolchainStep {
        id: "copy".into(),
        dependencies: vec![],
        toolchain: node,
        arguments: vec!["copy".into(), source.path.clone(), output.into()],
        environment: vec![],
        inputs: vec![InputRef::Artifact(source.id.clone())],
        outputs: vec![StepOutput {
            id: "copied".into(),
            path: output.into(),
            kind: ArtifactKind::Resource,
        }],
    }]
}
