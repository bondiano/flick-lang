//! The host's answers to what a command needs: the TypeScript projection
//! of a module, analyzed as the language server analyzes it, with the
//! editor's text over the file. The modules of its project are imported
//! as their `.fl` files; built-in and package modules have no `.fl` file,
//! so their projections are written under `.flick/projections` of the
//! project, which the projection's imports name.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use flick_backend::command::{Answer, Need, Source, TsSpan, TypescriptModule};
use flick_core::project::{self, Analyzed};
use flick_core::projection::{self, Projected};
use flick_core::source::SourceMap;
use flick_core::types::builtins;

use crate::commands::{interface_of, projection_of};
use crate::lsp::{Caches, Snapshot, analyze};
use crate::sources::FsHost;
use crate::stdlib;

/// What answering keeps between needs: the analysis caches and the
/// projects whose `.flick/projections` it wrote.
#[derive(Default)]
pub(super) struct Projections {
    caches: Caches,
    written: HashSet<PathBuf>,
    /// Whether the implicit packages are ready, which a contributed
    /// command does not need until it asks for a projection.
    prepared: bool,
}

impl Projections {
    /// The answers to `needs`, in order; a relative path is under `root`.
    pub(super) fn answer(&mut self, root: &Path, needs: &[Need]) -> Vec<Answer> {
        (needs.iter())
            .map(|need| match need {
                Need::Typescript(source) => Answer::Typescript(self.typescript(root, source)),
            })
            .collect()
    }

    fn typescript(&mut self, root: &Path, source: &Source) -> Option<TypescriptModule> {
        if !self.prepared {
            self.prepared = true;
            if let Err(why) = stdlib::prepare(Some(&root.to_string_lossy())) {
                eprintln!("flick: {why}");
            }
        }
        // the analysis knows a module by its canonical path
        let path = root.join(&source.path);
        let path = path.canonicalize().unwrap_or(path);
        let docs = (source.text.clone())
            .map(|text| HashMap::from([(path.clone(), text)]))
            .unwrap_or_default();
        let snapshot = analyze(&docs, &path, &mut self.caches);
        let module = module_at(&snapshot, &path)?;
        let text = snapshot.sources.get(module.file).text.clone();
        let projected = project_module(&mut self.written, &snapshot, module, &path);
        let spans = (projected.mappings.iter())
            .map(|m| TsSpan {
                ts_start: m.ts[0],
                ts_end: m.ts[1],
                fl_start: m.fl[0],
                fl_end: m.fl[1],
                exact: m.exact,
            })
            .collect();
        Some(TypescriptModule {
            path: path.to_string_lossy().into_owned(),
            source: text,
            text: projected.text,
            spans,
        })
    }
}

fn module_at<'a>(snapshot: &'a Snapshot, path: &Path) -> Option<&'a Analyzed> {
    let file = snapshot.file(path)?;
    snapshot.analysis.modules.iter().find(|m| m.file == file)
}

/// The projection of `module` of `snapshot`, the file at `path`: it imports
/// the project's modules as their `.fl` files and the rest from
/// `.flick/projections`, which this writes.
fn project_module(
    written: &mut HashSet<PathBuf>,
    snapshot: &Snapshot,
    module: &Analyzed,
    path: &Path,
) -> Projected {
    let text = &snapshot.sources.get(module.file).text;
    let dir = path.parent().unwrap_or(Path::new("/"));
    let elsewhere = snapshot.root.join(".flick/projections");
    // the built-in modules project the same in every request: once a session
    if written.insert(elsewhere.clone()) {
        write_elsewhere(snapshot, &elsewhere);
    }
    let specifier = |m: &str| match snapshot.analysis.module(m) {
        Some(other) => relative(dir, &other.path),
        None => relative(dir, &elsewhere.join(projection::file_of(m))),
    };
    let host = FsHost::new();
    let project = project::discover(
        &host,
        &mut SourceMap::default(),
        &snapshot.root,
        Some(&snapshot.root),
    )
    .ok()
    .flatten();
    projection_of(
        &host,
        &snapshot.analysis,
        project.as_ref(),
        module,
        text,
        &specifier,
    )
}

/// Writes the projection of every module the analysis knows only by its
/// interface, built-ins and packages, under `dir`, but those it holds.
fn write_elsewhere(snapshot: &Snapshot, dir: &Path) {
    let analysis = &snapshot.analysis;
    let fingerprint = "external";
    let lookup = |m: &str| interface_of(analysis, m);
    for interface in builtins() {
        let specifier = |m: &str| projection::relative(&interface.module, m);
        let Projected { file, text, .. } =
            projection::project_interface(interface, fingerprint, &lookup, &|_| None, &specifier);
        let path = dir.join(file);
        if fs::read_to_string(&path).is_ok_and(|held| held == text) {
            continue;
        }
        let saved = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, &text));
        if let Err(err) = saved {
            eprintln!("flick: cannot write {}: {err}", path.display());
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
