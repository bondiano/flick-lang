//! `flick new` and `flick init`: the files of a project pinning the
//! compiler, the runtime, Effect and pnpm. `init` never overwrites: every
//! file that exists is reported before anything is written. Neither
//! installs anything; that is `flick install`.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::SourceMap;

use super::build::write;
use crate::args::Template;
use crate::command::{Command, Outcome};
use crate::toolchain::{COMPILER, EFFECT, PNPM, REACT, RUNTIME, VITE};

/// The entries of the `fullstack` template: a server, and a page that Vite
/// serves in development.
const FULLSTACK_ENTRIES: &str = "kind = \"app\"\nrunner = \"Runner.Node\"\nentries = { server = \"App.Server\", web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n";

/// The sources of the `fullstack` template; `NAME` is the project's.
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

/// The files of the `plugin` template besides the manifests; `NAME` is the
/// package's, `COMPILER` this compiler's version.
const PLUGIN: [(&str, &str); 6] = [
    ("src/Tagged.fl", include_str!("templates/plugin/Tagged.fl")),
    // the toolchain of this compiler, with the target of components
    (
        "rust-toolchain.toml",
        include_str!("../../../rust-toolchain.toml"),
    ),
    (
        "plugin/Cargo.toml",
        include_str!("templates/plugin/Cargo.toml.template"),
    ),
    ("plugin/src/lib.rs", include_str!("templates/plugin/lib.rs")),
    ("explain/T001.md", include_str!("templates/plugin/T001.md")),
    ("explain/T002.md", include_str!("templates/plugin/T002.md")),
];

/// The `[package]` and `[scripts]` of the `plugin` template: the component
/// `flick run plugin` builds is the package's plugin.
const PLUGIN_PACKAGE: &str = "[package]\nname = \"NAME\"\nversion = \"0.1.0\"\nplugin = \"plugin.wasm\"\n\n[scripts]\nplugin = [\n  \"cargo build --manifest-path plugin/Cargo.toml --features guest --target wasm32-wasip2 --release\",\n  \"cp plugin/target/wasm32-wasip2/release/CRATE.wasm plugin.wasm\",\n]\n\n";

pub struct ScaffoldCommand {
    pub dir: PathBuf,
    pub template: Template,
    /// `new`: the directory must not exist yet.
    pub fresh: bool,
}

/// The files of a project named `name`.
fn files(name: &str, template: Template) -> Vec<(&'static str, String)> {
    let app = !matches!(template, Template::Lib | Template::Plugin);
    let project = match template {
        Template::Lib | Template::Plugin => "kind = \"lib\"\n",
        Template::Fullstack => FULLSTACK_ENTRIES,
        Template::App | Template::Cli => "kind = \"app\"\nentry = \"App.Main\"\n",
    };
    let package = if template == Template::Plugin {
        let crate_name = format!("{name}_plugin").replace('-', "_");
        PLUGIN_PACKAGE
            .replace("NAME", name)
            .replace("CRATE", &crate_name)
    } else {
        String::new()
    };
    let config = format!(
        "schema-version = 1\n\n{package}[project]\n{project}source-dir = \"src\"\ntest-dir = \"test\"\n\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\nemit = \"js\"\n"
    );
    let mut package = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "private": app,
        "type": "module",
        "packageManager": format!("pnpm@{PNPM}"),
        "dependencies": { "@flick-lang/runtime": RUNTIME, "effect": EFFECT },
        "flick": { "compiler": COMPILER },
    });
    if template == Template::Fullstack {
        let dependencies = &mut package["dependencies"];
        dependencies["react"] = REACT.into();
        dependencies["react-dom"] = REACT.into();
        package["devDependencies"] =
            serde_json::json!({ "@flick-lang/astro": RUNTIME, "vite": VITE });
    }
    let mut out = vec![
        ("flick.toml", config),
        ("package.json", format!("{package:#}\n")),
        // snapshots of what plugins fetched are part of the project
        (
            ".gitignore",
            "node_modules/\ndist/\n.flick/*\n!.flick/plugin-snapshots/\n".to_owned(),
        ),
    ];
    if template == Template::Plugin {
        out[2].1.push_str("plugin/target/\n");
        out.extend(PLUGIN.iter().map(|(path, text)| {
            (
                *path,
                text.replace("NAME", name).replace("COMPILER", COMPILER),
            )
        }));
    } else if template == Template::Fullstack {
        out.extend(
            FULLSTACK
                .iter()
                .map(|(path, text)| (*path, text.replace("NAME", name))),
        );
    } else if app {
        let main = if template == Template::Cli {
            CLI_MAIN.replace("NAME", name)
        } else {
            MAIN.to_owned()
        };
        out.push(("src/App/Main.fl", main));
        out.push((
            "src/App/Greeting.fl",
            GREETING.replace("Lib.Greeting", "App.Greeting"),
        ));
        out.push((
            "test/App/GreetingTest.fl",
            TEST.replace("Lib.Greeting", "App.Greeting"),
        ));
    } else {
        out.push(("src/Lib/Greeting.fl", GREETING.to_owned()));
        out.push(("test/Lib/GreetingTest.fl", TEST.to_owned()));
    }
    out
}

