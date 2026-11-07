use super::as_written;
use crate::ide::tests::fixture_of;
use crate::source::SourceMap;
use crate::syntax;
use crate::syntax::kind::SyntaxKind as K;

#[test]
fn a_foreign_function_shows_what_retention_owner_gives_it() {
    let source = "module App.Main exposing (defer, plain)

defer : (Eff r none Unit)^c ->{Scope s | r;} Unit
foreign effect defer from \"./scope.js\" \"defer\"
  retention owner

plain : forall a. a -> a
foreign pure plain from \"./scope.js\" \"plain\"
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    let hover = |needle: &str| {
        let offset = source.find(needle).expect("needle");
        ide.hover(file, u32::try_from(offset).expect("offset"))
            .expect("hover")
            .0
    };
    let deferred = hover("defer :");
    assert!(
        deferred.contains("(Outlives c s, Provided r s, Detached r) =>"),
        "{deferred}"
    );
    let plain = hover("plain :");
    assert!(plain.contains("plain : a -> a"), "{plain}");
}

/// The non-breaking space after the first line is more bytes than characters.
#[test]
fn a_continuation_line_is_dedented_by_characters() {
    let text = "module M exposing (..)\n\nf :\n  Int ->\n\u{a0}\u{a0}\u{a0}Int\n";
    let mut sources = SourceMap::default();
    let file = sources.add("M.fl", text);
    let parsed = syntax::parse(text, file);
    let signature = (parsed.tree.nodes())
        .find(|n| n.kind == K::Signature)
        .expect("the signature");
    let ty = signature.nodes().next().expect("its type");
    assert_eq!(as_written(ty, text), "Int ->\n\u{a0}Int");
}
