//! Blocks inside the user's code of other blocks: each expands in a round of
//! its own, after the block around it.

use std::path::PathBuf;

use super::{host, library, show};
#[cfg(feature = "ide")]
use crate::ide::Ide;
#[cfg(feature = "ide")]
use crate::project::load::analyze;
use crate::project::load::{check, compile};
use crate::source::SourceMap;

const NESTED: &str = "module App.Main exposing (..)

import Pairs exposing (plugin pairs)

inner : { x : { y : Float; blocks : Float }; blocks : Float }
let inner = pairs { x = pairs { y = 1.0 } }

deeper : { x : { y : { z : Float; blocks : Float }; blocks : Float }; blocks : Float }
let deeper = pairs { x = pairs { y = pairs { z = base } } }

base : Float
let base = 2.0
";

fn roots() -> [PathBuf; 1] {
    [PathBuf::from("/p/src/App/Main.fl")]
}

#[test]
fn a_block_inside_another_expands_and_the_cache_keeps_it() {
    let host = host(NESTED);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, _) = compile(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = diagnostics.iter().map(|d| show(&sources, d)).collect();
    assert!(diagnostics.is_empty(), "{shown:#?}");
    // a collect for each round, one for each block
    assert_eq!(host.calls(), (3, 5));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    compile(&host, &mut sources, Some(&project), &roots());
    assert_eq!(host.calls(), (3, 5));
}

#[test]
fn an_error_in_a_nested_block_lands_on_the_users_code() {
    let main = NESTED.replace("y = 1.0", "y = \"one\"");
    let host = host(&main);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert!(shown.iter().any(|s| s.contains("`\"one\"`")), "{shown:#?}");
}

#[cfg(feature = "ide")]
#[test]
fn editors_see_the_users_code_of_a_nested_block() {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let host = host(NESTED);
    let analysis = analyze(&host, &mut sources, Some(&project), &roots());
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App.Main").expect("the module").file;
    let at = u32::try_from(NESTED.find("base }").expect("in the text")).expect("short");
    let (shown, _) = ide.hover(file, at).expect("a hover in the innermost block");
    assert!(shown.contains("Float"), "{shown}");
}
