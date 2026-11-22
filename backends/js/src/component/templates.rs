//! Project templates contributed by the official JavaScript backend (`flick/js`).

use flick_backend::command::{self, File, Plan, TemplateDescriptor, TemplateRequest};

const RUNTIME: &str = "0.1.0";
const PNPM: &str = "12.16.1";
const REACT: &str = "19.3.1";
const VITE: &str = "8.3.5";
const ADAPTER_MANIFEST: &str = include_str!("../../../../packages/effect-ts/package.json");

fn adapter_version() -> String {
    let manifest: serde_json::Value =
        serde_json::from_str(ADAPTER_MANIFEST).expect("adapter manifest");
    manifest["version"]
        .as_str()
        .expect("adapter version")
        .to_owned()
}

const FULLSTACK_ENTRIES: &str = "kind = \"app\"\nrunner = \"Runner.Node\"\nentries = { server = \"App.Server\", web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n";

const FULLSTACK: [(&str, &str); 8] = [
    (
        "src/App/Todo.fl",
        include_str!("templates/fullstack/Todo.fl"),
    ),
    (
        "src/App/Store.fl",
        include_str!("templates/fullstack/Store.fl"),
    ),
    ("src/App/Api.fl", include_str!("templates/fullstack/Api.fl")),
    (
        "src/App/Server.fl",
        include_str!("templates/fullstack/Server.fl"),
    ),
    ("src/App/Web.fl", include_str!("templates/fullstack/Web.fl")),
    ("src/main.js", include_str!("templates/fullstack/main.js")),
    ("index.html", include_str!("templates/fullstack/index.html")),
    (
        "vite.config.mjs",
        include_str!("templates/fullstack/vite.config.mjs"),
    ),
];

const PLUGIN: [(&str, &str); 6] = [
    ("src/Tagged.fl", include_str!("templates/plugin/Tagged.fl")),
    (
        "rust-toolchain.toml",
        include_str!("../../../../rust-toolchain.toml"),
    ),
    (
        "plugin/Cargo.toml",
        include_str!("templates/plugin/Cargo.toml.template"),
    ),
    ("plugin/src/lib.rs", include_str!("templates/plugin/lib.rs")),
    ("explain/T001.md", include_str!("templates/plugin/T001.md")),
    ("explain/T002.md", include_str!("templates/plugin/T002.md")),
];

const PLUGIN_PACKAGE: &str = "[package]\nname = \"NAME\"\nversion = \"0.1.0\"\nplugin = \"plugin.wasm\"\n\n[scripts]\nplugin = [\n  \"cargo build --manifest-path plugin/Cargo.toml --features guest --target wasm32-wasip2 --release\",\n  \"cp plugin/target/wasm32-wasip2/release/CRATE.wasm plugin.wasm\",\n]\n\n";

const AGENTS: &str = include_str!("templates/AGENTS.md.template");

const PLUGIN_AGENTS: &str = "- `flick run plugin` rebuilds `plugin.wasm` after a change in `plugin/`;\n  the compiler runs the component, not the Rust sources.\n";

const MAIN: &str = "module App.Main exposing (main)

import App.Greeting exposing (greet)

run : Unit ->{Host.Arguments, Host.Console} Unit
let run () =
  let name = Host.Arguments.args () |> List.head |> Option.with_default \"world\"
  Host.Console.stdout_line (greet name)

main : Unit ->{; AppError} Unit
let main () = provide (Layer.merge Host.arguments Host.console) { run () }
";

const CLI_MAIN: &str = "module App.Main exposing (main)

import App.Greeting exposing (greet)

command : Cli.Command {Host.Console} none
let command = cli \"NAME\" {
  description \"Prints a greeting\"
  /// Print in capitals
  flag loud : Bool
  /// Who to greet
  arg name : String = \"world\"
  handle fun { loud; name } ->
    let text = greet name
    Platform.stdout_line (if loud then String.to_upper text else text)
}

main : Unit ->{Host.Arguments, Host.Console, Host.Environment, Async; AppError} Unit
let main () =
  try Cli.run_version \"0.1.0\" command (Platform.args ()) with
  | Cli.CliError message -> App.fail message
";

const GREETING: &str = "module Lib.Greeting exposing (greet)

/// A greeting for `name`.
greet : String -> String
let greet name = \"Hello, \" ++ name ++ \"!\"
";

const TEST: &str = "module Lib.GreetingTest

import Lib.Greeting exposing (greet)
import Test exposing (expect, to)

suite \"greet\" {
  test \"greets by name\" {
    expect (greet \"Ada\") |> to.equal \"Hello, Ada!\"
  }
}
";

