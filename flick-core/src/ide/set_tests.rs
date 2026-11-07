//! Sets of services and errors in hover and completion.

use super::tests::fixture_of;

#[test]
fn a_set_and_a_signature_that_names_it_read_as_they_are_written() {
    let source = "module App.Sets exposing (..)

type error Boom = Boom

service Log =
  line : String ->{} Unit

type alias Held = {Log; Boom}

run : String ->{...Held} Unit
let run text = Log.line text
";
    let f = fixture_of(&[("src/App/Sets.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Sets");
    let at = |name: &str| u32::try_from(source.find(name).expect("in source")).expect("small");
    let (alias, _) = ide.hover(file, at("Held") + 1).expect("alias hover");
    assert!(alias.contains("type alias Held = {Log; Boom}"), "{alias}");
    let (function, _) = ide.hover(file, at("run :") + 1).expect("function hover");
    assert!(
        function.contains("run : String ->{...Held} Unit"),
        "{function}"
    );
}

#[test]
fn completion_in_a_row_offers_a_set_after_its_dots_and_minus() {
    let source = "module App.Main exposing (..)

service Log =
  line : String ->{} Unit

type alias Held = {Log}

merged : String ->{...Held} Unit
let merged text = Log.line text

taken : String ->{...Held, -Log} Unit
let taken _ = ()
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let names = |at: usize| {
        let offset = u32::try_from(at).expect("small");
        (ide.complete(main, offset, false).into_iter())
            .map(|c| c.label)
            .collect::<Vec<_>>()
    };
    let dots = source.find("...Held}").expect("the set") + 3;
    assert!(names(dots).contains(&"Held".to_owned()));
    let minus = source.find("-Log").expect("the element") + 1;
    assert!(names(minus).contains(&"Log".to_owned()));
}
