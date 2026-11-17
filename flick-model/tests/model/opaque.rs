//! An opaque alias inside its module is its representation
//! (`docs/formal/elaboration.md#an-opaque-alias-inside-its-module-resolves-through-its-representation`):
//! the model checks what flick-core accepts there.

use flick_model::run;

/// The model's errors on the one module `source`, which flick-core accepts.
fn model_errors(source: &str) -> Vec<String> {
    let modules = run::compile_source(source).expect("flick-core accepts it");
    run::check(&modules, &run::program(&modules))
}

/// An opaque alias may name itself: inside its module two of it are equal
/// by their arguments, as for flick-core.
#[test]
fn a_recursive_opaque_alias_is_checked() {
    let errors = model_errors(
        "module Streams exposing (Stream, identity)\n\n\
         opaque type alias Stream = Option (Float, Stream)\n\n\
         identity : Stream -> Stream\n\
         let identity s = s\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

/// One that names itself through an arrow: subsuming it unfolds it once on
/// both sides, and the occurrence inside is compared by its arguments.
#[test]
fn a_recursive_opaque_alias_of_a_function_is_checked() {
    let errors = model_errors(
        "module Loops exposing (Loop, spin)\n\n\
         opaque type alias Loop = Int -> Loop\n\n\
         spin : Loop -> Loop\n\
         let spin l = l\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

/// A parameter of an opaque alias is replaced by its kind: `Job {Boom}` is
/// `Eff none {Boom} Unit`.
#[test]
fn an_opaque_alias_takes_a_row() {
    let errors = model_errors(
        "module Jobs exposing (Job, Boom, job)\n\n\
         type error Boom = Boom Int\n\n\
         opaque type alias Job e = Eff none e Unit\n\n\
         job : Job {Boom}\n\
         let job = task { from_result (Err (Boom 1)) }\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}
