use super::{Dict, check_all};

fn instance(module: &str, name: &str, args: Vec<Dict>) -> Dict {
    Dict::Instance {
        module: module.to_owned(),
        name: name.to_owned(),
        args,
    }
}

const TRANSFORMERS: (&str, &str) = (
    "App.Transformers",
    include_str!("../../../../../tests/check/pass/regions/transformers.fl"),
);

const STATE: (&str, &str) = (
    "App.State",
    r"module App.State exposing (MonadState(..))
import App.Transformers exposing (..)

class Monad m => MonadState (m : Type -> Type) s | m -> s =
  get : m s

instance Monad m => MonadState (StateT s m) s =
  get : StateT s m s
  let get = StateT (fun state -> pure (state, state))

instance MonadState m s => MonadState (MaybeT m) s =
  get : MaybeT m s
  let get = MaybeT (map Some get)

instance MonadState m s => MonadState (ReaderT r m) s =
  get : ReaderT r m s
  let get = ReaderT (fun _ -> get)
",
);

#[test]
fn discarded_state_is_inferred_through_native_lifted_and_given_dictionaries() {
    let body = r"
type Config = Config

native : StateT Float Identity Unit
let native = do { _ <- get; pure () }

lifted : ReaderT Config (MaybeT (StateT Float Identity)) Unit
let lifted = do { _ <- get; pure () }

type alias Stack a = ReaderT Config (MaybeT (StateT Float Identity)) a
aliased : Stack Unit
let aliased = do { _ <- get; pure () }

generic : MonadState m s => ReaderT Config (MaybeT m) Unit
let generic = do { _ <- get; pure () }
";
    let headers = [
        "import App.Transformers exposing (..)\nimport App.State exposing (..)\n",
        "import App.State exposing (..)\nimport App.Transformers exposing (..)\n",
    ];
    let native = instance(
        "App.State",
        "MonadState (StateT s m) s",
        vec![instance("App.Transformers", "Monad Identity", vec![])],
    );
    let lifted = |leaf| {
        instance(
            "App.State",
            "MonadState (ReaderT r m) s",
            vec![instance("App.State", "MonadState (MaybeT m) s", vec![leaf])],
        )
    };
    let mut previous = None;
    for header in headers {
        let main = format!("module App.Main exposing (..)\n{header}{body}");
        let (diagnostics, dicts) = check_all(&[TRANSFORMERS, STATE, ("App.Main", &main)]);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        for expected in [
            native.clone(),
            lifted(native.clone()),
            lifted(Dict::Given("MonadState m s".to_owned())),
        ] {
            assert!(dicts.contains(&expected), "missing {expected:?}: {dicts:?}");
        }
        assert_eq!(
            dicts
                .iter()
                .filter(|dict| **dict == lifted(native.clone()))
                .count(),
            2,
            "{dicts:?}"
        );
        if let Some(previous) = previous {
            assert_eq!(dicts, previous);
        }
        previous = Some(dicts);
    }
}

const RELATIONS: (&str, &str) = (
    "App.Relations",
    r#"module App.Relations exposing (..)
class AB a b | a -> b =
  ab : a -> b
class BC b c | b -> c =
  bc : b -> c
class Ready c
class Outer a b c | a -> b c =
  outer : a -> (b, c)
class Waiting a b c | a -> b =
  waiting : a -> b
instance AB Float String =
  ab : Float -> String
  let ab _ = "number"
instance BC String Bool =
  bc : String -> Bool
  let bc _ = true
instance Ready Bool
"#,
);

#[test]
fn context_refinement_retries_earlier_constraints_without_reordering_dictionaries() {
    let bc = instance("App.Relations", "BC String Bool", vec![]);
    let ab = instance("App.Relations", "AB Float String", vec![]);
    for (context, args) in [
        ("BC b c, AB a b", vec![bc.clone(), ab.clone()]),
        ("AB a b, BC b c", vec![ab.clone(), bc.clone()]),
    ] {
        let outer = format!(
            r"module App.Outer exposing (Wrap(..))
import App.Relations exposing (..)
type Wrap a = Wrap a
instance ({context}) => Outer (Wrap a) b c =
  outer : Wrap a -> (b, c)
  let outer (Wrap value) =
    let middle = ab value
    (middle, bc middle)
"
        );
        for imports in [
            "import App.Relations exposing (..)\nimport App.Outer exposing (..)\n",
            "import App.Outer exposing (..)\nimport App.Relations exposing (..)\n",
        ] {
            let main = format!(
                "module App.Main exposing (observed)\n{imports}\n\
                 observed : Unit\nlet observed = ignore (outer (Wrap 0.0))\n"
            );
            let (diagnostics, dicts) =
                check_all(&[RELATIONS, ("App.Outer", &outer), ("App.Main", &main)]);
            assert!(diagnostics.is_empty(), "{context}: {diagnostics:?}");
            assert_eq!(
                dicts,
                vec![instance("App.Outer", "Outer (Wrap a) b c", args.clone())]
            );
        }
    }
}

