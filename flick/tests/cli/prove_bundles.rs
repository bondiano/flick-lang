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

/// What `flick prove` says of a project of `files`, and its exit code.
fn prove_files(name: &str, files: &[(&str, &str)]) -> (Option<i32>, String) {
    let mut all = vec![("flick.toml", LIB)];
    all.extend_from_slice(files);
    let dir = project(name, &all);
    let proved = flick_in(&dir, &["prove"]);
    (
        proved.status.code(),
        String::from_utf8_lossy(&proved.stderr).into_owned(),
    )
}

/// `pick` answers its second argument, so `first` is false; a lemma of the
/// law's sides at `One`, where there is one value, is no lemma of the law at
/// every type: its equation's type is not the law's.
#[test]
fn a_lemma_at_a_smaller_type_proves_no_law() {
    let (code, stderr) = prove_files(
        "bundle_at_a_smaller_type",
        &[
            (
                "src/App/Cls.fl",
                r"module App.Cls exposing (..)

class Pick (f : Type) =
  pick : forall b. f -> b -> b -> b
  law first (s : f) (w : b)
  where equal (fun (x : b) (y : b) -> pick s x y) (fun (x : b) (y : b) -> x)

type Tag = Tag

/// Picks the second: the law `first` is false of it at any type with two values.
instance Pick Tag =
  let pick _ _ y = y

type One = One
",
            ),
            (
                "proofs/App/Laws.fl",
                r"module App.Laws exposing (..)

import App.Cls exposing (..)

/// The law's terms, but at `One`, where every two values are equal.
proof first_at_one (s : Tag) (w : c)
where equal (fun (x : One) (y : One) -> pick s x y) (fun (x : One) (y : One) -> x) =
  funext (fun x -> funext (fun y -> match x with
    | One ->
      match y with
      | One -> refl))

/// At `Bool` the law fails: `pick` returns the second.
proof second_at_bool (s : Tag) where equal (pick s true false) false = refl

proof tag where lawful Pick Tag = { first = first_at_one }
",
            ),
        ],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("`first_at_one` is not stated over the parameters of the law `first`"),
        "{stderr}"
    );
}

/// A project module named as a built-in one is where its classes are: its
/// laws are read, not the built-in module's, which has none of them.
#[test]
fn a_module_named_as_a_built_in_keeps_its_laws() {
    let (code, stderr) = prove_files(
        "bundle_of_a_shadowing_module",
        &[
            (
                "src/Option.fl",
                r"module Option exposing (..)

class Same a =
  same : a -> a
  law fixed (x : a) where equal (same x) x

instance Same Bool =
  let same x =
    match x with
    | true -> false
    | false -> true
",
            ),
            (
                "proofs/App/Laws.fl",
                r"module App.Laws exposing (..)

import Option exposing (..)

/// `same` is negation: the law `fixed` is false of `Same Bool`.
proof flips where equal (same true) false = refl

proof bool where lawful Same Bool = {}
",
            ),
        ],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("no lemma for the law `fixed` of `Same`"),
        "{stderr}"
    );
}

/// `A.X` twice and `B.X0` name their dictionary fields `X0`, `X1`, `X0`: a
/// field names no one superclass, so the bundle is refused rather than
/// checked against whichever `X0` comes first.
#[test]
fn superclasses_sharing_a_field_get_no_bundle() {
    let (code, stderr) = prove_files(
        "bundle_with_shared_fields",
        &[
            (
                "src/App/A.fl",
                r"module App.A exposing (..)

class X a =
  op : a -> a
",
            ),
            (
                "src/App/B.fl",
                r"module App.B exposing (..)

class X0 a =
  op : a -> a
  law keep (x : a) where equal (op x) x
",
            ),
            (
                "src/App/C.fl",
                r"module App.C exposing (..)

import App.A as A
import App.B as B

class (A.X a, A.X b, B.X0 a) => C a b =
  tag : a -> b -> Bool

type T = P | Q

type U = U

instance A.X T =
  let op x = x

instance A.X U =
  let op x = x

/// Swaps: `keep` is false of it.
instance B.X0 T =
  let op x =
    match x with
    | P -> Q
    | Q -> P

instance C T U =
  let tag _ _ = true
",
            ),
            (
                "proofs/App/Laws.fl",
                r"module App.Laws exposing (..)

import App.A as A
import App.B as B
import App.C exposing (..)

/// `keep` of the `A.X T` instance, which is the identity.
proof keep_x (x : T) where equal (A.op x) x =
  match x with
  | P -> refl
  | Q -> refl

/// `B.X0 T` swaps: `keep` is false of it.
proof swaps where equal (B.op P) Q = refl

proof ct where lawful C T U = { X0 = { keep = keep_x } }
",
            ),
        ],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("FLICK-T082") && stderr.contains("share the dictionary field `X0`"),
        "{stderr}"
    );
    assert!(!stderr.contains("no law of the classes"), "{stderr}");
}

const PAIR: &str = r"module App.Types exposing (..)

type Pair e a = Pair e a

instance Functor (Pair e) =
  let map f value =
    match value with
    | Pair e x -> Pair e (f x)
";

/// The laws of `Functor (Pair a)` are about every `Pair a x`: lemmas of
/// `Pair a a` only are none of them, and lemmas of every pair are.
#[test]
fn a_bundle_over_a_type_variable_needs_lemmas_as_general_as_its_laws() {
    let diagonal = r"module App.Laws exposing (..)

import App.Types exposing (..)

proof pid (v : Pair a a)
where equal (map identity v) v =
  match v with
  | Pair e x -> refl

proof pcomp (g : b -> c) (h : a -> b) (v : Pair a a)
where equal (map (fun x -> g (h x)) v) (map g (map h v)) =
  match v with
  | Pair e x -> refl

proof pair where lawful Functor (Pair a) = {
  identity = pid
  composition = pcomp
}
";
    let (code, stderr) = prove_files(
        "bundle_over_a_type_variable",
        &[("src/App/Types.fl", PAIR), ("proofs/App/Laws.fl", diagonal)],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("`pid` is not stated over the parameters of the law `identity`"),
        "{stderr}"
    );
    let general = diagonal.replace("Pair a a", "Pair e a");
    let (code, stderr) = prove_files(
        "bundle_over_a_type_variable_general",
        &[("src/App/Types.fl", PAIR), ("proofs/App/Laws.fl", &general)],
    );
    assert_eq!(code, Some(0), "{stderr}");
}

/// A field names a lemma of another proof module as `M.name`, which the
/// bundle's module replays before it.
#[test]
fn a_bundle_names_the_lemmas_of_another_proof_module() {
    let (code, stderr) = prove_files(
        "bundle_of_imported_lemmas",
        &[
            (
                "src/App/Types.fl",
                r"module App.Types exposing (..)

type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)
",
            ),
            (
                "proofs/App/Lemmas.fl",
                r"module App.Lemmas exposing (..)

import App.Types exposing (..)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box_composition (g : b -> c) (h : a -> b) (v : Box a)
where equal (map (fun x -> g (h x)) v) (map g (map h v)) =
  match v with
  | Box x -> refl
",
            ),
            (
                "proofs/App/Laws.fl",
                r"module App.Laws exposing (..)

import App.Types exposing (..)
import App.Lemmas as L

proof box where lawful Functor Box = {
  identity = L.box_identity
  composition = L.box_composition
}
",
            ),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(!stderr.contains("FLICK-"), "{stderr}");
}
