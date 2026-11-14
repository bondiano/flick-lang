//! Target generation consumes the reference program and explicit projections.

use super::{data::Data, failure, launch, optimize, selected};
use crate::codegen::{self, Known, Options, RuntimeLayout};
use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, BuildResult, Diagnostic, ModuleRole, Profile,
};
use flick_ir::Module;
use std::collections::HashSet;

pub(super) fn generate(
    request: &BuildRequest,
    data: &Data,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let builtins: Vec<_> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Builtin)
        .map(|m| m.name.as_str())
        .collect();
    let lang = Known::of(&data.program.lang);
    let (workers, direct) = optimize::exports(request, data);
    let builtin_paths: Vec<_> = request
        .modules
        .iter()
        .filter(|m| m.output.starts_with("_flick/"))
        .map(|m| m.name.as_str())
        .collect();
    let options = Options {
        builtins: &builtin_paths,
        paths: Some(&data.paths),
        packages: &data.packages,
        resolved: &data.foreign,
        typescript: data.typescript.then_some(&data.typed),
        release: request.target.profile == Profile::Release,
        workers: &workers,
        direct: &direct,
        labels: data.labels,
        rt: data.rt,
        effect_bridge: data.bridge,
        lang: &lang,
    };
    let own = |name: &str| {
        request
            .modules
            .iter()
            .any(|m| m.name == name && m.role == ModuleRole::Own)
    };
    let layout = RuntimeLayout::new(data.program.modules.iter().filter(|m| own(&m.name)));
    let mut artifacts = Vec::new();
    let mut pending: Vec<_> = request
        .entries
        .iter()
        .filter(|e| selected(request, &e.symbol.module))
        .filter_map(|e| e.runner.clone())
        .collect();
    for module in &data.program.modules {
        if !own(&module.name) || !selected(request, &module.name) {
            continue;
        }
        let optimized = (request.target.profile == Profile::Release)
            .then(|| optimize::module(request, data, module, &lang));
        let module = optimized.as_ref().unwrap_or(module);
        for (physical, output) in codegen::js_parts(module, &options, &layout) {
            pending.extend(output.imports.iter().cloned());
            let facts = data.facts[&module.name]
                .files
                .iter()
                .find(|f| f.module == physical)
                .ok_or_else(|| failure("missing physical JavaScript file projection"))?;
            mapped(request, data, facts, &output, &mut artifacts);
        }
    }
    if request
        .target
        .required_capabilities
        .iter()
        .any(|c| c.name == "declarations")
    {
        pending = builtins.iter().map(|m| (*m).to_owned()).collect();
    }
    let mut written = HashSet::new();
    while let Some(name) = pending.pop() {
        if !builtins.contains(&name.as_str()) || !written.insert(name.clone()) {
            continue;
        }
        let module: &Module = data
            .program
            .modules
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| failure("missing builtin IR"))?;
        let output = codegen::js(module, &options);
        pending.extend(output.imports);
        artifacts.push(artifact(
            options.path(&name),
            ArtifactKind::Source,
            output.code,
        ));
    }
    let launches = launch::generate(request, data, &options, &mut artifacts)?;
    artifacts.push(package_json(request, data.typescript));
    Ok(BuildResult {
        artifacts,
        steps: vec![],
        runtime_dependencies: request.runtime_dependencies.clone(),
        launches,
        diagnostics: vec![],
    })
}

pub(super) fn artifact(path: String, kind: ArtifactKind, text: String) -> Artifact {
    Artifact {
        id: format!("js:{path}"),
        path,
        kind,
        bytes: text.into_bytes(),
    }
}

fn mapped(
    request: &BuildRequest,
    data: &Data,
    facts: &flick_backend::JavascriptFile,
    output: &codegen::Output,
    artifacts: &mut Vec<Artifact>,
) {
    let file = &facts.output;
    let base = file.rsplit('/').next().unwrap_or(file);
    let source_path = |id: flick_ir::FileId| facts.source_paths[id.index() as usize].clone();
    let position = |span: flick_ir::Span| {
        let source = &request.sources[span.file.index() as usize];
        let text = &data.sources[span.file.index() as usize];
        let line = source
            .line_starts
            .partition_point(|&start| start <= span.start)
            .saturating_sub(1);
        let start = source.line_starts[line] as usize;
        (
            u32::try_from(line).expect("source line count"),
            text[start..span.start as usize].encode_utf16().count(),
        )
    };
    let map = codegen::sourcemap::source_map(base, output, &source_path, &position);
    artifacts.push(artifact(
        file.clone(),
        ArtifactKind::Source,
        format!("{}//# sourceMappingURL={base}.map\n", output.code),
    ));
    artifacts.push(artifact(
        format!("{file}.map"),
        ArtifactKind::SourceMap,
        map,
    ));
}

fn package_json(request: &BuildRequest, typescript: bool) -> Artifact {
    let ext = if typescript { "ts" } else { "js" };
    let runners: Vec<_> = request
        .entries
        .iter()
        .map(|e| format!("\"./flick-{}.{ext}\"", e.name))
        .collect();
    let side_effects = if runners.is_empty() {
        "false".into()
    } else {
        format!("[{}]", runners.join(", "))
    };
    artifact(
        "package.json".into(),
        ArtifactKind::Metadata,
        format!("{{ \"type\": \"module\", \"sideEffects\": {side_effects} }}\n"),
    )
}
