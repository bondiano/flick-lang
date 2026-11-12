#[cfg(feature = "ide")]
use std::sync::Arc;

use super::{check, compile, host, library, report, roots};
use crate::diagnostics::{Severity, codes};
use crate::ir;
use crate::source::SourceMap;

#[cfg(feature = "ide")]
#[test]
fn refined_calls_share_predicate_syntax_and_check_each_constant() {
    use flick_plugin::Refinement;

    let source = "module App.Main exposing (..)\nrefined type Even = Float where fun n -> n % 2.0 == 0.0\nlet two = Even.parse 2.0\nlet three = Even.parse 3.0\nlet four = Even.parse 4.0\n";
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = super::super::analyze(&host(source), &mut sources, Some(&project), &roots());
    let rejected: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.code == codes::D006)
        .collect();
    assert_eq!(rejected.len(), 1, "{:?}", analysis.diagnostics);
    assert_eq!(
        &source[rejected[0].span.expect("rejected constant").range()],
        "3.0"
    );
    let predicates: Vec<_> = analysis
        .module("App.Main")
        .expect("checked module")
        .typing
        .call_sites
        .iter()
        .filter_map(|call| match &call.refinement {
            Some(Refinement::Predicate(expression)) => Some(&expression.nodes),
            _ => None,
        })
        .collect();
    assert_eq!(predicates.len(), 3);
    assert!(predicates.windows(2).all(|p| Arc::ptr_eq(p[0], p[1])));
}

#[test]
fn refined_predicates_keep_exact_integer_literal_operations() {
    let source = "module App.Main exposing (..)\nrefined type Impossible = Float where fun n -> n > 0.0 && 1n == 2n\nrefined type Possible = Float where fun n -> n > 0.0 && 1n < 2n\nlet rejected = Impossible.parse 3.0\nlet accepted = Possible.parse 4.0\n";
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let diagnostics = check(&host(source), &mut sources, Some(&project), &roots());
    let rejected: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::D006)
        .collect();
    assert_eq!(rejected.len(), 1, "{diagnostics:?}");
    assert_eq!(
        &source[rejected[0].span.expect("rejected constant").range()],
        "3.0"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error || d.code == codes::D006)
    );
}

/// `refined type` is a declaration of std's plugin: a type of its own over
/// the base, and its check, `parse` and `value` as code of the language.
#[test]
fn a_refined_type_is_the_code_of_its_plugin() {
    let source = "module App.Main exposing (..)\n\nrefined type Port = Float where fun n -> n > 0\n  deriving Eq, Show\n";
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) = compile(&host(source), &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let printed = ir::print(&compilation.modules[0].ir);
    for line in [
        "constructor App.Main.Port.Port$Make/1",
        "let Port$check = (fun n -> ",
        "let Port$parse = (fun value -> ",
        "let Port$value = (fun (Port$Make value) -> value)",
        "instance Show Port",
        "instance Eq Port",
    ] {
        assert!(printed.contains(line), "{line} is not in\n{printed}");
    }
}

#[test]
fn what_a_refined_declaration_lacks_is_said_where_it_is_missing() {
    let head = "module App.Main exposing (..)\n\n";
    let of = |decl: &str| report(&format!("{head}{decl}\n"));
    assert_eq!(
        of("refined type Port = Float"),
        ["FLICK-P036 `Float`: plugin `std`: expected `where` and the check of the type"]
    );
    assert_eq!(
        of("refined type Port where fun n -> n > 0"),
        ["FLICK-P036 `where`: plugin `std`: expected `=` and the base type"]
    );
    assert_eq!(
        of("refined type Port = Float where fun n -> n > 0 deriving Eq { strict = true }"),
        ["FLICK-P036 `{`: plugin `std`: a class of a refined type takes no arguments"]
    );
    // the check is the user's code: its type is theirs to get right
    assert_eq!(
        of("refined type Port = Float where fun n -> n + 1"),
        ["FLICK-T003 `n + 1`: expected `Bool`, found `Float`"]
    );
}
