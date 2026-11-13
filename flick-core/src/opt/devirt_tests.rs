use super::super::tests::release;

/// `Default` of `derive_default.fl`, with `GDefault` over `Generic` and the
/// instance of `Float`, and `Point` after it.
const DEFAULT: &str = "module App exposing (..)\n\nimport Generic exposing (Generic(..), Sum(..), Con(..), Field(..))\n\n/// A value of each type: the first constructor, each field its default.\nclass Default a =\n  default : a\n  default : (Generic a rep, GDefault rep) => a\n  let default = from_rep gdefault\n\nclass GDefault rep =\n  gdefault : rep\n\ninstance GDefault Unit =\n  let gdefault = ()\n\ninstance (Default a, GDefault rest) => GDefault (Field l a, rest) =\n  let gdefault = (Field default, gdefault)\n\ninstance GDefault fields => GDefault (Con n fields) =\n  let gdefault = Con gdefault\n\ninstance GDefault a => GDefault (Sum a b) =\n  let gdefault = Inl gdefault\n\ninstance Default Float =\n  let default = 0\n\ntype Point = Point Float Float\n";

/// The declaration of `name`, including an initializer the printer wraps.
fn line<'a>(js: &'a str, name: &str) -> &'a str {
    let start = format!("export const {name} = ");
    let at = js
        .find(&start)
        .unwrap_or_else(|| panic!("no {name} in\n{js}"));
    let rest = &js[at..];
    let end = rest
        .find(";\n")
        .unwrap_or_else(|| panic!("no end of {name} in\n{js}"));
    &rest[..=end]
}

#[test]
fn a_default_through_generic_is_the_method_written_by_hand() {
    let derived = release(&format!("{DEFAULT}  deriving Default\n"));
    let hand = release(&format!(
        "{DEFAULT}\ninstance Default Point =\n  let default = Point 0 0\n"
    ));
    let method = "$i_Default_Point$default";
    assert_eq!(line(&derived, method), line(&hand, method));
    assert!(!derived.contains("get default()"), "{derived}");
}

#[test]
fn a_match_of_a_known_constructor_is_the_arm_it_takes() {
    let js = release(
        "module App exposing (pick, pair)\n\n\
         pick : Float\nlet pick =\n  match Some 2.0 with\n  | None -> 0\n  | Some x -> x\n\n\
         pair : Float\nlet pair =\n  match (1.0, Some 3.0) with\n  | (a, Some b) -> a + b\n  | _ -> 0\n",
    );
    assert_eq!(line(&js, "pick"), "export const pick = 2;");
    assert_eq!(line(&js, "pair"), "export const pair = 4;");
}

#[test]
fn a_known_default_is_taken_apart_where_it_is_and_named_elsewhere() {
    let js = release(&format!(
        "{DEFAULT}  deriving Default\n\n\
         type Shape = Rect Point Point | Circle Float\n  deriving Default\n\n\
         sum : Float\nlet sum =\n  let Point x y = (default : Point)\n  x + y\n\n\
         whole : Point\nlet whole = default\n\n\
         kind : String\nlet kind =\n  match (default : Shape) with\n  | Circle _ -> \"circle\"\n  | Rect _ _ -> \"rect\"\n"
    ));
    assert_eq!(line(&js, "sum"), "export const sum = 0;");
    // the shared value, not a copy of it
    assert_eq!(
        line(&js, "whole"),
        "export const whole = $i_Default_Point$default;"
    );
    assert_eq!(line(&js, "kind"), "export const kind = \"rect\";");
    // a default that uses another default is a value too
    assert!(
        line(&js, "$i_Default_Shape$default").contains("Rect#"),
        "{js}"
    );
}
