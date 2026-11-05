//! What a plugin reaches outside its blocks: only what the project grants,
//! and a kept expansion only while what it read stays the same.

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flick_plugin::{ReachError, Request, Response};

use super::{Memory, host, library, roots, show};
use crate::ir;
use crate::plugin::reach::Outside;
use crate::project::config::Grants;
use crate::project::load::compile;
use crate::source::SourceMap;

/// Files in memory and one variable, `TOKEN`; no snapshot answers.
#[derive(Default)]
struct World {
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
}

impl World {
    fn write(&self, path: &str, text: &str) {
        let mut files = self.files.lock().expect("files");
        files.insert(PathBuf::from(path), text.as_bytes().to_vec());
    }
}

impl Outside for World {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let files = self.files.lock().expect("files");
        files
            .get(path)
            .cloned()
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn env(&self, name: &str) -> Option<String> {
        (name == "TOKEN").then(|| "secret".to_owned())
    }

    fn fetch(&self, _: &Path, request: &Request) -> Result<Response, ReachError> {
        Err(ReachError::Offline(format!(
            "no snapshot answers `{}`",
            request.url
        )))
    }

    fn replay(&self, _: &Path) -> Option<Response> {
        None
    }
}

fn reaching(value: &str, world: &Arc<World>) -> Memory {
    let main = format!(
        "module App.Main exposing (..)\n\nimport Pairs exposing (plugin {head})\n\nnote : String\nlet note = {value}\n",
        head = value.split(' ').next().unwrap_or_default()
    );
    let mut host = host(&main);
    host.outside = Some(Arc::clone(world) as Arc<dyn Outside>);
    host
}

/// The IR of `App.Main` and each diagnostic, with `grants` for `pairs`.
fn built(host: &Memory, grants: Grants) -> (String, Vec<String>) {
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.plugins.insert("pairs".into(), grants);
    let (diagnostics, compilation) = compile(host, &mut sources, Some(&project), &roots());
    let shown = diagnostics.iter().map(|d| show(&sources, d)).collect();
    let ir = (compilation.modules.first()).map_or(String::new(), |m| ir::print(&m.ir));
    (ir, shown)
}

fn reads(paths: &[&str]) -> Grants {
    Grants {
        fs_read: paths.iter().map(|p| (*p).to_owned()).collect(),
        ..Grants::default()
    }
}

#[test]
fn what_the_project_does_not_grant_is_refused_at_the_block() {
    let world = Arc::new(World::default());
    world.write("/p/secret.txt", "key");
    let host = reaching("read_file \"secret.txt\" {}", &world);
    let (_, found) = built(&host, Grants::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with(
            "FLICK-P038 `read_file \"secret.txt\" {}`: plugin `pairs` asks to read `secret.txt`"
        ),
        "{found:?}"
    );
    assert!(found[0].contains("add `fs-read = [\"secret.txt\"]` to `[plugins.pairs]`"));
    let host = reaching("env_var \"HOME\" {}", &world);
    let (_, found) = built(&host, Grants::default());
    assert!(found[0].contains("add `env = [\"HOME\"]`"), "{found:?}");
}

#[test]
fn a_granted_file_is_read_and_a_change_to_it_expands_again() {
    let world = Arc::new(World::default());
    world.write("/p/notes/a.txt", "first");
    let host = reaching("read_file \"notes/a.txt\" {}", &world);
    let (ir, found) = built(&host, reads(&["notes"]));
    assert!(found.is_empty(), "{found:?}");
    assert!(ir.contains("\"first\""), "{ir}");
    assert_eq!(host.calls(), (1, 1));
    // the same file: the kept expansion holds
    built(&host, reads(&["notes"]));
    assert_eq!(host.calls(), (1, 1));
    world.write("/p/notes/a.txt", "second");
    let (ir, _) = built(&host, reads(&["notes"]));
    assert!(ir.contains("\"second\""), "{ir}");
    assert_eq!(host.calls(), (1, 2));
}

#[test]
fn a_granted_variable_is_read() {
    let world = Arc::new(World::default());
    let host = reaching("env_var \"TOKEN\" {}", &world);
    let grants = Grants {
        env: vec!["TOKEN".into()],
        ..Grants::default()
    };
    let (ir, found) = built(&host, grants);
    assert!(found.is_empty(), "{found:?}");
    assert!(ir.contains("\"secret\""), "{ir}");
}

#[test]
fn an_answer_in_no_snapshot_of_an_offline_run_is_refused() {
    let world = Arc::new(World::default());
    let host = reaching("fetch_text \"https://db.example.com/t\" {}", &world);
    let grants = Grants {
        net: vec!["db.example.com".into()],
        ..Grants::default()
    };
    let (_, found) = built(&host, grants);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("FLICK-P039 "), "{found:?}");
}

#[test]
fn collect_reaches_what_the_project_grants_once_for_the_group() {
    let world = Arc::new(World::default());
    world.write("/p/stated.txt", "from collect");
    let host = reaching("stated {}", &world);
    let (ir, found) = built(&host, reads(&["stated.txt"]));
    assert!(found.is_empty(), "{found:?}");
    assert!(ir.contains("\"from collect\""), "{ir}");
    assert_eq!(host.calls(), (1, 1));
    // a changed file: collect again, and the state it gives expands again
    world.write("/p/stated.txt", "changed");
    let (ir, _) = built(&host, reads(&["stated.txt"]));
    assert!(ir.contains("\"changed\""), "{ir}");
    assert_eq!(host.calls(), (2, 2));
    let (_, found) = built(&reaching("stated {}", &world), Grants::default());
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("FLICK-P038 `stated {}`")),
        "{found:?}"
    );
}
