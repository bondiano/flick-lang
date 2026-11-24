//! An accessor is typed as the explicit function it stands for: the
//! checker's own display, the inlay hints of unsigned locals, shows the
//! same type for `.a.b` and `fun r -> r.a.b`, open rows, polymorphic
//! fields and captures included.

use super::tests::fixture_of;

const MAIN: &str = "module App.Main exposing (..)

run : { id : forall a. a -> a } -> { say : (Unit -> String)^c } -> Int
let run poly holder =
  let a = .address.city
  let b = fun r -> r.address.city
  let c = .value
  let d = fun r -> r.value
  let e = .say holder
  let f = (fun r -> r.say) holder
  let g = .id poly
  let h = (fun r -> r.id) poly
  let i = .id
  let j = fun r -> r.id
  0
";

#[test]
fn an_accessor_has_the_type_of_its_explicit_function() {
    let f = fixture_of(&[("src/App/Main.fl", MAIN)]);
    let hints = f.ide().inlay_hints(f.file("App.Main"));
    // the hint of the local `let name`, at the end of its name
    let hint = |name: &str| {
        let end = f.at("App.Main", &format!("let {name} ="), 0)
            + 4
            + u32::try_from(name.len()).expect("small");
        let (_, shown) = (hints.iter())
            .find(|(at, _)| *at == end)
            .unwrap_or_else(|| panic!("a hint for `{name}`"));
        shown.strip_prefix(": ").unwrap_or(shown).to_owned()
    };
    for (accessor, function, want) in [
        ("a", "b", "{ address : { city : a | b } | d } -> a"),
        ("c", "d", "{ value : a | b } -> a"),
        // a closure keeps what it captures
        ("e", "f", "(Unit -> String)^c"),
        // a polymorphic field is used at an instance
        ("g", "h", "a -> a"),
        ("i", "j", "{ id : a | b } -> a"),
    ] {
        let (shown, explicit) = (hint(accessor), hint(function));
        assert_eq!(shown, explicit, "`{accessor}` and `{function}`");
        assert_eq!(shown, want, "`{accessor}`");
    }
}
