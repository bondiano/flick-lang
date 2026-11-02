//! A type's companions through a module alias: `Api.Public.api` in hover,
//! navigation and completion.

use super::tests::fixture_of;
use super::{CompletionKind, Symbol, Target};
use crate::diagnostics::Diagnostic;

const API: &str = "module App.Api exposing (..)

import HttpApi.Derive exposing (..)

/// The endpoints anyone may call.
type alias Public = {
  forget : Delete \"/notes\" {} Unit {};
} deriving HttpApi
";

const ROUTES: &str = "module App.Routes exposing (group)

import App.Api as Api

let group = Api.Public.api
";

fn labels(cs: Vec<super::Completion>) -> Vec<(String, CompletionKind)> {
    cs.into_iter().map(|c| (c.label, c.kind)).collect()
}

#[test]
fn the_owner_of_a_companion_through_an_alias_has_its_doc_and_definition() {
    let f = fixture_of(&[("src/App/Api.fl", API), ("src/App/Routes.fl", ROUTES)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Routes");
    let public = f.at("App.Routes", "Public.api", 0);
    let (markdown, range) = ide.hover(file, public + 1).expect("hover on Public");
    assert!(
        markdown.contains("The endpoints anyone may call."),
        "{markdown}"
    );
    assert_eq!(range, (public, public + 6));
    let (symbol, _) = ide.symbol_at(file, public + 1).expect("symbol");
    let definition = ide.definition(&symbol).expect("definition");
    assert_eq!(definition.target, Target::File(f.file("App.Api")));
    let api = f.at("App.Routes", "api", 0);
    let (markdown, _) = ide.hover(file, api + 1).expect("hover on api");
    assert!(markdown.contains("deriving HttpApi"), "{markdown}");
    let (symbol, _) = ide
        .symbol_at(file, f.at("App.Routes", "Api.", 0) + 1)
        .expect("module");
    assert_eq!(symbol, Symbol::Module("App.Api".into()));
}

#[test]
fn completion_after_an_alias_lists_the_module_and_after_a_type_its_companions() {
    let f = fixture_of(&[("src/App/Api.fl", API), ("src/App/Routes.fl", ROUTES)]);
    let ide = f.ide();
    let file = f.file("App.Routes");
    // as the server sees it: the text up to the dot, the member cut out
    let module = labels(ide.complete(file, f.at("App.Routes", "Api.Public", 0) + 3, true));
    assert!(
        module.contains(&("Public".into(), CompletionKind::Type)),
        "{module:?}"
    );
    let members = ide.complete(file, f.at("App.Routes", "Public.api", 0) + 6, true);
    let found = labels(members.clone());
    for member in ["api", "endpoints"] {
        assert!(
            found.contains(&(member.into(), CompletionKind::Value)),
            "{found:?}"
        );
    }
    let api = members.iter().find(|c| c.label == "api").expect("api");
    assert!(
        api.documentation
            .as_deref()
            .is_some_and(|d| d.contains("deriving HttpApi")),
        "{api:?}"
    );
    let names = labels(ide.complete(file, f.at("App.Routes", "Api.Public", 0), false));
    assert!(
        names.contains(&("Api".into(), CompletionKind::Module)),
        "{names:?}"
    );
}
