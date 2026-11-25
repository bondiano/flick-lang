//! JS library artifact emission (`[target.js] kind = "library"`):
//! ESM (`index.js`), `CommonJS` (`index.cjs`), and self-contained IIFE
//! (`index.iife.js`), each exporting every `@export "name"` under its host name.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use flick_backend::{Artifact, ArtifactKind, BuildRequest, BuildResult, Diagnostic};
use flick_backend_support::opt::eval::ReviewedPrimitives;
use flick_ir::Module;

use super::data::{Data, LibraryFormat};
use super::emit::{artifact, mapped_with_path};
use super::{failure, optimize, selected};
use crate::codegen::{self, Known, ModuleFormat, Options, RuntimeLayout, names, relative};

const DEFAULT_GLOBAL_NAME: &str = "FlickLib";

pub(super) fn generate(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    builtins: &[&str],
    own: &dyn Fn(&str) -> bool,
    layout: &RuntimeLayout,
    lang: &Known<'_>,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let primitives = (request.target.profile == flick_backend::Profile::Release)
        .then(|| optimize::primitives(request, data, lang));
    let has_headers = request.headers.as_ref().is_some_and(|h| h.enabled);

    match data.format {
        LibraryFormat::Esm => generate_esm(
            request,
            data,
            options,
            builtins,
            own,
            layout,
            lang,
            primitives.as_ref(),
            has_headers,
        ),
        LibraryFormat::Cjs => generate_cjs(
            request,
            data,
            options,
            builtins,
            own,
            layout,
            lang,
            primitives.as_ref(),
            has_headers,
        ),
        LibraryFormat::Iife => generate_iife(
            request,
            data,
            options,
            builtins,
            own,
            layout,
            lang,
            primitives.as_ref(),
            has_headers,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_esm(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    builtins: &[&str],
    own: &dyn Fn(&str) -> bool,
    layout: &RuntimeLayout,
    lang: &Known<'_>,
    primitives: Option<&ReviewedPrimitives>,
    has_headers: bool,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let entry_path = if data.typescript {
        "index.ts"
    } else {
        "index.js"
    };
    let mut artifacts = Vec::new();
    let mut pending = Vec::new();
    for module in &data.program.modules {
        if !own(&module.name) || !selected(request, &module.name) {
            continue;
        }
        let optimized =
            primitives.map(|prims| optimize::module(request, data, module, lang, prims));
        let module = optimized.as_ref().unwrap_or(module);
        for (physical, output) in codegen::js_parts(module, options, layout) {
            pending.extend(output.imports.iter().cloned());
            let facts = data.facts[&module.name]
                .files
                .iter()
                .find(|f| f.module == physical)
                .ok_or_else(|| failure("missing physical JavaScript file projection"))?;
            if facts.output.eq_ignore_ascii_case(entry_path) {
                return Err(failure(format!(
                    "library entry `{entry_path}` collides with module output `{}`: rename the module",
                    facts.output
                )));
            }
            mapped_with_path(request, data, facts, &facts.output, &output, &mut artifacts);
        }
    }
    if data.declarations {
        pending = builtins.iter().map(|m| (*m).to_owned()).collect();
    }
    emit_builtins_with_format(
        data,
        builtins,
        options,
        ModuleFormat::Esm,
        &mut pending,
        &mut artifacts,
    )?;
    artifacts.push(esm_entry(request, options, entry_path));
    artifacts.push(library_package_json(
        LibraryFormat::Esm,
        entry_path,
        has_headers,
    ));
    Ok(BuildResult {
        artifacts,
        steps: vec![],
        runtime_dependencies: request.runtime_dependencies.clone(),
        launches: vec![],
        diagnostics: vec![],
    })
}

#[allow(clippy::too_many_arguments)]
fn generate_cjs(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    builtins: &[&str],
    own: &dyn Fn(&str) -> bool,
    layout: &RuntimeLayout,
    lang: &Known<'_>,
    primitives: Option<&ReviewedPrimitives>,
    has_headers: bool,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let cjs_paths: BTreeMap<String, String> = data
        .paths
        .iter()
        .map(|(module, path)| (module.clone(), to_cjs_path(path)))
        .collect();
    let cjs_options = Options {
        paths: Some(&cjs_paths),
        ..*options
    };
    let entry_path = "index.cjs";
    let mut artifacts = Vec::new();
    let mut pending = Vec::new();
    for module in &data.program.modules {
        if !own(&module.name) || !selected(request, &module.name) {
            continue;
        }
        let optimized =
            primitives.map(|prims| optimize::module(request, data, module, lang, prims));
        let module = optimized.as_ref().unwrap_or(module);
        for (physical, output) in
            codegen::js_parts_with_format(module, &cjs_options, layout, ModuleFormat::Cjs)
        {
            pending.extend(output.imports.iter().cloned());
            let facts = data.facts[&module.name]
                .files
                .iter()
                .find(|f| f.module == physical)
                .ok_or_else(|| failure("missing physical JavaScript file projection"))?;
            let path = to_cjs_path(&facts.output);
            if path.eq_ignore_ascii_case(entry_path) {
                return Err(failure(format!(
                    "library entry `{entry_path}` collides with module output `{path}`: rename the module"
                )));
            }
            mapped_with_path(request, data, facts, &path, &output, &mut artifacts);
        }
    }
    emit_builtins_with_format(
        data,
        builtins,
        &cjs_options,
        ModuleFormat::Cjs,
        &mut pending,
        &mut artifacts,
    )?;
    artifacts.push(cjs_entry(request, &cjs_options, entry_path));
    artifacts.push(library_package_json(
        LibraryFormat::Cjs,
        entry_path,
        has_headers,
    ));
    Ok(BuildResult {
        artifacts,
        steps: vec![],
        runtime_dependencies: request.runtime_dependencies.clone(),
        launches: vec![],
        diagnostics: vec![],
    })
}

#[allow(clippy::too_many_arguments)]
fn generate_iife(
    request: &BuildRequest,
    data: &Data,
    options: &Options<'_>,
    builtins: &[&str],
    own: &dyn Fn(&str) -> bool,
    layout: &RuntimeLayout,
    lang: &Known<'_>,
    primitives: Option<&ReviewedPrimitives>,
    has_headers: bool,
) -> Result<BuildResult, Vec<Diagnostic>> {
    let mut bundled: BTreeMap<String, String> = BTreeMap::new();
    let mut pending = Vec::new();
    for module in &data.program.modules {
        if !own(&module.name) || !selected(request, &module.name) {
            continue;
        }
        let optimized =
            primitives.map(|prims| optimize::module(request, data, module, lang, prims));
        let module = optimized.as_ref().unwrap_or(module);
        for (physical, output) in
            codegen::js_parts_with_format(module, options, layout, ModuleFormat::Iife)
        {
            pending.extend(output.imports.iter().cloned());
            bundled.insert(physical, output.code);
        }
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
        let output = codegen::js_with_format(module, options, ModuleFormat::Iife);
        pending.extend(output.imports);
        bundled.insert(name, output.code);
    }

    let global_name = data.global_name.as_deref().unwrap_or(DEFAULT_GLOBAL_NAME);
    let entry_path = "index.iife.js";
    let code = format_iife_bundle(&bundled, request, global_name);

    let artifacts = vec![
        artifact(entry_path.into(), ArtifactKind::Source, code),
        library_package_json(LibraryFormat::Iife, entry_path, has_headers),
    ];
    Ok(BuildResult {
        artifacts,
        steps: vec![],
        runtime_dependencies: request.runtime_dependencies.clone(),
        launches: vec![],
        diagnostics: vec![],
    })
}

fn format_iife_bundle(
    bundled: &BTreeMap<String, String>,
    request: &BuildRequest,
    global_name: &str,
) -> String {
    let mut code = String::new();
    let _ = writeln!(code, "(function (global) {{");
    let _ = writeln!(code, "  \"use strict\";");
    let _ = writeln!(code, "  const __modules = {{");
    for (module_key, body) in bundled {
        let _ = writeln!(
            code,
            "    {}: function ($req, $exp) {{",
            names::string(module_key)
        );
        for line in body.lines() {
            if line.is_empty() {
                code.push('\n');
            } else {
                let _ = writeln!(code, "      {line}");
            }
        }
        let _ = writeln!(code, "    }},");
    }
    let _ = writeln!(code, "  }};");
    let _ = writeln!(code, "  const __cache = Object.create(null);");
    let _ = writeln!(code, "  function __require(id) {{");
    let _ = writeln!(
        code,
        "    if (Object.prototype.hasOwnProperty.call(__cache, id)) return __cache[id];"
    );
    let _ = writeln!(
        code,
        "    if (Object.prototype.hasOwnProperty.call(__modules, id)) {{"
    );
    let _ = writeln!(code, "      const exports = {{}};");
    let _ = writeln!(code, "      __cache[id] = exports;");
    let _ = writeln!(code, "      __modules[id](__require, exports);");
    let _ = writeln!(code, "      return exports;");
    let _ = writeln!(code, "    }}");
    let _ = writeln!(
        code,
        "    if (typeof require === \"function\") return require(id);"
    );
    let _ = writeln!(
        code,
        "    throw new Error(\"Flick IIFE library cannot resolve external module: \" + id);"
    );
    let _ = writeln!(code, "  }}");
    let _ = writeln!(code, "  const __exports = {{");
    for export in &request.exports {
        let js_sym = names::ident(&export.symbol.name);
        let _ = writeln!(
            code,
            "    {}: __require({}).{js_sym},",
            export.name,
            names::string(&export.symbol.module)
        );
    }
    let _ = writeln!(code, "  }};");
    let _ = writeln!(code, "  global.{global_name} = __exports;");
    let _ = writeln!(
        code,
        "}})(typeof globalThis !== \"undefined\" ? globalThis : this);"
    );
    code
}

fn emit_builtins_with_format(
    data: &Data,
    builtins: &[&str],
    options: &Options<'_>,
    format: ModuleFormat,
    pending: &mut Vec<String>,
    artifacts: &mut Vec<Artifact>,
) -> Result<(), Vec<Diagnostic>> {
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
        let output = codegen::js_with_format(module, options, format);
        pending.extend(output.imports);
        artifacts.push(artifact(
            options.path(&name),
            ArtifactKind::Source,
            output.code,
        ));
    }
    Ok(())
}

fn esm_entry(request: &BuildRequest, options: &Options<'_>, entry_path: &str) -> Artifact {
    let mut code = String::new();
    if request.exports.is_empty() {
        code.push_str("export {};\n");
    } else {
        let mut by_module: BTreeMap<&str, Vec<(&str, String)>> = BTreeMap::new();
        for export in &request.exports {
            by_module
                .entry(export.symbol.module.as_str())
                .or_default()
                .push((export.name.as_str(), names::ident(&export.symbol.name)));
        }
        let mut exported_names = Vec::new();
        for (i, (module, items)) in by_module.into_iter().enumerate() {
            let from = relative(entry_path, &options.path(module));
            let _ = writeln!(code, "import * as $m{i} from {};", names::string(&from));
            for (host_name, js_sym) in items {
                let _ = writeln!(code, "const $e_{host_name} = $m{i}.{js_sym};");
                exported_names.push(format!("$e_{host_name} as {host_name}"));
            }
        }
        let _ = writeln!(code, "export {{ {} }};", exported_names.join(", "));
    }
    artifact(entry_path.into(), ArtifactKind::Source, code)
}

fn cjs_entry(request: &BuildRequest, options: &Options<'_>, entry_path: &str) -> Artifact {
    let mut code = String::from("\"use strict\";\n");
    let mut by_module: BTreeMap<&str, Vec<(&str, String)>> = BTreeMap::new();
    for export in &request.exports {
        by_module
            .entry(export.symbol.module.as_str())
            .or_default()
            .push((export.name.as_str(), names::ident(&export.symbol.name)));
    }
    for (i, (module, items)) in by_module.into_iter().enumerate() {
        let from = relative(entry_path, &options.path(module));
        let _ = writeln!(code, "const $m{i} = require({});", names::string(&from));
        for (host_name, js_sym) in items {
            let _ = writeln!(code, "exports.{host_name} = $m{i}.{js_sym};");
        }
    }
    artifact(entry_path.into(), ArtifactKind::Source, code)
}

fn to_cjs_path(path: &str) -> String {
    path.strip_suffix(".js")
        .map_or_else(|| format!("{path}.cjs"), |stem| format!("{stem}.cjs"))
}

fn library_package_json(format: LibraryFormat, entry_path: &str, has_headers: bool) -> Artifact {
    let entry_rel = format!("./{entry_path}");
    let json = match format {
        LibraryFormat::Esm => {
            if has_headers {
                serde_json::json!({
                    "type": "module",
                    "main": entry_rel,
                    "types": "./index.d.ts",
                    "exports": {
                        ".": {
                            "types": "./index.d.ts",
                            "import": entry_rel,
                        }
                    },
                    "sideEffects": false,
                })
            } else {
                serde_json::json!({
                    "type": "module",
                    "main": entry_rel,
                    "exports": {
                        ".": {
                            "import": entry_rel,
                        }
                    },
                    "sideEffects": false,
                })
            }
        }
        LibraryFormat::Cjs => {
            if has_headers {
                serde_json::json!({
                    "type": "commonjs",
                    "main": entry_rel,
                    "types": "./index.d.ts",
                    "exports": {
                        ".": {
                            "types": "./index.d.ts",
                            "require": entry_rel,
                        }
                    },
                    "sideEffects": false,
                })
            } else {
                serde_json::json!({
                    "type": "commonjs",
                    "main": entry_rel,
                    "exports": {
                        ".": {
                            "require": entry_rel,
                        }
                    },
                    "sideEffects": false,
                })
            }
        }
        LibraryFormat::Iife => {
            if has_headers {
                serde_json::json!({
                    "main": entry_rel,
                    "types": "./index.d.ts",
                    "sideEffects": [entry_rel],
                })
            } else {
                serde_json::json!({
                    "main": entry_rel,
                    "sideEffects": [entry_rel],
                })
            }
        }
    };
    let mut text = serde_json::to_string_pretty(&json).expect("library package.json serializes");
    text.push('\n');
    artifact("package.json".into(), ArtifactKind::Metadata, text)
}
