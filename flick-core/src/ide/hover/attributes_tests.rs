use std::fmt::Write as _;

use crate::ide::tests::fixture_of;
use crate::types::lang::{self, Lang};

#[test]
fn attribute_hover_explains_all_attributes_and_derive_providers() {
    let source = "module App.Main exposing (..)\n\n@maker\nlet make = 1\n\n@cell\ntype Slot a\n\n@derive\nclass First a\n\n@derive std\nclass Second a\n\n@implicit\ninstance Into Int Float =\n  let into n = Int.to_float n\n\n@label gauge\ntype Gauge = Gauge Float\n\n@owner\nservice Own =\n  tick : Unit ->{} Float\n\n@sync reader\nservice Peek =\n  see : Unit ->{} Float\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (needle, word, expected) in [
        ("@maker", "@maker", "atom, action or view"),
        ("@cell", "@cell", "hold one value each"),
        ("@derive\n", "@derive", "plugin of the class's package"),
        ("@derive std", "@derive", "`std`"),
        ("std\nclass", "std", "deriving"),
        ("@implicit", "@implicit", "automatic conversion"),
        ("@label", "@label", "carry their names"),
        ("gauge\n", "gauge", "carry their names"),
        ("@owner", "@owner", "owns what is made under it"),
        ("@sync", "@sync", "run at once"),
        ("reader\n", "reader", "only reads"),
    ] {
        let start = u32::try_from(source.find(needle).expect("attribute")).expect("small");
        for offset in start..start + u32::try_from(word.len()).expect("small") {
            let (markdown, range) = ide.hover(file, offset).expect(needle);
            assert!(markdown.contains(expected), "{needle}: {markdown}");
            assert_eq!(
                range,
                (start, start + u32::try_from(word.len()).expect("small"))
            );
        }
    }
}

