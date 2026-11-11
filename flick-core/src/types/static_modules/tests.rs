use crate::diagnostics::{Severity, codes};
use crate::source::SourceMap;

#[test]
fn single_file_hom_captures_only_earlier_declarations() {
    let source = "module App.Factory exposing (Make)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 1\nmodule Make(K : Input) : Output =\n  answer : Int\n  let answer = K.seed + offset\n";
    let mut sources = SourceMap::default();
    let file = sources.add("App/Factory.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let late = source.replace("offset : Int\nlet offset = 1\n", "");
    let file = sources.add(
        "App/Late.fl",
        format!("{late}offset : Int\nlet offset = 1\n"),
    );
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.severity == Severity::Error
            && d.code == codes::T001
            && d.message.contains("offset")),
        "{diagnostics:?}"
    );
}
