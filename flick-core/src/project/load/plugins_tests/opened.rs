//! A block that opens a module: the names the module exports are in sight in
//! the user's code of the block, and nowhere else.

use super::{host, library, report, roots, show};
use crate::ir;
use crate::project::load::{check, compile};
use crate::source::SourceMap;

const HEAD: &str = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin opens)\n\n";

#[test]
fn a_block_that_opens_a_module_brings_its_names_into_sight() {
    // `version` is of `Pairs`, which the file imports for the head alone
    let main = format!("{HEAD}next : Float\nlet next = opens {{ version + 1.0 }}\n");
    assert_eq!(report(&main), [""; 0]);
    // the code built names the module's value
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (_, compilation) = compile(&host(&main), &mut sources, Some(&project), &roots());
    let printed = ir::print(&compilation.modules[0].ir);
    assert!(printed.contains("Pairs.version"), "{printed}");
    // not outside the block
    let outside = format!("{HEAD}next : Float\nlet next = opens {{ 1.0 }} + version\n");
    let found = report(&outside);
    assert_eq!(found, ["FLICK-T001 `version`: cannot find value `version`"]);
}

#[test]
fn a_local_hides_a_name_of_the_module_a_block_opens() {
    let main = format!(
        "{HEAD}label : String\nlet label = (fun version -> opens {{ version ++ \"!\" }}) \"one\"\n"
    );
    assert_eq!(report(&main), [""; 0]);
    // and the module's name, a `Float`, hides one of the file
    let file = format!(
        "{HEAD}version : String\nlet version = \"file\"\n\nnext : Float\nlet next = opens {{ version + 1.0 }}\n"
    );
    assert_eq!(report(&file), [""; 0]);
}

/// `Pairs` with a type whose values a block applies through `run`.
const JOBS: &str = "module Pairs exposing (..)\n\ntype Job a = Job a\n\nrun : Job a -> a -> Float\nlet run _ _ = 1.0\n\njob : Job Float\nlet job = Job 2.0\n";

fn with_jobs(main: &str) -> super::Memory {
    let host = host(main);
    host.write("/pairs/src/Pairs.fl", JOBS);
    host
}

#[test]
fn a_block_applies_a_value_through_the_function_it_names() {
    let head = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin runs)\n\n";
    // `job` is no function: in the block it is applied through `Pairs.run`
    let main = format!("{head}done : Float\nlet done = runs {{ job 3.0 }}\n");
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) =
        compile(&with_jobs(&main), &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let printed = ir::print(&compilation.modules[0].ir);
    assert!(printed.contains("((Pairs.run Pairs.job) 3)"), "{printed}");
    // the argument is the one the function asks for
    let wrong = format!("{head}done : Float\nlet done = runs {{ job \"three\" }}\n");
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&with_jobs(&wrong), &mut sources, Some(&project), &roots());
    let found: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert_eq!(
        found,
        ["FLICK-T003 `\"three\"`: expected `Float`, found `String`"]
    );
    // outside the block the value is what it is
    let outside =
        format!("{head}import Pairs exposing (job)\n\ndone : Float\nlet done = job 3.0\n");
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&with_jobs(&outside), &mut sources, Some(&project), &roots());
    let found: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert_eq!(
        found,
        [
            "FLICK-T003 `job 3.0`: this is not a function: it has type `Job Float`, so it cannot take another argument"
        ]
    );
}
