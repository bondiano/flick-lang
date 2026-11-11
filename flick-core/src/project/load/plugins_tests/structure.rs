use std::path::PathBuf;

use super::{check, host, library};
use crate::diagnostics::{Code, Severity, codes};
use crate::source::SourceMap;
use crate::types::Thresholds;

#[test]
fn component_complexity_is_measured_on_the_source_body() {
    let mut source = String::from(
        "module App.Main exposing (..)\nimport Ui exposing (plugin component)\ncomponent badge (p : Bool) =\n",
    );
    for _ in 0..11 {
        source.push_str("  if p then Ui.text \"да\" else ");
    }
    source.push_str("  Ui.text \"нет\"\n");
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let diagnostics = check(
        &host(&source),
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    let found: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::T038)
        .collect();
    assert_eq!(found.len(), 1, "{diagnostics:?}");
    assert_eq!(
        found[0].message,
        "`badge` has a complexity of 12, more than 10"
    );
    assert_eq!(
        &source[found[0].span.expect("component name").range()],
        "badge"
    );
}

#[test]
fn component_structure_uses_project_thresholds_and_suppression() {
    let source = "module App.Main exposing (..)\ncomponent badge (p : Bool) =\n  let label = if p then \"да\" else \"нет\"\n  Ui.text (if p then label else \"—\")\n";
    let run = |limits: Thresholds, allow: Vec<Code>| {
        let mut sources = SourceMap::default();
        let mut project = library(&mut sources);
        project.config.thresholds = limits;
        project.config.allow = allow;
        check(
            &host(source),
            &mut sources,
            Some(&project),
            &[PathBuf::from("/p/src/App/Main.fl")],
        )
    };
    let limits = Thresholds {
        function_lines: 1,
        complexity: 2,
        nesting: 0,
        ..Thresholds::default()
    };
    let diagnostics = run(limits, Vec::new());
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.code, codes::T036 | codes::T037 | codes::T038))
        .collect();
    assert_eq!(warnings.len(), 4, "{diagnostics:?}");
    for d in &warnings {
        let at = &source[d.span.expect("source span").range()];
        assert!(at == "badge" || at == "if", "{d:?}: {at}");
        assert_eq!(d.severity, Severity::Warning);
    }
    let boundary = run(
        Thresholds {
            function_lines: 2,
            complexity: 3,
            nesting: 1,
            ..limits
        },
        Vec::new(),
    );
    assert!(
        !boundary
            .iter()
            .any(|d| matches!(d.code, codes::T036 | codes::T037 | codes::T038)),
        "{boundary:?}"
    );
    let allowed = run(limits, [codes::T036, codes::T037, codes::T038].to_vec());
    assert!(
        !allowed
            .iter()
            .any(|d| matches!(d.code, codes::T036 | codes::T037 | codes::T038)),
        "{allowed:?}"
    );
}
