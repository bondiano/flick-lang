//! Project templates contributed by the official Go backend (`flick/go`).

use flick_backend::command::{self, File, Plan, TemplateDescriptor, TemplateRequest};

const AGENTS: &str = include_str!("templates/AGENTS.md.template");

const PLUGIN_AGENTS: &str = "- `flick run plugin` rebuilds `plugin.wasm` after a change in `plugin/`;\n  the compiler runs the component, not the Rust sources.\n";

const PLUGIN: [(&str, &str); 6] = [
    (
        "src/Tagged.fl",
        include_str!("../../../js/src/component/templates/plugin/Tagged.fl"),
    ),
    (
        "rust-toolchain.toml",
        include_str!("../../../../rust-toolchain.toml"),
    ),
    (
        "plugin/Cargo.toml",
        include_str!("../../../js/src/component/templates/plugin/Cargo.toml.template"),
    ),
    (
        "plugin/src/lib.rs",
        include_str!("../../../js/src/component/templates/plugin/lib.rs"),
    ),
    (
        "explain/T001.md",
        include_str!("../../../js/src/component/templates/plugin/T001.md"),
    ),
    (
        "explain/T002.md",
        include_str!("../../../js/src/component/templates/plugin/T002.md"),
    ),
];

const PLUGIN_PACKAGE: &str = "[package]\nname = \"NAME\"\nversion = \"0.1.0\"\nplugin = \"plugin.wasm\"\n\n[scripts]\nplugin = [\n  \"cargo build --manifest-path plugin/Cargo.toml --features guest --target wasm32-wasip2 --release\",\n  \"cp plugin/target/wasm32-wasip2/release/CRATE.wasm plugin.wasm\",\n]\n\n";

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
        ("app", "Application with `main` compiled as a Go executable"),
        (
            "cli",
            "Command-line application with a `cli` block compiled as a Go executable",
        ),
        ("lib", "Library targeting Go"),
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

pub(super) fn scaffold(request: &TemplateRequest) -> Result<Plan, Vec<command::Diagnostic>> {
    let template = request.template.as_str();
    if !matches!(template, "app" | "cli" | "lib" | "plugin") {
        return Err(command::unknown_template(template, &descriptors()));
    }
    let name = request.name.as_str();
    let compiler = request.compiler.as_str();
    let app = matches!(template, "app" | "cli");
    let project = if app {
        "kind = \"app\"\nentry = \"App.Main\"\n"
    } else {
        "kind = \"lib\"\n"
    };
    let package_header = if template == "plugin" {
        let crate_name = format!("{name}_plugin").replace('-', "_");
        PLUGIN_PACKAGE
            .replace("NAME", name)
            .replace("CRATE", &crate_name)
    } else {
        String::new()
    };
    let config = format!(
        "schema-version = 1\n\n{package_header}[project]\n{project}source-dir = \"src\"\ntest-dir = \"test\"\n\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\ntarget = \"go\"\n\n[tools]\n\"flick/go\" = [\"go\"]\n"
    );
    let mut files = vec![
        ("flick.toml", config),
        (
            ".gitignore",
            "dist/\n.flick/*\n!.flick/plugin-snapshots/\n".to_owned(),
        ),
        ("AGENTS.md", AGENTS.to_owned()),
    ];
    if template == "plugin" {
        files[1].1.push_str("plugin/target/\n");
        files[2].1.push_str(PLUGIN_AGENTS);
        files.extend(PLUGIN.iter().map(|(path, text)| {
            (
                *path,
                text.replace("NAME", name).replace("COMPILER", compiler),
            )
        }));
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
    let stdout = match template {
        "app" | "cli" => "Next: flick run (needs Go on PATH; `flick test` runs the tests)\n",
        "lib" => "Next: flick test (needs Go on PATH)\n",
        "plugin" => {
            "Next: flick run plugin, which builds the plugin's component (needs Go on PATH for `flick test`)\n"
        }
        _ => unreachable!(),
    };
    plan.stdout = stdout.as_bytes().to_vec();
    Ok(plan)
}
