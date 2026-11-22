//! Entry launchers of the deliberately small fixture backend.

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Launch, LaunchKind, OptionValue};

pub(super) fn run(request: &BuildRequest, artifacts: &mut Vec<Artifact>) -> Vec<Launch> {
    let mut launches = vec![];
    let case = super::case(request);
    if let Some(node) = request.toolchains.iter().find(|t| t.id == "node") {
        for entry in &request.entries {
            let module = request
                .modules
                .iter()
                .find(|m| m.name == entry.symbol.module)
                .expect("entry module");
            let id = format!("launch:{}", entry.name);
            artifacts.push(Artifact {
                id: id.clone(),
                path: format!("invoke-{}.mjs", entry.name),
                kind: ArtifactKind::Source,
                bytes: format!(
                    "import {{ main }} from {};\nmain(undefined);\n",
                    serde_json::to_string(&format!("./{}", module.output)).expect("JS string")
                )
                .into_bytes(),
            });
            let runtime_arguments = if case == "bad-launch-arg" {
                vec!["-e".into(), "process.exit(1)".into()]
            } else {
                vec!["--enable-source-maps".into()]
            };
            let environment = if case == "bad-launch-env" {
                vec![OptionValue {
                    key: "LD_PRELOAD".into(),
                    value: "/evil.so".into(),
                }]
            } else {
                vec![]
            };
            launches.push(Launch {
                name: entry.name.clone(),
                kind: LaunchKind::Run,
                artifact: id,
                runtime: Some(node.clone()),
                runtime_arguments,
                arguments: vec![],
                environment,
            });
        }
    }
    launches
}
