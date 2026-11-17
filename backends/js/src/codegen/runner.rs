//! Portable entry launcher rendering; type classification belongs to the frontend.

use std::iter;

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
    // the root of `main`: the live provider of each `Host` service it names,
    // imported alone so that a bundle keeps no other; none, no root at all
    let run_main = |module: &'static str, host: &[String], rt: bool| {
        let providers: Vec<String> = (host.iter())
            .filter_map(|s| Some(format!("host{}", s.strip_prefix("Host.")?)))
            .collect();
        let list = providers.join(", ");
        let (root, run) = match (providers.is_empty(), rt) {
            (true, _) => (None, "await runMain(main);".to_owned()),
            (false, false) => (
                Some("provideHost"),
                format!("await runMain(main, {{ provide: provideHost([{list}]) }});"),
            ),
            (false, true) => (
                Some("withHost"),
                format!("await runMain(withHost([{list}], main));"),
            ),
        };
        let names = iter::once("runMain")
            .chain(root)
            .chain(providers.iter().map(String::as_str));
        (Some((names.collect::<Vec<_>>().join(", "), module)), run)
    };
    let (import, run) = match start {
        Start::Call => (None, "main(undefined);".to_owned()),
        Start::RunMain(host) => run_main(bindings::ADAPTER_PREFIX, host, false),
        Start::RunMainRt(host) => run_main(bindings::NATIVE_ENTRY, host, true),
        // what `run` returns is what the host takes from the module, such as Cloudflare's `fetch`
        Start::Run(module) => (
            Some(("run".to_owned(), module.as_str())),
            "export default run(main);".to_owned(),
        ),
    };
    let import = import.map_or(String::new(), |(names, module)| {
        format!("import {{ {names} }} from \"{module}\";\n")
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
    /// An effect, run by the runtime's Node runner with the live providers
    /// of the `Host` services it names, such as `Host.Console`, as its root.
    RunMain(Vec<String>),
    /// [`Start::RunMain`] on Flick's own runtime.
    RunMainRt(Vec<String>),
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

    #[test]
    fn a_runner_imports_only_the_host_providers_main_names() {
        let console = named_runner(
            "./App.js",
            "main",
            &Start::RunMain(vec!["Host.Console".to_owned()]),
            None,
        );
        assert_eq!(
            console,
            "#!/usr/bin/env node\nimport { runMain, provideHost, hostConsole } from \"@flick-lang/effect-ts\";\nimport { main } from \"./App.js\";\n\nawait runMain(main, { provide: provideHost([hostConsole]) });\n"
        );
        let rt = named_runner(
            "./App.js",
            "main",
            &Start::RunMainRt(vec!["Host.Console".to_owned(), "Host.Clock".to_owned()]),
            None,
        );
        assert!(rt.contains(
            "import { runMain, withHost, hostConsole, hostClock } from \"@flick-lang/runtime/rt\";"
        ));
        assert!(rt.ends_with("await runMain(withHost([hostConsole, hostClock], main));\n"));
        // no service named, no root and no provider in the bundle
        let none = named_runner("./App.js", "main", &Start::RunMainRt(vec![]), None);
        assert_eq!(
            none,
            "#!/usr/bin/env node\nimport { runMain } from \"@flick-lang/runtime/rt\";\nimport { main } from \"./App.js\";\n\nawait runMain(main);\n"
        );
    }
}
