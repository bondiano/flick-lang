use super::super::tests::fixture_of;

const MAIN: &str = "module App.Main exposing (..)

/// A person.
type alias User =
  {
    /// The full name.
    name : String;
    /// Age in years.
    age : Number;
  }

type Shape =
  /// A round one, by its radius.
  | Circle Number
  | Square Number

older : User -> Number
let older user = user.age + 1

make : String -> User
let make n = { name = n; age = 0 }

rename : User -> User
let rename u = { u with age = 2 }

years : User -> Number
let years { age } = age

area : Shape -> Number
let area s =
  match s with
  | Circle r -> r
  | Square a -> a

page : Ui.View Unit
let page = Ui.view (fun () -> ui { div { class_name = \"page\" } [] })
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
    let want = "```flick\nage : Number\n```\n\nAge in years.\n\nfield of `User` from `App.Main`";
    // the declaration, an access, a literal, an update and a pattern
    for nth in 0..5 {
        assert_eq!(hover("age", nth), want, "occurrence {nth}");
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
fn a_codec_label_shows_its_decoded_field_type() {
    let source = "module App.Main exposing (..)

refined type Email = String where Checks.email
  deriving Schema

type alias Profile = {
  email : Email;
  nickname : Option String;
} deriving Schema

profile_schema : Schema Profile
let profile_schema = schema Profile

profile_json : PureCodec Json Profile
let profile_json = codec profile_schema {
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
                format!("```flick\n{label} : {ty}\n```\n\nfield of `Profile` from `App.Main`")
            );
            assert_eq!(range, (start, end));
        }
    }
}
