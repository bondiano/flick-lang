use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::find;
use crate::diagnostics::codes;
use crate::project::{Host, check, discover};
use crate::source::SourceMap;

struct Memory(BTreeMap<PathBuf, String>);

impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";
const SERVER: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\ncontract = { path = \"../contract\" }\n";

fn memory(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| (Path::new("/w").join(p), (*t).to_owned()))
            .collect(),
    )
}

fn root(members: &str) -> String {
    format!("schema-version = 1\n[workspace]\nmembers = {members}\n")
}

#[test]
fn members_follow_their_path_dependencies() {
    let host = memory(&[
        ("flick.toml", &root("[\"server\", \"contract\"]")),
        ("server/flick.toml", SERVER),
        ("contract/flick.toml", LIB),
    ]);
    let mut sources = SourceMap::default();
    let workspace = find(&host, &mut sources, Path::new("/w"))
        .expect("valid")
        .expect("a workspace");
    assert_eq!(workspace.names(), ["contract", "server"]);
    assert_eq!(workspace.member("server"), Some(Path::new("/w/server")));
    let inside = find(&host, &mut sources, Path::new("/w/server/src")).expect("valid");
    assert!(inside.is_some());
    let host = memory(&[
        ("flick.toml", &root("[\"server\"]")),
        ("server/flick.toml", SERVER),
    ]);
    let outside = find(&host, &mut sources, Path::new("/w/contract")).expect("valid");
    assert!(outside.is_none());
}

#[test]
fn a_cycle_or_a_member_without_a_project_is_an_error() {
    let cyclic = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nserver = { path = \"../server\" }\n";
    for files in [
        vec![
            ("flick.toml", root("[\"server\", \"contract\"]")),
            ("server/flick.toml", SERVER.to_owned()),
            ("contract/flick.toml", cyclic.to_owned()),
        ],
        vec![("flick.toml", root("[\"server\"]"))],
    ] {
        let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
        let err = find(&memory(&files), &mut SourceMap::default(), Path::new("/w"))
            .expect_err("invalid workspace");
        assert_eq!(err.code, codes::P031);
    }
}

#[test]
fn a_path_dependency_is_compiled_from_its_sources() {
    let host = memory(&[
        (
            "contract/flick.toml",
            "schema-version = 1\n[project]\nkind = \"lib\"\nsource-dir = \"lib\"\n",
        ),
        (
            "contract/lib/Shared.fl",
            "module Shared exposing (answer)\n\nanswer : Int\nlet answer = 42\n",
        ),
        ("server/flick.toml", SERVER),
        (
            "server/src/Main.fl",
            "module Main exposing (twice)\n\nimport Shared\n\ntwice : Int\nlet twice = Shared.answer + Shared.answer\n",
        ),
    ]);
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/w/server"), None)
        .expect("valid")
        .expect("a project");
    let diagnostics = check(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/w/server/src/Main.fl")],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_path_dependency_must_be_a_library() {
    let host = memory(&[
        (
            "contract/flick.toml",
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"Main\"\n",
        ),
        ("server/flick.toml", SERVER),
        (
            "server/src/Main.fl",
            "module Main exposing (x)\n\nx : Int\nlet x = 1\n",
        ),
    ]);
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, Path::new("/w/server"), None)
        .expect("valid")
        .expect("a project");
    let diagnostics = check(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/w/server/src/Main.fl")],
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::P031),
        "{diagnostics:?}"
    );
}
