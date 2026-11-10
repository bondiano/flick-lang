#[cfg(feature = "ide")]
use std::sync::Arc;

use super::{check, host, library, roots};
use crate::diagnostics::{Severity, codes};
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
