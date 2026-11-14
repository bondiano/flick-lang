//! Decode only declared plain inputs, without compiler objects or callbacks.

use super::failure;
use crate::codegen::{EffectBridge, Labels, Resolved, ts::Typed};
use flick_backend::{
    BuildRequest, Diagnostic, Inputs, JavascriptProjection, ModuleRole, TypescriptModuleProjection,
};
use flick_ir::{Expr, ExprKind, OpClass, Program};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Data {
    pub program: Program,
    pub facts: BTreeMap<String, JavascriptProjection>,
    pub paths: BTreeMap<String, String>,
    pub typed: BTreeMap<String, Typed>,
    typed_paths: BTreeMap<String, String>,
    pub sources: Vec<String>,
    pub packages: BTreeMap<String, String>,
    pub foreign: Vec<Resolved>,
    pub rt: bool,
    pub typescript: bool,
    pub labels: Labels,
    pub bridge: EffectBridge,
}

impl Data {
    pub fn read(request: &BuildRequest, inputs: &mut dyn Inputs) -> Result<Self, Vec<Diagnostic>> {
        let program = Program::from_cbor(&request.program).map_err(|e| failure(e.to_string()))?;
        let mut facts: BTreeMap<String, JavascriptProjection> = BTreeMap::new();
        let mut typed = BTreeMap::new();
        let mut typed_paths = BTreeMap::new();
        for projection in &request.projections {
            let bytes = inputs
                .read(&projection.input)
                .map_err(|e| failure(format!("projection input: {e:?}")))?;
            match (
                projection.schema.name.as_str(),
                projection.schema.version.as_str(),
            ) {
                ("flick.javascript", "0.1.0") => {
                    let value = ciborium::from_reader(&bytes[..])
                        .map_err(|e| failure(format!("JS facts: {e}")))?;
                    if facts.insert(projection.module.clone(), value).is_some() {
                        return Err(failure("duplicate JavaScript projection"));
                    }
                }
                ("flick.typescript", "0.2.0") => {
                    let value: TypescriptModuleProjection = ciborium::from_reader(&bytes[..])
                        .map_err(|e| failure(format!("TS facts: {e}")))?;
                    for file in value.files {
                        typed_paths.insert(file.module.clone(), file.output);
                        let value = file.projection;
                        if typed
                            .insert(
                                file.module,
                                Typed {
                                    declarations: value.declarations,
                                    annotations: value
                                        .annotations
                                        .into_iter()
                                        .map(|v| (v.key, v.value))
                                        .collect(),
                                    imports: value.imports,
                                    modules: value.modules,
                                },
                            )
                            .is_some()
                        {
                            return Err(failure("duplicate physical TypeScript projection"));
                        }
                    }
                }
                _ => {
                    return Err(failure(format!(
                        "unsupported projection {}@{}",
                        projection.schema.name, projection.schema.version
                    )));
                }
            }
        }
        let paths = physical_paths(request, &facts)?;
        let sources = request
            .sources
            .iter()
            .map(|source| {
                let bytes = inputs
                    .read(&source.input)
                    .map_err(|e| failure(format!("source input: {e:?}")))?;
                String::from_utf8(bytes).map_err(|e| failure(format!("source encoding: {e}")))
            })
            .collect::<Result<_, _>>()?;
        let (rt, bridge, labels) = runtime_options(request)?;
        let typescript = request
            .target
            .required_capabilities
            .iter()
            .any(|c| c.name == "typescript");
        let packages = request
            .modules
            .iter()
            .filter_map(|m| m.package.as_ref().map(|p| (m.name.clone(), p.clone())))
            .collect();
        let foreign = foreign_bindings(request)?;
        Ok(Self {
            program,
            facts,
            paths,
            typed,
            typed_paths,
            sources,
            packages,
            foreign,
            rt,
            typescript,
            labels,
            bridge,
        })
    }

