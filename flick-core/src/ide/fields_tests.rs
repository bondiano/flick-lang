use super::super::tests::fixture_of;
use crate::ide::{Location, Target};
use crate::types::builtin_sources;

const MAIN: &str = "module App.Main exposing (..)

/// A person.
type alias User =
  {
    /// The full name.
    name : String;
    /// Age in years.
    age : Float;
  }

type Shape =
  /// A round one, by its radius.
  | Circle Float
  | Square Float

older : User -> Float
let older user = user.age + 1

make : String -> User
let make n = { name = n; age = 0 }

rename : User -> User
let rename u = { u with age = 2 }

years : User -> Float
let years { age } = age

area : Shape -> Float
let area s =
  match s with
  | Circle r -> r
  | Square a -> a

page : Ui.View Unit
let page = Ui.view (fun () -> ui { div { class_name = \"page\" } })

small : Check Float
let small =
  Check.between { minimum = 0; maximum = 5; exclusiveMinimum = false; exclusiveMaximum = false }
";

fn hover(needle: &str, nth: usize) -> String {
    let f = fixture_of(&[("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let at = f.at("App.Main", needle, nth) + 1;
    ide.hover(f.file("App.Main"), at)
        .unwrap_or_else(|| panic!("hover {needle} {nth}"))
        .0
}

#[test]
fn a_field_shows_its_type_comment_and_record() {
    let want = "```flick\nage : Float\n```\n\nAge in years.\n\nfield of `User` from `App.Main`";
    // the declaration, an access, a literal, an update and a pattern
    for nth in 0..5 {
        assert_eq!(hover("age", nth), want, "occurrence {nth}");
    }
}

#[test]
fn a_field_leads_to_its_declaration() {
    let f = fixture_of(&[("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    let declared = f.at("App.Main", "age", 0);
    // an access, a literal, an update and a pattern
    for nth in 1..5 {
        let at = ide
            .field_definition(file, f.at("App.Main", "age", nth) + 1)
            .unwrap_or_else(|| panic!("age {nth}"));
        assert_eq!(
            at,
            Location {
                target: Target::File(file),
                start: declared,
                end: declared + 3
            }
        );
    }
    for (label, module) in [("class_name", "Ui"), ("maximum", "Check")] {
        let at = ide
            .field_definition(file, f.at("App.Main", label, 0) + 1)
            .unwrap_or_else(|| panic!("{label}"));
        assert_eq!(at.target, Target::Builtin(module.into()), "{label}");
        let text = builtin_sources()
            .iter()
            .find(|(n, _)| *n == module)
            .expect("builtin")
            .1;
        assert_eq!(&text[at.start as usize..at.end as usize], label);
    }
}

#[test]
fn a_constructor_shows_its_comment() {
    assert!(hover("Circle r", 0).contains("A round one, by its radius."));
}

#[test]
fn a_prop_of_an_element_shows_its_comment() {
    let shown = hover("class_name", 0);
    assert!(
        shown.starts_with("```flick\nclass_name : String\n```"),
        "{shown}"
    );
    assert!(shown.contains("The CSS classes of the element"), "{shown}");
    assert!(
        shown.ends_with("field of `ElementProps` from `Ui`"),
        "{shown}"
    );
}

#[test]
fn a_field_of_a_signature_shows_its_comment() {
    assert_eq!(
        hover("maximum", 0),
        "```flick\nmaximum : Float\n```\n\nThe upper bound.\n\nfield of `between` from `Check`"
    );
}

#[test]
fn a_codec_label_shows_its_field_type() {
    let source = "module App.Main exposing (..)

import Codec

refined type Email = String where Checks.email
  deriving Schema

type alias Profile = {
  email : Email;
  nickname : Option String;
} deriving Schema

profile_json : PureCodec Json Profile
let profile_json = codec Profile.schema {
  email = field \"email_address\" Email.json;
  nickname = field \"nickname\" (option_as_null String.json);
}
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (label, ty) in [("email", "Email"), ("nickname", "Option String")] {
        let start = f.at("App.Main", &format!("{label} ="), 0);
        let end = start + u32::try_from(label.len()).expect("small");
        for offset in start..end {
            let (shown, range) = ide.hover(file, offset).expect("codec field hover");
            assert_eq!(
                shown,
                format!("```flick\n{label} : {ty}\n```\n\nfield of `Profile`")
            );
            assert_eq!(range, (start, end));
        }
    }
}