#[test]
fn attribute_hover_documents_every_known_lang_item() {
    // User modules cannot register lang items, but the editor still explains
    // them while a file is being edited, including when checking reports R016.
    let mut source = String::from("module App.Main exposing (..)\n");
    for (index, lang) in Lang::ALL.iter().enumerate() {
        writeln!(source, "@lang {}\nlet marked_{index} = ()", lang.name()).expect("write");
    }
    let f = fixture_of(&[("src/App/Main.fl", &source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for &lang in Lang::ALL {
        let attr = format!("@lang {}\n", lang.name());
        let start = u32::try_from(source.find(&attr).expect("attribute")).expect("small");
        let end = start + 6 + u32::try_from(lang.name().len()).expect("small");
        for offset in [start, start + 1, start + 6, end - 1] {
            let (markdown, range) = ide
                .hover(file, offset)
                .unwrap_or_else(|| panic!("hover for {}", lang.name()));
            assert!(markdown.contains("built-in modules only"), "{markdown}");
            assert!(
                markdown.contains(&format!("@lang {}", lang.name())),
                "{markdown}"
            );
            if let Some(item) = lang::table().find(lang) {
                let name = if matches!(lang, Lang::Prelude | Lang::Instances) {
                    item.module.clone()
                } else {
                    format!("{}.{}", item.module, item.name)
                };
                assert!(
                    markdown.contains(&format!("`{name}`")),
                    "{lang:?}: {markdown}"
                );
            }
            assert_eq!(
                range,
                if offset < start + 5 {
                    (start, start + 5)
                } else {
                    (start + 6, end)
                }
            );
        }
    }
}

#[test]
fn lang_attribute_hover_includes_action_documentation() {
    let source = "module App.Main exposing (..)\n@lang action\nlet action_type = ()\n@lang action_concurrent\nlet concurrent = ()\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (needle, expected) in [
        ("action\n", "Action.Action"),
        ("action_concurrent", "Action.concurrent"),
    ] {
        let at = u32::try_from(source.find(needle).expect("item")).expect("small");
        let (markdown, _) = ide.hover(file, at).expect("lang item");
        assert!(markdown.contains(expected), "{markdown}");
        #[cfg(feature = "native-project")]
        assert!(
            markdown.contains(if needle == "action\n" {
                "snapshot"
            } else {
                "`limit` tasks at once"
            }),
            "{markdown}"
        );
    }
}

#[test]
fn lang_attribute_hover_reaches_class_members_and_constructors() {
    let source = "module App.Main exposing (..)\nclass Factory a =\n  @lang pure\n  make : a\ntype Choice = @lang none Empty | @lang some Full Int\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (word, expected) in [
        ("pure", "Prelude.pure"),
        ("none", "Prelude.None"),
        ("some", "Prelude.Some"),
    ] {
        let start = u32::try_from(source.find(word).expect("lang item")).expect("small");
        let (markdown, range) = ide.hover(file, start).expect("nested attribute");
        assert!(markdown.contains(expected), "{markdown}");
        assert_eq!(
            range,
            (start, start + u32::try_from(word.len()).expect("small"))
        );
    }
}

#[test]
fn attribute_hover_handles_incomplete_attributes_without_matching_plain_text() {
    let source = "module App.Main exposing (..)\n// @maker @lang action\nlet text = \"@implicit @lang action_concurrent\"\nlet maker = 1\n@unknown\nlet x = 2\n@lang unknown_item\nlet y = 3\n@lang\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for needle in ["@maker", "@unknown", "unknown_item"] {
        let at = u32::try_from(source.find(needle).expect("text")).expect("small");
        assert!(ide.hover(file, at).is_none(), "{needle}");
    }
    let space = u32::try_from(source.find("@lang unknown").expect("attribute") + 5).expect("small");
    assert!(
        ide.hover(file, space).is_none(),
        "space between attribute tokens"
    );
    let at = u32::try_from(source.find("@implicit").expect("string contents")).expect("small");
    if let Some((markdown, _)) = ide.hover(file, at) {
        assert!(!markdown.contains("automatic conversion"), "{markdown}");
    }
    for at in [
        source.find("@lang unknown").expect("unknown item"),
        source.rfind("@lang").expect("incomplete attribute"),
    ] {
        let (markdown, _) = ide
            .hover(file, u32::try_from(at).expect("small"))
            .expect("@lang");
        assert!(markdown.contains("built-in modules only"), "{markdown}");
    }
    let at = u32::try_from(source.find("maker =").expect("plain name")).expect("small");
    let (markdown, _) = ide.hover(file, at).expect("ordinary name");
    assert!(markdown.contains("maker : Int"), "{markdown}");
    assert!(!markdown.contains("atom, action or view"), "{markdown}");
}

#[test]
fn attribute_hover_explains_the_typescript_form_of_a_type() {
    let source = "module App.Main exposing (..)\n\n@ts \"$Dayjs\"\n@ts import \"dayjs\" \"Dayjs\"\ntype Moment\n\n@ts none\ntype Cursor\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    let at =
        |needle: &str| u32::try_from(source.find(needle).expect("in the source")).expect("small");
    for (needle, shown) in [
        ("@ts \"$Dayjs\"", "@ts \"$Dayjs\""),
        ("@ts import", "@ts import \"dayjs\" \"Dayjs\""),
        ("@ts none", "@ts none"),
    ] {
        let start = at(needle);
        let (markdown, range) = ide.hover(file, start + 1).expect(needle);
        assert!(
            markdown.contains(&format!("```flick\n{shown}\n```")),
            "{markdown}"
        );
        assert!(markdown.contains("is in TypeScript"), "{markdown}");
        assert_eq!(range, (start, start + 3));
    }
    // an argument is hovered by itself, with the same explanation
    let module = at("\"dayjs\"");
    let (markdown, range) = ide.hover(file, module + 2).expect("the module");
    assert!(markdown.contains("is in TypeScript"), "{markdown}");
    assert_eq!(range, (module, module + 7));
}
