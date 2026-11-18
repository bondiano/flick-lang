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

const PORTS: &str = "module App.Ports exposing (Port, Positive)\n\nrefined type Port = Float where fun n -> n > 0 && n < 65536\n\nrefined type Positive = Float where fun n -> n > 0\n";

const AGAIN: &str = "module App.Again exposing (Port)\n\nimport App.Ports exposing (Port)\n";

const USES: &str = "module App.Main exposing (..)\n\nimport App.Ports as P exposing (Port)\nimport App.Again as A\n\nas_port : Float -> Option Port\nlet as_port n = if n > 0 && n < 65536 then Some (Port.prove n) else None\n\naliased : Float -> Option P.Port\nlet aliased n = if P.Port.check n then Some (P.Port.prove n) else None\n\nagain : Float -> Option A.Port\nlet again n = if n > 0 && n < 65536 then Some (A.Port.prove n) else None\n\npositive : Port -> P.Positive\nlet positive p = P.Positive.prove (Port.value p)\n";

/// Each diagnostic of checking the modules `files` and the main module with
/// one host, which keeps the interfaces of a check for the next.
fn checks(host: &super::Memory, files: &[(&str, &str)]) -> Vec<String> {
    use super::{check, library, roots, show};
    use crate::source::SourceMap;
    for (path, text) in files {
        host.write(path, text);
    }
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(host, &mut sources, Some(&project), &roots());
    found.iter().map(|d| show(&sources, d)).collect()
}

#[test]
fn an_imported_invariant_proves_through_imports_aliases_and_re_exports() {
    let host = super::host(USES);
    let files = [
        ("/p/src/App/Ports.fl", PORTS),
        ("/p/src/App/Again.fl", AGAIN),
    ];
    assert_eq!(checks(&host, &files), [""; 0]);
    // the interfaces kept from the first check prove the same
    assert_eq!(checks(&host, &[]), [""; 0]);
    let unproved = USES.replace(
        "if n > 0 && n < 65536 then Some (Port.prove n)",
        "if n > 0 then Some (Port.prove n)",
    );
    host.write("/p/src/App/Main.fl", &unproved);
    assert_eq!(
        checks(&host, &[]),
        [
            "FLICK-T077 `Port.prove n`: cannot prove the check of `Port` holds for this value: nothing here establishes `n < 65536`; `Port.parse n` checks it when the program runs"
        ]
    );
}

#[test]
fn an_edited_check_invalidates_what_importers_proved() {
    let host = super::host(USES);
    let files = [
        ("/p/src/App/Ports.fl", PORTS),
        ("/p/src/App/Again.fl", AGAIN),
    ];
    assert_eq!(checks(&host, &files), [""; 0]);
    // only the check changes: no signature does, the proofs do; a fact of
    // `Port.check` and the lower bound `Positive` reads still prove theirs
    let narrower = PORTS.replace("n < 65536", "n < 1024");
    let found = checks(&host, &[("/p/src/App/Ports.fl", &narrower)]);
    let open = "cannot prove the check of `Port` holds for this value: nothing here establishes `n < 1024`; `Port.parse n` checks it when the program runs";
    assert_eq!(
        found,
        [
            format!("FLICK-T077 `Port.prove n`: {open}"),
            format!("FLICK-T077 `A.Port.prove n`: {open}"),
        ]
    );
}

/// An error unrelated to the refined types of the module.
const BROKEN: &str = "\nbroken : Int\nlet broken = \"x\"\n";

/// The errors of `BROKEN`.
const ITS_ERRORS: [&str; 2] = [
    "FLICK-R014 `broken`: `broken` is never used",
    "FLICK-T003 `\"x\"`: expected `Int`, found `String`",
];

#[test]
fn a_module_with_errors_publishes_no_invariants_of_its_own() {
    let host = super::host(USES);
    let broken = format!("{PORTS}{BROKEN}");
    let files = [
        ("/p/src/App/Ports.fl", broken.as_str()),
        ("/p/src/App/Again.fl", AGAIN),
    ];
    // the importer is still checked, but no type of `Ports` has a `prove`
    let none =
        |ty: &str| format!("FLICK-T001 `{ty}.prove`: `{ty}` is a type, not a module or a service");
    let mut expected = ["Port", "P.Port", "A.Port", "P.Positive"]
        .map(none)
        .to_vec();
    expected.extend(ITS_ERRORS.map(str::to_owned));
    assert_eq!(checks(&host, &files), expected);
}

#[test]
fn a_function_called_prove_of_a_module_is_no_proof() {
    // `Port.prove` names the function of the module `Port`, not the proof
    let main = "module App.Main exposing (..)\n\nimport App.Ports exposing (Port)\nimport App.Port as Port\n\nbumped : Float -> Float\nlet bumped n = Port.prove n\n";
    let host = super::host(main);
    let files = [
        ("/p/src/App/Ports.fl", PORTS),
        (
            "/p/src/App/Port.fl",
            "module App.Port exposing (prove)\n\nprove : Float -> Float\nlet prove n = n + 1\n",
        ),
    ];
    assert_eq!(checks(&host, &files), [""; 0]);
    // without the module, the same spelling is the proof, with its obligation
    host.write(
        "/p/src/App/Main.fl",
        &main.replace("import App.Port as Port\n", ""),
    );
    let found = checks(&host, &[]);
    assert!(
        found
            .iter()
            .any(|d| d.starts_with("FLICK-T077 `Port.prove n`")),
        "{found:?}"
    );
}

/// A module that reaches `Port` only through re-exports, one and two deep.
const THROUGH: &str = "module App.Main exposing (..)\n\nimport App.Again as A\nimport App.Third as T\n\nagain : Float -> Option A.Port\nlet again n = if n > 0 && n < 65536 then Some (A.Port.prove n) else None\n\nthird : Float -> Option T.Port\nlet third n = if T.Port.check n then Some (T.Port.prove n) else None\n\nparsed : Float -> Option A.Port\nlet parsed n = match A.Port.parse n with\n  | Ok p -> Some p\n  | Err _ -> None\n\nraw : T.Port -> Float\nlet raw p = T.Port.value p\n";

const THIRD: &str = "module App.Third exposing (Port)\n\nimport App.Again exposing (Port)\n";

#[test]
fn a_re_export_alone_carries_the_companions_and_the_invariant() {
    let host = super::host(THROUGH);
    let files = [
        ("/p/src/App/Ports.fl", PORTS),
        ("/p/src/App/Again.fl", AGAIN),
        ("/p/src/App/Third.fl", THIRD),
    ];
    assert_eq!(checks(&host, &files), [""; 0]);
    // the interfaces kept from the first check carry them too
    assert_eq!(checks(&host, &[]), [""; 0]);
}

#[test]
fn a_module_with_errors_passes_on_the_invariants_it_learned() {
    let host = super::host(THROUGH);
    let broken = format!("{AGAIN}{BROKEN}");
    let files = [
        ("/p/src/App/Ports.fl", PORTS),
        ("/p/src/App/Again.fl", broken.as_str()),
        ("/p/src/App/Third.fl", THIRD),
    ];
    // `Ports` has no errors, so what `Again` learned from it still proves
    assert_eq!(checks(&host, &files), ITS_ERRORS);
}
