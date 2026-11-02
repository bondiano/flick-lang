//! The tables of `sql migration`: their types and companions in the editor.

use super::Target;
use super::tests::fixture_of;
use crate::diagnostics::Diagnostic;

const SCHEMA: &str = "module App.Schema exposing (..)

import Sql exposing (Pg)

/// The people.
sql migration 1 \"people\" {
  create table people (id bigint primary key generated always as identity, name text not null);
}

names : Sql.Template Pg {} { name : String }
let names = sql {
  from p in People
  select p.name
}
";

#[test]
fn a_migrated_table_hovers_and_leads_to_its_create_table() {
    let f = fixture_of(&[("src/App/Schema.fl", SCHEMA)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Schema");
    let at = f.at("App.Schema", "People", 0) + 1;
    let (markdown, _) = ide.hover(file, at).expect("hover");
    assert!(markdown.contains("name : String"), "{markdown}");
    let (symbol, _) = ide.symbol_at(file, at).expect("symbol");
    let definition = ide.definition(&symbol).expect("definition");
    let declared = f.at("App.Schema", "people (", 0);
    assert_eq!(definition.target, Target::File(file));
    assert_eq!((definition.start, definition.end), (declared, declared + 6));
}
