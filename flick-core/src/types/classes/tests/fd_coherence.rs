use super::check_all;
use crate::diagnostics::{Code, codes};

fn rejects(text: &str, code: Code) {
    let (diagnostics, _) = check_all(&[("App.Main", text)]);
    assert_eq!(
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![code],
        "{text}\n{diagnostics:?}"
    );
}

#[test]
fn context_coverage_handles_superclasses_determinants_and_aliases() {
    let (diagnostics, _) = check_all(&[(
        "App.FdCoverage",
        include_str!("../../../../../tests/check/pass/classes/fd_coverage.fl"),
    )]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_lifting_instance_and_a_specialization_overlap_in_either_order() {
    let header = r"module App.Main exposing (..)
type Identity a = Identity a
type Config = Config
type ReaderT r (m : Type -> Type) a = ReaderT
class State (m : Type -> Type) s | m -> s
";
    let lifting = "instance State m s => State (ReaderT r m) s\n";
    let concrete = "instance State (ReaderT Config Identity) Number\n";
    for instances in [
        format!("{lifting}{concrete}"),
        format!("{concrete}{lifting}"),
    ] {
        rejects(&format!("{header}{instances}"), codes::C003);
    }
}

#[test]
fn one_determining_monad_cannot_have_two_concrete_states() {
    let header = r"module App.Main exposing (..)
type Identity a = Identity a
class State (m : Type -> Type) s | m -> s
";
    let number = "instance State Identity Number\n";
    let string = "instance State Identity String\n";
    for instances in [format!("{number}{string}"), format!("{string}{number}")] {
        rejects(&format!("{header}{instances}"), codes::C004);
    }
}

const MARKED: &str = r"module App.Main exposing (..)
type Wrap a = Wrap a
type LeftTag = LeftTag
type RightTag = RightTag
class Base a s | a -> s =
  base : a -> s
class Marked a s tag | a -> s =
  marked : a -> tag -> s
instance Base Number Number =
  base : Number -> Number
  let base value = value
";

const LEFT: &str = r"instance Base a s => Marked (Wrap a) s LeftTag =
  marked : Wrap a -> LeftTag -> s
  let marked (Wrap value) _ = base value
";

const RIGHT: &str = r#"instance Marked (Wrap Number) String RightTag =
  marked : Wrap Number -> RightTag -> String
  let marked _ _ = "right"
"#;

#[test]
fn nonoverlapping_tags_do_not_hide_context_determined_disagreement() {
    for instances in [format!("{LEFT}{RIGHT}"), format!("{RIGHT}{LEFT}")] {
        rejects(&format!("{MARKED}{instances}"), codes::C004);
    }
}

#[test]
fn a_wrong_tag_with_unknown_output_is_missing_not_a_fresh_meta_loop() {
    let text = format!(
        "{MARKED}{LEFT}\n\
         missing : Unit\nlet missing = ignore (marked (Wrap 0) RightTag)\n"
    );
    rejects(&text, codes::C001);
}

#[test]
fn an_unknown_determining_monad_is_not_defaulted_to_an_instance() {
    let text = r"module App.Main exposing (..)
type Identity a = Identity a
class State (m : Type -> Type) s | m -> s =
  get : m s
instance State Identity Number =
  get : Identity Number
  let get = Identity 0
ambiguous : Unit
let ambiguous = ignore get
";
    rejects(text, codes::C002);
}

#[test]
fn growing_superclasses_stop_after_the_cycle_diagnostic_during_method_checking() {
    let text = r"module App.Main exposing (..)
class Loop (List a) b => Loop a b | a -> b =
  loop : a -> b
type Wrap a = Wrap a
class Project a b | a -> b =
  project : a -> b
instance Loop a b => Project (Wrap a) b =
  project : Wrap a -> b
  let project (Wrap value) = loop value
";
    rejects(text, codes::C008);
}
