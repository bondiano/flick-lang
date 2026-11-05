use super::tests::fixture_of;

#[test]
fn an_enum_constructor_shows_its_literal() {
    let source = "module App.Main exposing (Method(..), Level(..), pick)

untagged type Method = Get = \"GET\" | Post = \"POST\"

untagged type Level = Low = -1 | High = 2

pick : (Method, Level)
let pick = (Post, Low)
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (needle, expected) in [
        ("Get =", "Get = \"GET\""),
        ("Post, Low", "Post = \"POST\""),
        ("Low)", "Low = -1"),
    ] {
        let start = u32::try_from(source.find(needle).expect("hover target")).expect("small");
        let (markdown, _) = ide.hover(file, start + 1).expect(needle);
        assert!(markdown.contains(expected), "{needle}: {markdown}");
    }
}
