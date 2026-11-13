//! Entry runners of the native JavaScript build.

use crate::codegen;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::Namespace;
use crate::project::config::Backend;
use crate::project::{Compiled, MainKind, main_kind};
use crate::source::Span;

use super::super::{Artifact, Settings, bindings, runner_file, schema_aot_file};

/// The runner of each entry, or why `main` cannot be started.
pub(super) fn entry_runners(
    settings: &Settings,
    modules: &[Compiled],
    path: &dyn Fn(&str) -> String,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, entry) in settings.entries {
        let started_by = settings
            .runners
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r.as_str());
        let module = modules.iter().find(|m| &m.name == entry);
        let main = module.and_then(|m| m.interface.find(Namespace::Value, "main"));
        // a re-exported `main` is imported from the module that defines it, whose output has it
        let defining = main.map_or(entry.as_str(), |e| e.entity.module.as_str());
        // a project checked its entry already, against its runner if it has one;
        // a file built alone is checked here
        let kind = main
            .filter(|_| started_by.is_none())
            .and_then(|e| Some(main_kind(e.scheme.as_ref()?)));
        match kind {
            Some(Err(message)) => {
                let diagnostic = Diagnostic::new(codes::P026, Severity::Error, message);
                let file = modules.iter().find(|m| m.name == defining).map(|m| m.file);
                let at = file.zip(main.and_then(|e| e.at));
                diagnostics.push(match at {
                    Some((f, [start, end])) => diagnostic.with_span(Span::new(f, start, end)),
                    None => diagnostic,
                });
            }
            kind => {
                let file = runner_file(name, settings.typescript);
                let aot = settings.extras.schema_aot.then(|| schema_aot_file(name));
                let start = match started_by {
                    Some(module) => Start::Run(codegen::relative(&file, &path(module))),
                    None => match kind.and_then(Result::ok) {
                        Some(MainKind::Pure) => Start::Call,
                        _ if settings.backend == Backend::Rt => Start::RunMainRt,
                        _ => Start::RunMain,
                    },
                };
                out.push(runner(
                    &file,
                    &codegen::relative(&file, &path(defining)),
                    &start,
                    aot.as_deref(),
                ));
            }
        }
    }
}

/// The runner of an entry: starts the `main` of the module at `entry`, a
/// specifier relative to the runner, a pure one by a plain call. With the
/// file of compiled decoders `schema_aot` it collects the codecs the modules
/// make while loading and installs those decoders, which `flick build`
/// writes next to it.
fn runner(file: &str, entry: &str, start: &Start, schema_aot: Option<&str>) -> Artifact {
    let (import, run) = match start {
        Start::Call => (None, "main(undefined);"),
        Start::RunMain => (
            Some(("runMain", bindings::ADAPTER_PREFIX)),
            "await runMain(main);",
        ),
        Start::RunMainRt => (
            Some(("runMain", bindings::NATIVE_ENTRY)),
            "await runMain(main);",
        ),
        // what `run` returns is what the host takes from the module, such as Cloudflare's `fetch`
        Start::Run(module) => (Some(("run", module.as_str())), "export default run(main);"),
    };
    let import = import.map_or(String::new(), |(name, module)| {
        format!("import {{ {name} }} from \"{module}\";\n")
    });
    let contents = if let Some(aot) = schema_aot {
        format!(
            "import {{ enableSchemaAot, installSchemaAot }} from \"{}\";\n{import}\nenableSchemaAot();\nconst {{ main }} = await import(\"{entry}\");\ninstallSchemaAot(await import(\"./{aot}\"));\n{run}\n",
            bindings::ADAPTER_PREFIX
        )
    } else {
        format!("{import}import {{ main }} from \"{entry}\";\n\n{run}\n")
    };
    // a runner is also a package's `bin`, which a shell starts with Node
    Artifact {
        path: file.to_owned(),
        contents: format!("#!/usr/bin/env node\n{contents}"),
    }
}

/// How the runner of an entry starts `main`.
enum Start {
    /// A pure `main`, called.
    Call,
    /// An effect, run by the runtime's Node runner.
    RunMain,
    /// [`Start::RunMain`] on Flick's own runtime.
    RunMainRt,
    /// By `run` of a runner module, at this path from the runner.
    Run(String),
}
