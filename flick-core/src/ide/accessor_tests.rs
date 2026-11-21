//! Accessors `.a.b` in every editor query: none fails on them, on finished
//! code or on a step still being written. What the queries show for an
//! accessor is not pinned here.

use super::Ide;
use super::tests::fixture_of;
use crate::source::FileId;

const MAIN: &str = "module App.Main exposing (..)

type alias Address = { city : String }

type alias User = { name : String; address : Address }

get_city : User -> String
let get_city = .address.city

cities : User -> List User -> List String
let cities first users =
  let get = .name
  let names = users |> List.map get
  List.map .address.city users ++ [.address.city first, first |> .name] ++ names
";

/// Every query at every offset of `module`, and the queries of the file.
fn query_everywhere(ide: &Ide<'_>, file: FileId, len: usize) {
    ide.semantic_tokens(file);
    ide.inlay_hints(file);
    ide.folding_ranges(file);
    ide.document_symbols(file);
    for offset in 0..=u32::try_from(len).expect("small") {
        ide.hover(file, offset);
        ide.complete(file, offset, false);
        ide.complete(file, offset, true);
        ide.signature_help(file, offset);
        ide.type_definition(file, offset);
        ide.field_definition(file, offset);
        ide.actions(file, offset, offset);
        let _ = ide.renamable(file, offset);
        if let Some((symbol, _)) = ide.symbol_at(file, offset) {
            ide.definition(&symbol);
            ide.references(&symbol);
        }
    }
}

#[test]
fn every_query_answers_on_accessors() {
    let f = fixture_of(&[("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let diagnostics = &ide.analysis.diagnostics;
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    query_everywhere(&ide, f.file("App.Main"), MAIN.len());
}

#[test]
fn every_query_answers_on_an_unfinished_step() {
    let source = MAIN.replace(
        "List.map .address.city users ++",
        "List.map .address. users ++",
    );
    let f = fixture_of(&[("src/App/Main.fl", source.as_str())]);
    query_everywhere(&f.ide(), f.file("App.Main"), source.len());
}
