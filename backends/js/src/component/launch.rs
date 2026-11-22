//! Launch descriptions use an approved VM pin, never a process inside the guest.

use super::{data::Data, emit::artifact, failure, selected};
use crate::codegen::{
    Options, ident, relative,
    runner::{Start, named_runner},
};
use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, Diagnostic, Launch, LaunchKind, OptionValue,
};
use flick_backend_support::suites::suite_name;
use std::fmt::Write;

pub(super) fn generate(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    artifacts: &mut Vec<Artifact>,
) -> Result<Vec<Launch>, Vec<Diagnostic>> {
    let node = request.toolchains.iter().find(|t| t.id == "node");
    let mut launches = Vec::new();
    for entry in request
        .entries
        .iter()
        .filter(|e| selected(request, &e.symbol.module))
    {
        let facts = data
            .facts
            .get(&entry.symbol.module)
            .and_then(|f| f.entries.iter().find(|e| e.name == entry.name))
            .ok_or_else(|| failure("missing entry classification"))?;
        let file = format!(
            "flick-{}.{}",
            entry.name,
            if data.typescript { "ts" } else { "js" }
        );
        let start = match entry.runner.as_ref() {
            Some(module) => Start::Run(relative(&file, &options.path(module))),
            None if facts.pure => Start::Call,
            None if data.rt => Start::RunMainRt(facts.host.clone()),
            None => Start::RunMain(facts.host.clone()),
        };
        let schema = data.schema_aot.then(|| {
            if entry.name == "main" {
                "schemas.aot.js".into()
            } else {
                format!("schemas.{}.aot.js", entry.name)
            }
        });
        let contents = named_runner(
            &relative(&file, &options.path(&facts.module)),
            &ident(&entry.symbol.name),
            &start,
            schema.as_deref(),
        );
        let output = artifact(file, ArtifactKind::Source, contents);
        if let Some(runtime) = node {
            let mut runtime_arguments = vec!["--enable-source-maps".into()];
            let mut environment = vec![];
            if let Some(url) = &data.devtools {
                let import = if data.rt {
                    "@flick-lang/runtime/rt/runner/devtools"
                } else {
                    "@flick-lang/effect-ts/runner/devtools"
                };
                runtime_arguments.push("--import".into());
                runtime_arguments.push(import.into());
                environment.push(OptionValue {
                    key: "FLICK_DEVTOOLS".into(),
                    value: url.clone(),
                });
            }
            launches.push(Launch {
                name: entry.name.clone(),
                kind: LaunchKind::Run,
                artifact: output.id.clone(),
                runtime: Some(runtime.clone()),
                runtime_arguments,
                arguments: vec![],
                environment,
            });
        }
        artifacts.push(output);
    }
    if request
        .target
        .options
        .iter()
        .any(|o| o.key == "command" && o.value == "test")
    {
        generate_test_launch(request, data, options, node, artifacts, &mut launches);
    }
    Ok(launches)
}

fn generate_test_launch(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    node: Option<&flick_backend::Toolchain>,
    artifacts: &mut Vec<Artifact>,
    launches: &mut Vec<Launch>,
) {
    let roots: Vec<_> = request
        .test_roots
        .iter()
        .filter(|r| selected(request, &r.module))
        .collect();
    let runtime = if data.rt {
        "@flick-lang/runtime/rt"
    } else {
        "@flick-lang/effect-ts"
    };
    let mut contents = format!("import {{ runTests }} from \"{runtime}\";\n");
    for (i, root) in roots.iter().enumerate() {
        let path = relative("test.js", &options.path(&root.module));
        let _ = writeln!(contents, "import * as $s{i} from {path:?};");
    }
    let list: Vec<_> = roots
        .iter()
        .enumerate()
        .map(|(i, root)| {
            let module = suite_name(&root.module);
            format!("[{module:?}, $s{i}.{}]", ident(&root.name))
        })
        .collect();
    let _ = writeln!(contents, "\nawait runTests([{}]);", list.join(", "));
    let output = artifact("test.js".into(), ArtifactKind::Source, contents);
    if let Some(runtime) = node {
        launches.push(Launch {
            name: "test".into(),
            kind: LaunchKind::Test,
            artifact: output.id.clone(),
            runtime: Some(runtime.clone()),
            runtime_arguments: vec!["--enable-source-maps".into()],
            arguments: vec![],
            environment: vec![],
        });
    }
    artifacts.push(output);
}
