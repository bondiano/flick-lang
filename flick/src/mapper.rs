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

use flick_core::project::Analyzed;
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
    let Some(module) = module_at(&snapshot, &path) else {
        // not a module of a project, as a file outside its source root
        return json!({ "text": "export {};\n", "extension": ".ts", "mappings": [] });
    };
    let text = &snapshot.sources.get(module.file).text;
    let projected = project_module(&mut project.written, &snapshot, module, &path);
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

fn module_at<'a>(snapshot: &'a Snapshot, path: &Path) -> Option<&'a Analyzed> {
    let file = snapshot.file(path)?;
    snapshot.analysis.modules.iter().find(|m| m.file == file)
}

/// The projection of `module` of `snapshot`, the file at `path`: it imports
/// the project's modules as their `.fl` files and the rest from
/// `.flick/projections`, which this writes.
fn project_module(
    written: &mut HashMap<PathBuf, String>,
    snapshot: &Snapshot,
    module: &Analyzed,
    path: &Path,
) -> Projected {
    let text = &snapshot.sources.get(module.file).text;
    let dir = path.parent().unwrap_or(Path::new("/"));
    let elsewhere = snapshot.root.join(".flick/projections");
    write_elsewhere(written, snapshot, &elsewhere);
    let specifier = |m: &str| match snapshot.analysis.module(m) {
        Some(other) => relative(dir, &other.path),
        None => relative(dir, &elsewhere.join(projection::file_of(m))),
    };
    let host = FsHost::new();
    projection_of(
        &host,
        &snapshot.analysis,
        Some(&snapshot.root),
        module,
        text,
        &specifier,
    )
}

/// `flick content-mapper --declarations paths`: the projection of each
/// `.fl` file under `paths`, written beside it as `Name.d.fl.ts`, where
/// TypeScript with `allowArbitraryExtensions` finds the types of
/// `import … from "./Name.fl"`. For editors whose TypeScript runs no
/// Content Mapper, as Astro's.
pub fn declarations(paths: &[PathBuf]) -> ExitCode {
    let mut files = Vec::new();
    for path in paths {
        sources_under(path, &mut files);
    }
    let mut caches = Caches::new();
    let mut written = HashMap::new();
    let mut failed = false;
    for file in files {
        let path = file.canonicalize().unwrap_or(file);
        let snapshot = analyze(&HashMap::new(), &path, &mut caches);
        let Some(module) = module_at(&snapshot, &path) else {
            continue;
        };
        let text = project_module(&mut written, &snapshot, module, &path).text;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let target = path.with_file_name(format!("{stem}.d.fl.ts"));
        if fs::read_to_string(&target).ok().as_deref() == Some(text.as_str()) {
            continue;
        }
        if let Err(err) = fs::write(&target, text) {
            eprintln!(
                "flick content-mapper: cannot write {}: {err}",
                target.display()
            );
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The `.fl` files of `path`, a file or a directory searched but for build
/// output and installed packages.
fn sources_under(path: &Path, out: &mut Vec<PathBuf>) {
    if path.extension().is_some_and(|e| e == "fl") {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let skipped = name.to_string_lossy().starts_with('.')
            || ["node_modules", "dist", "target"].contains(&name.to_string_lossy().as_ref());
        if !skipped {
            sources_under(&entry.path(), out);
        }
    }
}

/// Writes the projection of every module the analysis knows only by its
/// interface, built-ins and packages, under `dir`, each once per content.
fn write_elsewhere(written: &mut HashMap<PathBuf, String>, snapshot: &Snapshot, dir: &Path) {
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
        if written.get(&path) == Some(&text) {
            continue;
        }
        let saved = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, &text));
        match saved {
            Ok(()) => {
                written.insert(path, text);
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
