//! `proof name where lawful C T = { … }` certifies the laws of `C` and its
//! superclasses at the dictionary `C T` resolves to, each by the lemma its
//! record names. A lemma must be about exactly that dictionary's methods.

use std::fs;

use crate::common;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

/// `Box` maps as a functor does; `Bad` drops every value it holds.
const TYPES: &str = "module App.Types exposing (..)

type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)

type Bad a = Empty | Full a

instance Functor Bad =
  let map _ _ = Empty

/// What `Bad`'s `map` should have been.
good_map : (a -> b) -> Bad a -> Bad b
let good_map f value =
  match value with
  | Empty -> Empty
  | Full x -> Full (f x)

/// A dictionary named apart from the instance.
named : Functor.Dict Bad
let named = Functor.dictionary { map = fun _ _ -> Empty }
";

const LAWS: &str = "module App.Laws exposing (..)

import App.Types exposing (..)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box_composition (g : b -> c) (h : a -> b) (v : Box a)
where equal (map (fun x -> g (h x)) v) (map g (map h v)) =
  match v with
  | Box x -> refl

proof box where lawful Functor Box = {
  identity = box_identity
  composition = box_composition
}

proof good_identity (v : Bad a)
where equal (good_map identity v) v =
  match v with
  | Empty -> refl
  | Full x -> refl

proof good_composition (g : b -> c) (h : a -> b) (v : Bad a)
where equal (good_map (fun x -> g (h x)) v) (good_map g (good_map h v)) =
  match v with
  | Empty -> refl
  | Full x -> refl
";

/// What `flick prove` says of `LAWS` and then `bundle`, and its exit code.
fn prove(name: &str, bundle: &str) -> (Option<i32>, String) {
    let laws = format!("{LAWS}\n{bundle}");
    let dir = project(
        name,
        &[
            ("flick.toml", LIB),
            ("src/App/Types.fl", TYPES),
            ("proofs/App/Laws.fl", &laws),
        ],
    );
    let proved = flick_in(&dir, &["prove"]);
    (
        proved.status.code(),
        String::from_utf8_lossy(&proved.stderr).into_owned(),
    )
}

#[test]
fn a_bundle_of_lemmas_about_the_instance_certifies_it() {
    let (code, stderr) = prove("bundle_of_box", "");
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stderr.contains("FLICK-"), "{stderr}");
}

/// The lemmas hold of `good_map`, which is not the method of `Functor Bad`:
/// they prove nothing of the instance.
#[test]
fn lemmas_about_other_definitions_certify_no_instance() {
    let bundle = "proof bad where lawful Functor Bad = {\n  identity = good_identity\n  composition = good_composition\n}\n";
    let (code, stderr) = prove("bundle_of_bad", bundle);
    assert_eq!(code, Some(1), "{stderr}");
    // each lemma's sides are not the law's at `Functor Bad`: unproved
    assert_eq!(stderr.matches("FLICK-T081").count(), 2, "{stderr}");
    assert!(stderr.contains("`good_identity` proves `App.Types.good_map Prelude.identity v = v` of exactly the definitions it names"), "{stderr}");
}

/// The lemmas of `Box` are not even stated over `Bad`.
#[test]
fn lemmas_of_another_type_certify_no_instance() {
    let bundle = "proof bad where lawful Functor Bad = {\n  identity = box_identity\n  composition = box_composition\n}\n";
    let (code, stderr) = prove("bundle_of_bad_by_box", bundle);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("is not stated over the parameters of the law `identity`"),
        "{stderr}"
    );
}

#[test]
fn a_named_dictionary_is_outside_the_domain() {
    let bundle = "proof named_bad where lawful Functor Bad from named = {\n  identity = good_identity\n  composition = good_composition\n}\n";
    let (code, stderr) = prove("bundle_from_named", bundle);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("FLICK-T082") && stderr.contains("a named dictionary"),
        "{stderr}"
    );
}

/// A field for every law, and none other, whatever its lemma.
#[test]
fn a_bundle_names_a_lemma_for_each_law_and_nothing_else() {
    let bundle = "proof box_again where lawful Functor Box = {\n  identity = box_identity\n  fusion = box_composition\n}\n";
    let (code, stderr) = prove("bundle_of_box_misnamed", bundle);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("no lemma for the law `composition` of `Functor`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`fusion` is no law of the classes of the bundle"),
        "{stderr}"
    );
}

const CLASS: &str = "module App.Classes exposing (..)

class Wrap (f : Type -> Type) =
  wrap : a -> f a
  unwrap : f a -> a
  law round (x : a) where equal (unwrap (wrap x : f a)) x

type Cell a = Cell a

instance Wrap Cell =
  let wrap x = Cell x
  let unwrap v =
    match v with
    | Cell x -> x
";

const ROUND: &str = "module App.Round exposing (..)

import App.Classes exposing (..)

proof cell_round (x : a) where equal (unwrap (wrap x : Cell a)) x = refl

proof cell where lawful Wrap Cell = { round = cell_round }
";

/// The laws of a class are read again by every run with a bundle, not
/// taken from what an earlier run kept: an edit of a law the lemma no
/// longer states fails the bundle.
#[test]
fn a_bundle_follows_an_edit_of_its_class_laws() {
    let dir = project(
        "bundle_after_law_edit",
        &[
            ("flick.toml", LIB),
            ("src/App/Classes.fl", CLASS),
            ("proofs/App/Round.fl", ROUND),
        ],
    );
    // an ordinary check first keeps the class's module, laws unread
    let checked = flick_in(&dir, &["check"]);
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    // the second run finds the class's module unchanged since the first
    for _ in 0..2 {
        let proved = flick_in(&dir, &["prove"]);
        let stderr = String::from_utf8_lossy(&proved.stderr);
        assert_eq!(proved.status.code(), Some(0), "{stderr}");
    }
    let edited = CLASS.replace(
        "equal (unwrap (wrap x : f a)) x",
        "equal (wrap (unwrap (wrap x : f a)) : f a) (wrap x)",
    );
    fs::write(dir.join("src/App/Classes.fl"), edited).expect("edit the law");
    let refuted = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&refuted.stderr);
    assert_eq!(refuted.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("`cell_round` is not stated over")
            || stderr.contains("of exactly the definitions it names"),
        "{stderr}"
    );
    // and an ordinary check of the edit reads no proof
    let checked = flick_in(&dir, &["check"]);
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
}
