//! Cases of the model properties on the errors of a row, (Match-Err) and
//! (Errors-In) of `docs/formal/core.md` §2.4: short runs that end in a form,
//! and a case the properties found, each held to both checkers and to
//! progress and preservation.

use flick_model::generate;

use super::props::{GEN, core, model, progress_and_preservation};

/// (Match-Err): an arm `x` of a `try` takes the errors of the body apart
/// by a `match` over the row the body raises. The body raises `Oops`, so
/// the `match` runs on that error, which a configuration types at the row
/// (Errors-In). The choices are a short run that ends in this form.
#[test]
fn a_try_arm_takes_its_errors_apart_by_their_row() {
    let c = [
        1, 0, 1, 5, 3, 2, 5, 1, 5, 1, 4, 1, 0, 2, 5, 5, 1, 1, 0, 3, 0,
    ];
    let p = generate::program(&c);
    let source = generate::source(&p);
    assert!(
        source.contains("(match x1 with\n          | Bad y2 ->"),
        "{source}"
    );
    assert!(source.contains("(match true with"), "{source}");
    assert!(model(&p).is_ok(), "{source}");
    assert!(core(&source).is_empty(), "{source}");
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{source}");
}

/// (Errors-In) and the row of a `try` arm above its body's: the body
/// raises `Oops` only, the `match` on `x1` names `Bad` too, and its arm
/// `Oops` gives `Oops y2`, a term of one error, the errors of the row
/// `{Bad, Oops}` and takes it apart again. Both checkers accept it, and
/// each state of its run is typed (`docs/formal/core.md` §2.4).
#[test]
fn an_error_is_a_term_of_the_errors_of_its_row() {
    let c = [
        4, 1, 2, 5, 1, 4, 5, 4, 3, 4, 3, 1, 1, 0, 1, 3, 0, 2, 3, 2, 5, 3, 3, 0, 4, 1,
    ];
    let p = generate::program(&c);
    let source = generate::source(&p);
    assert!(source.contains("let w4 : Errors {Bad, Oops} ="), "{source}");
    assert!(source.contains("| Bad y3 ->"), "{source}");
    assert!(!source.contains("(Bad "), "{source}");
    assert!(model(&p).is_ok(), "{source}");
    assert!(core(&source).is_empty(), "{source}");
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{source}");
}

/// The `x` of a `try` in a configuration whose body has stepped past the
/// `Bad` it raised: the row of `x` was the body's, `{Oops, ρ - Bad, …}`,
/// with a variable of its own, and the `match` naming `Bad` grew only the
/// first variable of that row, `ρ`, which the inner `try` takes `Bad` out
/// of. The `x` now takes a row its `match` tells, which holds the body's
/// (`raised ⊆ caught`). A case the model properties found.
///
/// The choices are the picks of the case proptest printed: each below the
/// number of forms then, so forms added later at the end keep the program.
#[test]
fn a_try_arm_row_holds_an_error_its_body_no_longer_raises() {
    let c = [
        0, 0, 0, 1, 1, 1, 9, 1, 15, 0, 0, 0, 0, 7, 11, 2, 0, 7, 14, 1, 0, 0, 17, 0, 7, 0, 10, 0,
        10, 0, 2, 1, 13, 0, 5, 4, 0, 3, 2, 0, 1, 1, 3, 0, 2, 4, 0, 3, 6, 1, 5, 1, 1, 1, 4, 1, 6, 1,
        2, 1, 2, 3, 1, 9, 0, 6, 3, 2, 1, 0, 0, 2, 0, 2, 2, 1, 0, 1, 14, 0, 12, 1, 5, 1, 0, 0, 0,
        12, 19, 2, 2, 0, 3, 5, 0, 1, 3,
    ];
    let p = generate::program(&c);
    let source = generate::source(&p);
    assert!(
        source.contains("| x24 ->\n      (match x24 with\n        | Bad y25 ->"),
        "{source}"
    );
    assert!(model(&p).is_ok(), "{source}");
    assert!(core(&source).is_empty(), "{source}");
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{source}");
}

/// A pattern of one error in a `let` misses the other errors of the row it
/// is matched at: `let Oops z18 = w15` with `w15 : Errors {Bad, Oops}`, a
/// `match` of the (Errors-In) form a change left with one arm. flick-core
/// judged the pattern by the type of `Oops` alone and accepted it, where
/// the model's (Match) did not. A case `checkers_agree` found.
///
/// The choices are the picks of the case proptest printed: each below the
/// number of forms then, so forms added later at the end keep the program.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices of the change as proptest printed them"
)]
fn a_let_of_one_error_misses_the_others_of_its_row() {
    let c = [
        2, 0, 0, 1, 0, 0, 0, 3, 4, 5, 3, 0, 1, 4, 3, 2, 0, 1, 0, 1, 0, 1, 0, 1, 5, 7, 1, 13, 9, 19,
        7, 6, 1, 8, 3, 4, 0, 0, 0, 1, 1, 6, 0, 0, 1, 0, 0, 2, 3, 5, 0, 0, 0, 0, 1, 13, 0, 3, 0, 0,
        0, 0, 10, 0, 0, 0, 0, 1, 1, 8, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 7, 0, 6, 4, 3, 3, 9, 18,
        2, 0, 0, 3, 6, 4, 4, 4, 0, 1, 0, 0, 2, 2, 5, 2, 3, 3, 4, 7, 1, 0, 0, 0, 0, 14, 0, 0, 2, 0,
        0, 0, 6,
    ];
    let p = generate::mutate(&generate::program(&c), &[2459191009, 1627429535]);
    let source = generate::source(&p);
    assert!(source.contains("let Oops z18 =\n"), "{source}");
    let core = core(&source);
    assert!(model(&p).is_err(), "{source}");
    assert!(!core.is_empty(), "{source}");
}