const MAIN: &str = "module App.Main exposing (main)

import App.Greeting exposing (greet)

main : Unit ->{; AppError} Unit
let main () =
  let name = Platform.args () |> List.head |> Option.with_default \"world\"
  Platform.stdout_line (greet name)
";

const CLI_MAIN: &str = "module App.Main exposing (main)

import App.Greeting exposing (greet)
import Cli exposing (plugin cli)

command : Cli.Command none none
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

main : Unit ->{; AppError} Unit
let main () =
  try Cli.run_version \"0.1.0\" command (Platform.args ()) with
  | Cli.CliError message -> App.fail message
";

const GREETING: &str = "module Lib.Greeting exposing (greet)

/// A greeting for `name`.
greet : String -> String
let greet name = \"Hello, \" ++ name ++ \"!\"
";

const TEST: &str = "module Lib.GreetingTest exposing (tests)

import Lib.Greeting exposing (greet)

tests : List Test.Test
let tests = [Test.test \"greets\" (fun () -> Test.equal \"Hello, Ada!\" (greet \"Ada\"))]
";

impl Command for ScaffoldCommand {
    fn run(&self) -> Outcome {
        let error = |code, message: String| Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
        };
        if self.fresh && self.dir.exists() {
            return error(
                codes::P019,
                format!(
                    "`{}` exists: `flick new` makes a new directory; use `flick init` inside one",
                    self.dir.display()
                ),
            );
        }
        let name = package_name(&self.dir);
        let files = files(&name, self.template);
        let existing: Vec<Diagnostic> = files
            .iter()
            .filter(|(path, _)| self.dir.join(path).exists())
            .map(|(path, _)| {
                Diagnostic::new(
                    codes::P019,
                    Severity::Error,
                    format!("`{path}` exists; `flick init` writes nothing over it"),
                )
            })
            .collect();
        if !existing.is_empty() {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: existing,
            };
        }
        for (path, contents) in &files {
            if let Err(err) = write(&self.dir.join(path), contents) {
                return error(codes::P004, format!("cannot write `{path}`: {err}"));
            }
        }
        println!(
            "Created the {} `{name}` in {}",
            kind(self.template),
            shown(&self.dir)
        );
        println!("Next: flick install");
        if self.template == Template::Plugin {
            println!("Then: flick run plugin, which builds the plugin's component");
        }
        Outcome::default()
    }
}

fn kind(template: Template) -> &'static str {
    match template {
        Template::App => "app",
        Template::Cli => "command-line app",
        Template::Fullstack => "full-stack app",
        Template::Lib => "library",
        Template::Plugin => "library with a plugin",
    }
}

fn shown(dir: &Path) -> String {
    fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .display()
        .to_string()
}

/// An npm name from the directory: lowercase, other characters as `-`.
fn package_name(dir: &Path) -> String {
    let base = fs::canonicalize(dir)
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "app".to_owned());
    let name: String = base
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.trim_matches('-').to_owned()
}
