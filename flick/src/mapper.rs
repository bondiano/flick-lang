//! `flick content-mapper`: TypeScript's Content Mapper for `.fl` files, the
//! process `@flick-lang/content-mapper` starts. TypeScript sends requests on
//! stdin, framed as in LSP; each `transform` answers with the TypeScript
//! projection of the module, analyzed as the language server analyzes it,
//! the editor's unsaved text over the files. The modules of the project
//! import each other's `.fl` files, which TypeScript maps in turn; built-in
//! and package modules have no `.fl` file, so their projections are written
//! under `.flick/projections` of the project.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use flick_core::projection::{self, Projected};
use flick_core::types::builtins;
use serde_json::{Value, json};

use crate::commands::{interface_of, projection_of};
use crate::lsp::protocol;
use crate::lsp::{Caches, Snapshot, analyze};
use crate::sources::FsHost;

/// `SpanMapKind` and `SpanMapFeature` of TypeScript's span maps.
const VERBATIM: u32 = 0;
const ATOM: u32 = 1;
/// Every feature but those that write text back: a type is not the text it
/// maps to, so a rename or a fix through it would garble the source.
const READ_ONLY: u32 = 1_048_575 & !(256 | 1024 | 2048 | 65_536 | 131_072);

#[derive(Default)]
struct Project {
    caches: Caches,
    /// The projections written under `.flick/projections`, by path.
    written: HashMap<PathBuf, String>,
}

pub fn serve() -> ExitCode {
    let mut projects: HashMap<String, Project> = HashMap::new();
    let mut input = BufReader::new(io::stdin().lock());
    let mut out = io::stdout();
    loop {
        let message = match protocol::read(&mut input) {
            Ok(Some(m)) => m,
            Ok(None) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("flick content-mapper: {err}");
                return ExitCode::FAILURE;
            }
        };
        let params = &message["params"];
        let handle = params["projectHandle"].as_str().unwrap_or("").to_owned();
        let answer = match message["method"].as_str().unwrap_or("") {
            "initialize" => initialize(params),
            "openProject" => {
                projects.entry(handle).or_default();
                Ok(json!({}))
            }
            "transform" => {
                let project = projects.entry(handle).or_default();
                Ok(transform(project, params))
            }
            "closeProject" => {
                projects.remove(&handle);
                Ok(Value::Null)
            }
            method => Err(format!("unknown method `{method}`")),
        };
        let Some(id) = message.get("id") else {
            continue;
        };
        let reply = match answer {
            Ok(result) => protocol::response(id, &result),
            Err(text) => protocol::error(id, -32601, &text),
        };
        if protocol::write(&mut out, &reply).is_err() {
            return ExitCode::FAILURE;
        }
    }
}

/// Projections count UTF-8 bytes, which TypeScript offers.
fn initialize(params: &Value) -> Result<Value, String> {
    let offered = params["positionEncodings"]
        .as_array()
        .is_some_and(|e| e.iter().any(|e| e == "utf-8"));
    if !offered {
        return Err("flick content-mapper maps UTF-8 positions; TypeScript offered none".into());
    }
    Ok(json!({ "positionEncoding": "utf-8", "diagnosticSource": "flick" }))
}

fn transform(project: &mut Project, params: &Value) -> Value {
    let path = PathBuf::from(params["fileName"].as_str().unwrap_or(""));
    let content = params["content"].as_str().unwrap_or("").to_owned();
    let docs = HashMap::from([(path.clone(), content)]);
    let snapshot = analyze(&docs, &path, &mut project.caches);
    let Some(module) = snapshot
        .file(&path)
        .and_then(|file| snapshot.analysis.modules.iter().find(|m| m.file == file))
    else {
        // not a module of a project, as a file outside its source root
        return json!({ "text": "export {};\n", "extension": ".ts", "mappings": [] });
    };
    let text = &snapshot.sources.get(module.file).text;
    let dir = path.parent().unwrap_or(Path::new("/"));
    let elsewhere = snapshot.root.join(".flick/projections");
    write_elsewhere(project, &snapshot, &elsewhere);
    let specifier = |m: &str| match snapshot.analysis.module(m) {
        Some(other) => relative(dir, &other.path),
        None => relative(dir, &elsewhere.join(projection::file_of(m))),
    };
    let host = FsHost::new();
    let projected = projection_of(
        &host,
        &snapshot.analysis,
        Some(&snapshot.root),
        module,
        text,
        &specifier,
    );
    let mappings: Vec<Value> = projected
        .mappings
        .iter()
        .map(|m| {
            let (ts, fl) = (m.ts[0]..m.ts[1], m.fl[0]..m.fl[1]);
            let same = projected.text.get(ts.start as usize..ts.end as usize)
                == text.get(fl.start as usize..fl.end as usize);
            let (kind, features) = if m.exact && same {
                (VERBATIM, Value::Null)
            } else {
                (ATOM, json!(READ_ONLY))
            };
            let mut span = json!([ts.start, ts.len(), fl.start, fl.len(), kind]);
            if !features.is_null() {
                span.as_array_mut().expect("array").push(features);
            }
            span
        })
        .collect();
    json!({ "text": projected.text, "extension": ".ts", "mappings": mappings })
}

/// Writes the projection of every module the analysis knows only by its
/// interface, built-ins and packages, under `dir`, each once per content.
fn write_elsewhere(project: &mut Project, snapshot: &Snapshot, dir: &Path) {
    let analysis = &snapshot.analysis;
    let fingerprint = "external";
    let lookup = |m: &str| interface_of(analysis, m);
    let interfaces = builtins().iter().chain(
        analysis
            .compilation
            .package_modules
            .iter()
            .map(|p| &p.interface),
    );
    for interface in interfaces {
        let specifier = |m: &str| projection::relative(&interface.module, m);
        let Projected { file, text, .. } =
            projection::project_interface(interface, fingerprint, &lookup, &|_| None, &specifier);
        let path = dir.join(file);
        if project.written.get(&path) == Some(&text) {
            continue;
        }
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, &text));
        match written {
            Ok(()) => {
                project.written.insert(path, text);
            }
            Err(err) => eprintln!(
                "flick content-mapper: cannot write {}: {err}",
                path.display()
            ),
        }
    }
}

/// The specifier of `to` from a file in `from`: `./Other.fl`, `../x/Y.flick.ts`.
fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<Component> = from.components().collect();
    let to_parts: Vec<Component> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - common];
    parts.extend(
        to_parts[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specifiers_are_relative_to_the_importing_file() {
        assert_eq!(
            relative(Path::new("/p/src/App"), Path::new("/p/src/App/Users.fl")),
            "./Users.fl"
        );
        assert_eq!(
            relative(Path::new("/p/src/App"), Path::new("/p/src/Lib/Names.fl")),
            "../Lib/Names.fl"
        );
        assert_eq!(
            relative(
                Path::new("/p/src/App"),
                Path::new("/p/.flick/projections/Prelude.flick.ts")
            ),
            "../../.flick/projections/Prelude.flick.ts"
        );
    }
}
