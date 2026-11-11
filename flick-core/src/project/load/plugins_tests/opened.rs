//! A block that opens a module: the names the module exports are in sight in
//! the user's code of the block, and nowhere else.

use super::{host, library, report, roots};
use crate::ir;
use crate::project::load::compile;
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
