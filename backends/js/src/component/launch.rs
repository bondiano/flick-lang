//! Launch descriptions use an approved VM pin, never a process inside the guest.

use super::{data::Data, emit::artifact, failure, selected};
use crate::codegen::{
    Options, ident, relative,
    runner::{Start, named_runner},
};
use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, Launch, LaunchKind};
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
            None if data.rt => Start::RunMainRt,
            None => Start::RunMain,
        };
        let schema = request
            .target
            .required_capabilities
            .iter()
            .any(|c| c.name == "schema-aot")
            .then(|| {
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
            launches.push(Launch {
                name: entry.name.clone(),
                kind: LaunchKind::Run,
                artifact: output.id.clone(),
                runtime: Some(runtime.clone()),
                arguments: vec![],
            });
        }
        artifacts.push(output);
    }
    let roots: Vec<_> = request
        .test_roots
        .iter()
        .filter(|r| selected(request, &r.module))
        .collect();
    if !roots.is_empty() {
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
            .map(|(i, root)| format!("[{:?}, $s{i}.{}]", root.module, ident(&root.name)))
            .collect();
        let _ = writeln!(contents, "\nawait runTests([{}]);", list.join(", "));
        let output = artifact("test.js".into(), ArtifactKind::Source, contents);
        if let Some(runtime) = node {
            launches.push(Launch {
                name: "test".into(),
                kind: LaunchKind::Test,
                artifact: output.id.clone(),
                runtime: Some(runtime.clone()),
                arguments: vec![],
            });
        }
        artifacts.push(output);
    }
    Ok(launches)
}