#[test]
fn a_stuck_sibling_can_refine_types_before_either_wanted_is_solved() {
    let outer = (
        "App.Outer",
        r"module App.Outer exposing (Wrap(..), siblings)
import App.Relations exposing (..)
type Wrap a = Wrap a
instance (Ready c, AB a b) => Waiting (Wrap a) b c =
  waiting : Wrap a -> b
  let waiting (Wrap value) = ab value
siblings : (BC b c, Waiting (Wrap a) b c) => a -> Unit
let siblings value = ()
",
    );
    let main = r"module App.Main exposing (observed)
import App.Outer exposing (..)
import App.Relations exposing (..)
observed : Unit
let observed = siblings 0.0
";
    let (diagnostics, dicts) = check_all(&[RELATIONS, outer, ("App.Main", main)]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        dicts,
        vec![
            instance("App.Relations", "BC String Bool", vec![]),
            instance(
                "App.Outer",
                "Waiting (Wrap a) b c",
                vec![
                    instance("App.Relations", "Ready Bool", vec![]),
                    instance("App.Relations", "AB Float String", vec![]),
                ],
            ),
        ]
    );
}

#[test]
fn context_determined_structures_and_repeated_outputs_are_inferred() {
    let library = (
        "App.Structured",
        r"module App.Structured exposing (..)
type Wrap a = Wrap a
class Base a s | a -> s =
  base : a -> s
class Structured key output | key -> output =
  collect : key -> output
class Repeated key left right | key -> left right =
  repeat : key -> (left, right)
instance Base Float Float =
  base : Float -> Float
  let base value = value
instance Base a s => Structured (Wrap a) (List s) =
  collect : Wrap a -> List s
  let collect (Wrap value) = [base value]
instance Base a s => Repeated (Wrap a) s s =
  repeat : Wrap a -> (s, s)
  let repeat (Wrap value) = (base value, base value)
",
    );
    let main = r"module App.Main exposing (observed)
import App.Structured exposing (..)
observed : Unit
let observed =
  ignore (collect (Wrap 0.0))
  ignore (repeat (Wrap 0.0))
";
    let (diagnostics, dicts) = check_all(&[library, ("App.Main", main)]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let leaf = instance("App.Structured", "Base Float Float", vec![]);
    assert_eq!(
        dicts,
        vec![
            instance(
                "App.Structured",
                "Structured (Wrap a) (List s)",
                vec![leaf.clone()],
            ),
            instance("App.Structured", "Repeated (Wrap a) s s", vec![leaf]),
        ]
    );
}

#[test]
fn superclass_coverage_preserves_both_projection_steps() {
    let library = (
        "App.Supers",
        r"module App.Supers exposing (..)
type Wrap a = Wrap a
class Base a b | a -> b =
  base : a -> b
class Base a b => Middle a b
class Middle a b => Top a b
class Project a b | a -> b =
  project : a -> b
instance Top a b => Project (Wrap a) b =
  project : Wrap a -> b
  let project (Wrap value) = base value
",
    );
    let main = r"module App.Main exposing (from_super, from_instance)
import App.Supers exposing (..)
from_super : Top a b => a -> b
let from_super value = base value
from_instance : Top a b => Wrap a -> b
let from_instance value = project value
";
    let (diagnostics, dicts) = check_all(&[library, ("App.Main", main)]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let given = Dict::Given("Top a b".to_owned());
    assert_eq!(
        dicts,
        vec![
            Dict::Super {
                dict: Box::new(Dict::Super {
                    dict: Box::new(given.clone()),
                    class: "Middle".to_owned(),
                }),
                class: "Base".to_owned(),
            },
            instance("App.Supers", "Project (Wrap a) b", vec![given]),
        ]
    );
}
