use crate::ide::tests::fixture_of;

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
