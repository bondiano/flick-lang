use crate::source::SourceMap;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;

#[test]
fn a_file_structure_keeps_tokens_and_original_declaration_positions() {
    let text = "module App.Example exposing (answer) =\n  // kept at its source offset\n  answer : Int\n  let answer = 42\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Example.fl", text);
    let raw = parse(text, file);
    assert!(raw.diagnostics.is_empty(), "{:?}", raw.diagnostics);
    assert_eq!(
        raw.tree.nodes().next().expect("file declaration").kind,
        K::InlineModuleDecl
    );
    let semantic = raw.into_module();
    assert_eq!(semantic.tree.text(text), text);
    assert_eq!(
        semantic
            .tree
            .nodes()
            .next()
            .expect("normalized header")
            .kind,
        K::ModuleHeader
    );
    let declaration = semantic
        .tree
        .nodes()
        .find(|n| n.kind == K::LetDecl)
        .expect("lifted value declaration");
    assert_eq!(
        declaration.range().0 as usize,
        text.find("let answer").expect("original declaration")
    );
    let diagnostics = crate::check(&sources, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