    pub fn check(&self, request: &BuildRequest) -> Result<(), Vec<Diagnostic>> {
        if request.sources.len() != self.program.files.len()
            || self.sources.len() != request.sources.len()
        {
            return Err(failure("source table disagrees with the portable program"));
        }
        for module in &self.program.modules {
            let meta = request
                .modules
                .iter()
                .find(|m| m.name == module.name)
                .ok_or_else(|| failure("IR module has no metadata"))?;
            let facts = self.facts.get(&module.name).ok_or_else(|| {
                failure(format!(
                    "missing flick.javascript@0.1.0 for {}",
                    module.name
                ))
            })?;
            let mut names = BTreeSet::new();
            for file in &facts.files {
                if !names.insert(&file.module) || file.source_paths.len() != self.sources.len() {
                    return Err(failure(
                        "inconsistent JavaScript file layout or source map paths",
                    ));
                }
                if self.typescript && self.typed_paths.get(&file.module) != Some(&file.output) {
                    return Err(failure(format!(
                        "missing TypeScript projection for {}",
                        file.module
                    )));
                }
            }
            if !facts
                .files
                .iter()
                .any(|f| f.module == module.name && f.output == meta.output)
            {
                return Err(failure(
                    "module output disagrees with JavaScript projection",
                ));
            }
            if !self.rt && meta.role == ModuleRole::Own {
                let mut diagnostics = Vec::new();
                for item in &module.items {
                    item.each_body(&mut |e| control(e, &mut diagnostics));
                }
                if !diagnostics.is_empty() {
                    return Err(diagnostics);
                }
            }
        }
        for entry in &request.entries {
            let facts = self
                .facts
                .get(&entry.symbol.module)
                .and_then(|f| f.entries.iter().find(|e| e.name == entry.name))
                .ok_or_else(|| failure("missing entry classification"))?;
            if !request.modules.iter().any(|m| m.name == facts.module) {
                return Err(failure("entry defining module is outside the graph"));
            }
        }
        Ok(())
    }
}

fn control(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    if let ExprKind::Handle(handler) = &expr.kind
        && handler
            .effects
            .iter()
            .any(|h| h.operations.iter().any(|o| o.class == OpClass::Control))
    {
        let mut diagnostic =
            failure("handlers of `control` operations are checked, but compiled only to Go yet")
                .remove(0);
        diagnostic.span = Some(flick_backend::Span {
            file: expr.span.file.index(),
            start: expr.span.start,
            end: expr.span.end,
        });
        diagnostics.push(diagnostic);
    }
    expr.each_child(&mut |e| control(e, diagnostics));
}

fn runtime_options(
    request: &BuildRequest,
) -> Result<(bool, EffectBridge, Labels), Vec<Diagnostic>> {
    let option = |key| {
        request
            .target
            .options
            .iter()
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    let rt = match option("js.runtime") {
        Some("rt") => true,
        Some("effect") => false,
        _ => return Err(failure("missing or unknown js.runtime")),
    };
    let bridge = match option("js.effect-bridge") {
        Some("absent") => EffectBridge::Absent,
        Some("available") => EffectBridge::Available,
        Some("available-with-ui") => EffectBridge::AvailableWithUi,
        Some("available-with-web") => EffectBridge::AvailableWithWeb,
        Some("available-with-ui-and-web") => EffectBridge::AvailableWithUiAndWeb,
        _ => return Err(failure("missing or unknown js.effect-bridge")),
    };
    let labels = match option("labels") {
        Some("all") => Labels::All,
        Some("components") => Labels::Components,
        Some("none") => Labels::None,
        _ => return Err(failure("missing or unknown labels")),
    };
    let mut seen = BTreeSet::new();
    for option in &request.target.options {
        if !seen.insert(&option.key)
            || !matches!(
                option.key.as_str(),
                "command" | "js.runtime" | "js.effect-bridge" | "labels"
            )
        {
            return Err(failure(format!(
                "unsupported or duplicate JS option {}",
                option.key
            )));
        }
    }
    if !matches!(option("command"), Some("build" | "test")) {
        return Err(failure("missing or unknown command"));
    }
    Ok((rt, bridge, labels))
}

fn foreign_bindings(request: &BuildRequest) -> Result<Vec<Resolved>, Vec<Diagnostic>> {
    let mut foreign = Vec::new();
    for binding in &request.foreign {
        if binding.abi.name != "flick.javascript-ffi" || binding.abi.version != "0.1.0" {
            return Err(failure("unsupported foreign binding ABI"));
        }
        foreign.push(Resolved {
            module: binding.module.clone(),
            spec: binding.specifier.clone(),
            specifier: binding.resolved.clone(),
        });
    }
    Ok(foreign)
}

fn physical_paths(
    request: &BuildRequest,
    facts: &BTreeMap<String, JavascriptProjection>,
) -> Result<BTreeMap<String, String>, Vec<Diagnostic>> {
    let mut paths = BTreeMap::new();
    for module in &request.modules {
        paths.insert(module.name.clone(), module.output.clone());
    }
    for facts in facts.values() {
        for file in &facts.files {
            if paths
                .insert(file.module.clone(), file.output.clone())
                .is_some_and(|old| old != file.output)
            {
                return Err(failure("conflicting physical JavaScript output paths"));
            }
        }
    }
    Ok(paths)
}
