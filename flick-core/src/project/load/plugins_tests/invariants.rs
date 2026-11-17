//! The invariant a plugin declares for a type of its own: the compiler
//! proves its check wherever the type's constructor is applied, the
//! plugin's own code included, and no plugin gives an invariant to a type
//! another block declares.

use super::report;

const HEAD: &str = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin kept, plugin leaky, plugin claimed, plugin stray, plugin companion, plugin wrapped)\n\n";

#[test]
fn a_type_with_an_invariant_is_made_where_its_check_is_proved() {
    let main = format!(
        "{HEAD}kept Positive {{ Checks.positive }}\n\nmade : Float -> Option Positive\nlet made x = if Positive.check x then Some (Positive.prove x) else None\n\ndirect : Float -> Option Positive\nlet direct x = if Checks.positive x then Some (Positive.prove x) else None\n"
    );
    assert_eq!(report(&main), [""; 0]);
    let unproved = format!(
        "{HEAD}kept Positive {{ Checks.positive }}\n\nmade : Float -> Positive\nlet made x = Positive.prove x\n"
    );
    assert_eq!(
        report(&unproved),
        [
            "FLICK-T077 `Positive.prove x`: cannot prove the check of `Positive` holds for this value: nothing here establishes `Checks.positive x`; `Positive.parse x` checks it when the program runs"
        ]
    );
}

#[test]
fn the_plugin_that_owns_the_constructor_proves_its_check_too() {
    let main = format!("{HEAD}leaky Positive {{ Checks.positive }}\n");
    let found = report(&main);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with("FLICK-T079"), "{found:?}");
    assert!(found[0].contains("`Positive$Make` makes a value only applied to one"));
    assert!(found[1].starts_with("FLICK-T077"), "{found:?}");
}

#[test]
fn no_plugin_gives_an_invariant_with_the_check_of_another_block() {
    let main = format!(
        "{HEAD}claimed Meters\n\ncompanion Meters {{ Checks.positive }}\n\nforged : Float -> Meters\nlet forged x = Meters.prove x\n"
    );
    let found = report(&main);
    assert_eq!(
        found.first().map(String::as_str),
        Some(
            "FLICK-P036 `claimed Meters`: plugin `pairs` gave code that cannot be placed: its invariant stands before a type, or names a check or a value, that its block does not declare"
        ),
        "{found:?}"
    );
}

#[test]
fn no_plugin_gives_an_invariant_to_a_type_of_another_block() {
    // the claim would stand right before the type of the next block
    let main = format!(
        "{HEAD}stray Meters\n\nwrapped Meters\n\nforged : Float -> Meters\nlet forged x = Meters.prove x\n"
    );
    let found = report(&main);
    assert!(
        found
            .iter()
            .any(|d| d.starts_with("FLICK-P036 `stray Meters`")),
        "{found:?}"
    );
    // and it gives no `Meters.prove`
    assert!(
        found
            .iter()
            .any(|d| d.starts_with("FLICK-T001 `Meters.prove`")),
        "{found:?}"
    );
}
