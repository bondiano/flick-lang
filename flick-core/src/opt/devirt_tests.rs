use super::super::tests::release;

/// `Default` of `derive_default.fl`, with `GDefault` over `Generic` and the
/// instance of `Float`, and `Point` after it.
const DEFAULT: &str = "module App exposing (..)\n\nimport Generic exposing (Generic(..), Sum(..), Con(..), Field(..))\n\n/// A value of each type: the first constructor, each field its default.\nclass Default a =\n  default : a\n  default : (Generic a rep, GDefault rep) => a\n  let default = from_rep gdefault\n\nclass GDefault rep =\n  gdefault : rep\n\ninstance GDefault Unit =\n  let gdefault = ()\n\ninstance (Default a, GDefault rest) => GDefault (Field l a, rest) =\n  let gdefault = (Field default, gdefault)\n\ninstance GDefault fields => GDefault (Con n fields) =\n  let gdefault = Con gdefault\n\ninstance GDefault a => GDefault (Sum a b) =\n  let gdefault = Inl gdefault\n\ninstance Default Float =\n  let default = 0\n\ntype Point = Point Float Float\n";

/// The line of `js` that defines `name`.
fn line<'a>(js: &'a str, name: &str) -> &'a str {
    let start = format!("export const {name} = ");
    js.lines()
        .find(|l| l.starts_with(&start))
        .unwrap_or_else(|| panic!("no {name} in\n{js}"))
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
