//! Portable entry launcher rendering; type classification belongs to the frontend.

use flick_backend_support::effects::bindings;

/// The runner of an entry: starts the `main` of the module at `entry`, a
/// specifier relative to the runner, a pure one by a plain call. With the
/// file of compiled decoders `schema_aot` it collects the codecs the modules
/// make while loading and installs those decoders, which `flick build`
/// writes next to it.
pub fn runner(entry: &str, start: &Start, schema_aot: Option<&str>) -> String {
    named_runner(entry, "main", start, schema_aot)
}

/// Render an explicit entry symbol, whose JS identifier the caller supplies.
pub fn named_runner(entry: &str, symbol: &str, start: &Start, schema_aot: Option<&str>) -> String {
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
    let imported = if symbol == "main" {
        "main".into()
    } else {
        format!("{symbol} as main")
    };
    let bound = if symbol == "main" {
        "main".into()
    } else {
        format!("{symbol}: main")
    };
    let contents = if let Some(aot) = schema_aot {
        format!(
            "import {{ enableSchemaAot, installSchemaAot }} from \"{}\";\n{import}\nenableSchemaAot();\nconst {{ {bound} }} = await import(\"{entry}\");\ninstallSchemaAot(await import(\"./{aot}\"));\n{run}\n",
            bindings::ADAPTER_PREFIX
        )
    } else {
        format!("{import}import {{ {imported} }} from \"{entry}\";\n\n{run}\n")
    };
    // a runner is also a package's `bin`, which a shell starts with Node
    format!("#!/usr/bin/env node\n{contents}")
}

/// How the runner of an entry starts `main`.
pub enum Start {
    /// A pure `main`, called.
    Call,
    /// An effect, run by the runtime's Node runner.
    RunMain,
    /// [`Start::RunMain`] on Flick's own runtime.
    RunMainRt,
    /// By `run` of a runner module, at this path from the runner.
    Run(String),
}

#[cfg(test)]
mod tests {
    use super::{Start, named_runner};

    #[test]
    fn an_explicit_root_symbol_is_imported_as_the_local_main() {
        assert_eq!(
            named_runner("./App.js", "boot", &Start::Call, None),
            "#!/usr/bin/env node\nimport { boot as main } from \"./App.js\";\n\nmain(undefined);\n"
        );
        let aot = named_runner("./App.js", "boot", &Start::Call, Some("schemas.aot.js"));
        assert!(aot.contains("const { boot: main } = await import(\"./App.js\");"));
    }
}
