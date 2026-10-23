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
use crate::toolchain::{COMPILER, EFFECT, PNPM, RUNTIME};

pub struct ScaffoldCommand {
    pub dir: PathBuf,
    pub template: Template,
    /// `new`: the directory must not exist yet.
    pub fresh: bool,
}

/// The files of a project named `name`.
fn files(name: &str, template: Template) -> Vec<(&'static str, String)> {
    let app = template == Template::App;
    let project = if app {
        "kind = \"app\"\nentry = \"App.Main\"\n"
    } else {
        "kind = \"lib\"\n"
    };
    let config = format!(
        "schema-version = 1\n\n[project]\n{project}source-dir = \"src\"\ntest-dir = \"test\"\n\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\nemit = \"js\"\n"
    );
    let package = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "private": app,
        "type": "module",
        "packageManager": format!("pnpm@{PNPM}"),
        "dependencies": { "@flick-lang/runtime": RUNTIME, "effect": EFFECT },
        "flick": { "compiler": COMPILER },
    });
    let mut out = vec![
        ("flick.toml", config),
        ("package.json", format!("{package:#}\n")),
        (".gitignore", "node_modules/\ndist/\n.flick/\n".to_owned()),
    ];
    if app {
        out.push(("src/App/Main.fl", MAIN.to_owned()));
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
        Outcome::default()
    }
}

fn kind(template: Template) -> &'static str {
    match template {
        Template::App => "app",
        Template::Lib => "library",
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