pub(super) fn descriptors() -> Vec<TemplateDescriptor> {
    [
        ("app", "Application with `main`"),
        ("cli", "Command-line application with a `cli` block"),
        (
            "fullstack",
            "Server and browser page sharing `server fn` calls with Vite",
        ),
        ("lib", "Library package"),
        (
            "plugin",
            "Library with a compiler plugin crate in `plugin/`",
        ),
    ]
    .into_iter()
    .map(|(name, summary)| TemplateDescriptor {
        name: name.into(),
        summary: summary.into(),
    })
    .collect()
}

fn package_json(name: &str, compiler: &str, template: &str) -> String {
    let app = !matches!(template, "lib" | "plugin");
    let needs_effect = matches!(template, "cli" | "fullstack");
    let mut package = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "private": app,
        "type": "module",
        "packageManager": format!("pnpm@{PNPM}"),
        "dependencies": { "@flick-lang/runtime": RUNTIME },
        "flick": { "compiler": compiler },
    });
    if needs_effect {
        package["dependencies"]["@flick-lang/effect-ts"] = adapter_version().into();
    }
    if template == "fullstack" {
        let dependencies = &mut package["dependencies"];
        dependencies["react"] = REACT.into();
        dependencies["react-dom"] = REACT.into();
        package["devDependencies"] =
            serde_json::json!({ "@flick-lang/astro": RUNTIME, "vite": VITE });
    }
    format!("{package:#}\n")
}

pub(super) fn scaffold(request: &TemplateRequest) -> Result<Plan, Vec<command::Diagnostic>> {
    let template = request.template.as_str();
    if !matches!(template, "app" | "cli" | "fullstack" | "lib" | "plugin") {
        return Err(command::unknown_template(template, &descriptors()));
    }
    let name = request.name.as_str();
    let compiler = request.compiler.as_str();
    let app = !matches!(template, "lib" | "plugin");
    let project = match template {
        "lib" | "plugin" => "kind = \"lib\"\n",
        "fullstack" => FULLSTACK_ENTRIES,
        _ => "kind = \"app\"\nentry = \"App.Main\"\n",
    };
    let package_header = if template == "plugin" {
        let crate_name = format!("{name}_plugin").replace('-', "_");
        PLUGIN_PACKAGE
            .replace("NAME", name)
            .replace("CRATE", &crate_name)
    } else {
        String::new()
    };
    let mut config = format!(
        "schema-version = 1\n\n{package_header}[project]\n{project}source-dir = \"src\"\ntest-dir = \"test\"\n\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\ntarget = \"js\"\n"
    );
    if matches!(template, "cli" | "fullstack") {
        config.push_str("\n[target.js]\nruntime = \"effect\"\n");
    }
    if template == "fullstack" {
        config.push_str("\n[tools]\n\"flick/js\" = [\"pnpm\", \"node\"]\n");
    } else {
        config.push_str("\n[tools]\n\"flick/js\" = [\"pnpm\"]\n");
    }
    let mut files = vec![
        ("flick.toml", config),
        ("package.json", package_json(name, compiler, template)),
        (
            ".gitignore",
            "node_modules/\ndist/\n.flick/*\n!.flick/plugin-snapshots/\n".to_owned(),
        ),
        ("AGENTS.md", AGENTS.to_owned()),
    ];
    if template == "plugin" {
        files[2].1.push_str("plugin/target/\n");
        files[3].1.push_str(PLUGIN_AGENTS);
        files.extend(PLUGIN.iter().map(|(path, text)| {
            (
                *path,
                text.replace("NAME", name).replace("COMPILER", compiler),
            )
        }));
    } else if template == "fullstack" {
        files.extend(
            FULLSTACK
                .iter()
                .map(|(path, text)| (*path, text.replace("NAME", name))),
        );
    } else if app {
        let main = if template == "cli" {
            CLI_MAIN.replace("NAME", name)
        } else {
            MAIN.to_owned()
        };
        files.push(("src/App/Main.fl", main));
        files.push((
            "src/App/Greeting.fl",
            GREETING.replace("Lib.Greeting", "App.Greeting"),
        ));
        files.push((
            "test/App/GreetingTest.fl",
            TEST.replace("Lib.Greeting", "App.Greeting"),
        ));
    } else {
        files.push(("src/Lib/Greeting.fl", GREETING.to_owned()));
        files.push(("test/Lib/GreetingTest.fl", TEST.to_owned()));
    }

    let mut plan = Plan::empty();
    plan.files = files
        .into_iter()
        .map(|(path, contents)| File {
            path: path.into(),
            bytes: contents.into_bytes(),
            executable: false,
        })
        .collect();
    let mut stdout = format!(
        "Next: flick install (it needs pnpm {PNPM}, the version this project pins; `flick doctor` checks the toolchain)\n"
    );
    if template == "plugin" {
        stdout.push_str("Then: flick run plugin, which builds the plugin's component\n");
    }
    plan.stdout = stdout.into_bytes();
    Ok(plan)
}
