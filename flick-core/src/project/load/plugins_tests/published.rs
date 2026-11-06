//! What a plugin publishes of the states of its groups: a module sees the
//! names of its own and of the modules it imports, from any group, a kept
//! expansion holds only while what it was told stays the same, and an
//! expression that is the whole body of a `let` gets its signature.

use std::path::PathBuf;

use super::{Memory, host, library, show};
use crate::ir;
use crate::project::load::compile;
use crate::source::SourceMap;

const NOTES: &str = "module App.Notes exposing (..)\n\nimport Pairs exposing (plugin note)\n\nnote greeting \"hello\"\n";

const MAIN: &str = "module App.Main exposing (..)

import Pairs exposing (plugin heard, plugin above)
import App.Notes as N exposing (greeting)

bare : String
let bare = heard greeting

qualified : String
let qualified = heard N.greeting

stray : String
let stray = heard nothing

signed : String
let signed = above {}

inside : String
let inside = String.trim (above {})

echo : String
let echo = greeting
";

const TEST: &str = "module App.NotesTest exposing (..)

import Pairs exposing (plugin heard)
import App.Notes exposing (greeting)

from_tests : String
let from_tests = heard greeting

echo : String
let echo = greeting
";

fn noting(notes: &str) -> Memory {
    let host = host(MAIN);
    host.write("/p/src/App/Notes.fl", notes);
    host.write("/p/test/App/NotesTest.fl", TEST);
    host
}

/// The IR of each module by name, and each diagnostic.
fn built(host: &Memory) -> (Vec<(String, String)>, Vec<String>) {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/test/App/NotesTest.fl"),
    ];
    let (diagnostics, compilation) = compile(host, &mut sources, Some(&project), &roots);
    let shown = diagnostics.iter().map(|d| show(&sources, d)).collect();
    let irs = (compilation.modules.iter())
        .map(|m| (m.name.clone(), ir::print(&m.ir)))
        .collect();
    (irs, shown)
}

fn ir<'a>(irs: &'a [(String, String)], module: &str) -> &'a str {
    let found = irs.iter().find(|(name, _)| name == module);
    found.map_or_else(|| panic!("no {module} in {irs:?}"), |(_, ir)| ir)
}

#[test]
fn a_module_hears_what_was_published_of_the_names_it_sees() {
    let host = noting(NOTES);
    let (irs, found) = built(&host);
    assert!(found.is_empty(), "{found:?}");
    let main = ir(&irs, "App.Main");
    for (name, value) in [
        ("bare", "\"hello\""),
        ("qualified", "\"hello\""),
        ("stray", "\"unheard\""),
        ("signed", "\"signed : String\""),
        ("inside", "\"none\""),
    ] {
        let line = (main.lines())
            .find(|l| l.starts_with(&format!("let {name} =")))
            .unwrap_or_else(|| panic!("no {name} in {main}"));
        assert!(line.contains(value), "{name}: {line}");
    }
    // the tests are a group of their own
    assert!(ir(&irs, "App.NotesTest").contains("\"hello\""), "{irs:?}");
}

#[test]
fn a_kept_expansion_hears_again_when_what_was_published_changes() {
    let host = noting(NOTES);
    built(&host);
    let (_, first) = host.calls();
    // nothing changed: every expansion is kept
    built(&host);
    assert_eq!(host.calls().1, first);
    host.write("/p/src/App/Notes.fl", &NOTES.replace("hello", "goodbye"));
    let (irs, found) = built(&host);
    assert!(found.is_empty(), "{found:?}");
    assert!(ir(&irs, "App.Main").contains("\"goodbye\""), "{irs:?}");
    assert!(ir(&irs, "App.NotesTest").contains("\"goodbye\""), "{irs:?}");
}
