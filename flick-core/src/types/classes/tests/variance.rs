use super::{Dict, check_all};
use crate::diagnostics::codes;

const VARIANCE_DEFS: (&str, &str) = (
    "App.Defs",
    "\
module App.Defs exposing (Box(..), Sink(..), Producer(..), Consumer(..), Transform(..), Combine(..), Marker(..))

type Box a = Box { value : a }
type Sink a = Sink (a -> Unit)

class Producer a =
  produce : a

class Consumer a =
  consume : a -> String

class Transform a b =
  transform : a -> b

class Combine a =
  combine : a -> a -> a

class Marker a =
  marker : String

instance Producer (Box (Unit -> Int)) =
  let produce = Box { value = fun () -> 7 }

instance Consumer (Unit ->{Async} Unit) =
  let consume _ = \"async\"

instance Consumer (Sink (Unit -> Unit)) =
  let consume _ = \"sink\"

instance Transform (Unit ->{Async} Unit) (Unit -> Unit) =
  let transform _ = fun () -> ()

instance Combine (Unit -> Unit) =
  let combine f _ = f

instance Combine (Unit ->{Async} Unit) =
  let combine f _ = f

instance Marker (Unit -> Unit) =
  let marker = \"pure\"
",
);

#[test]
fn b1_resolution_selects_expected_dictionaries_for_covariant_contravariant_and_imported_classes() {
    let main = (
        "App.Main",
        "\
module App.Main exposing (..)
import App.Defs exposing (..)

use_producer : Unit ->{} Box ((Unit -> Int)^{s})
let use_producer () =
  scope {
    let b : Box ((Unit -> Int)^{s}) = produce
    b
  }
",
    );
    // Sanity check: escaping `Box ((Unit -> Int)^{s})` from `scope` is caught by G002/G001.
    let (escape_diags, _) = check_all(&[VARIANCE_DEFS, main]);
    assert!(
        escape_diags
            .iter()
            .any(|d| d.code == codes::G002 || d.code == codes::G001),
        "scope escape through covariant dictionary must fail: {escape_diags:?}"
    );

    let ok_main = (
        "App.Main",
        "\
module App.Main exposing (..)
import App.Defs exposing (..)

use_producer : Unit ->{} Int
let use_producer () =
  scope s {
    let b : (Box ((Unit -> Int)^{s}))^{s} = produce
    match b with
    | Box r -> r.value ()
  }

use_producer_async : Box (Unit ->{Async} Int)
let use_producer_async = produce

use_consumer : String
let use_consumer = consume (fun () -> ())

use_consumer_sink : (Sink (Unit ->{Async} Unit))^{} -> String
let use_consumer_sink s = consume s

use_transform : (Unit -> Unit) -> (Unit ->{Async} Unit)
let use_transform f = transform f

use_combine_pure : (Unit -> Unit) -> (Unit -> Unit)
let use_combine_pure f = combine f f

use_combine_async : (Unit ->{Async} Unit) -> (Unit ->{Async} Unit)
let use_combine_async f = combine f f

check_marker : Marker a => a -> Int
let check_marker _ = 1

use_marker : Int
let use_marker = check_marker (fun () -> ())
",
    );
    let (diags, dicts) = check_all(&[VARIANCE_DEFS, ok_main]);
    assert!(diags.is_empty(), "{diags:?}");
    let chosen: Vec<&str> = dicts
        .iter()
        .filter_map(|d| match d {
            Dict::Instance { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    for expected in [
        "Producer (Box (Unit -> Int))",
        "Consumer (Unit ->{Async} Unit)",
        "Consumer (Sink (Unit -> Unit))",
        "Transform (Unit ->{Async} Unit) (Unit -> Unit)",
        "Combine (Unit -> Unit)",
        "Combine (Unit ->{Async} Unit)",
    ] {
        assert!(
            chosen.contains(&expected),
            "expected dictionary `{expected}` in {chosen:?}"
        );
    }
}

#[test]
fn k1_prime_overlap_rejects_comparable_and_product_heads_in_both_orders_locally_and_across_imports()
{
    let cases = [
        (
            "contravariant comparable",
            "class Describe a =\n  describe : a -> String\n",
            "instance Describe (Unit ->{Async} Unit) =\n  let describe _ = \"async\"\n",
            "instance Describe (Unit -> Unit) =\n  let describe _ = \"pure\"\n",
        ),
        (
            "covariant comparable",
            "type Box a = Box { value : a }\nclass Default a =\n  default : a\n",
            "instance Default (Box (Unit -> Unit)) =\n  let default = Box { value = fun () -> () }\n",
            "instance Default (Box (Unit ->{Async} Unit)) =\n  let default = Box { value = fun () -> () }\n",
        ),
        (
            "incomparable product heads with common lower bound",
            "class PairDesc a =\n  pair_desc : a -> String\n",
            "instance PairDesc (Unit ->{Async} Unit, Unit -> Unit) =\n  let pair_desc _ = \"rp\"\n",
            "instance PairDesc (Unit -> Unit, Unit ->{Async} Unit) =\n  let pair_desc _ = \"pr\"\n",
        ),
        (
            "incomparable product heads with common upper bound",
            "class PairDef a =\n  pair_def : a\n",
            "instance PairDef (Unit ->{Async} Unit, Unit -> Unit) =\n  let pair_def = (fun () -> (), fun () -> ())\n",
            "instance PairDef (Unit -> Unit, Unit ->{Async} Unit) =\n  let pair_def = (fun () -> (), fun () -> ())\n",
        ),
        (
            "mixed multi-parameter overlap",
            "class Convert a b =\n  convert : a -> b\n",
            "instance Convert (Unit ->{Async} Unit) (Unit -> Unit) =\n  let convert _ = fun () -> ()\n",
            "instance Convert (Unit -> Unit) (Unit ->{Async} Unit) =\n  let convert _ = fun () -> ()\n",
        ),
    ];

    for (label, decls, inst_a, inst_b) in cases {
        for (order, first, second) in [("AB", inst_a, inst_b), ("BA", inst_b, inst_a)] {
            let src = format!("module App.Local exposing (..)\n{decls}\n{first}\n{second}\n");
            let (diags, _) = check_all(&[("App.Local", &src)]);
            assert!(
                diags.iter().any(|d| d.code == codes::C003),
                "{label} ({order}) must reject overlap with FLICK-C003: {diags:?}"
            );
        }

        // Cross-module sibling overlap (A and B do not see each other; Main imports both).
        let base = (
            "App.Base",
            format!("module App.Base exposing (..)\n{decls}\n"),
        );
        let mod_a = (
            "App.A",
            format!("module App.A exposing (..)\nimport App.Base exposing (..)\n{inst_a}\n"),
        );
        let mod_b = (
            "App.B",
            format!("module App.B exposing (..)\nimport App.Base exposing (..)\n{inst_b}\n"),
        );
        for (order, imports) in [
            ("AB", "import App.A\nimport App.B\n"),
            ("BA", "import App.B\nimport App.A\n"),
        ] {
            let main = format!("module App.Main exposing (..)\n{imports}let ok = 1\n");
            let (diags, _) = check_all(&[
                (base.0, &base.1),
                (mod_a.0, &mod_a.1),
                (mod_b.0, &mod_b.1),
                ("App.Main", &main),
            ]);
            assert!(
                diags.iter().any(|d| d.code == codes::C003),
                "imported sibling {label} ({order}) must report FLICK-C003: {diags:?}"
            );
        }
    }
}

#[test]
fn unknown_meta_refinement_waits_and_selects_unique_dictionary_without_switching() {
    let src = "\
module App.Meta exposing (..)

class Describe a =
  describe : a -> String

instance Describe Int =
  let describe _ = \"int\"

instance Describe (Unit ->{Async} Unit) =
  let describe _ = \"fn\"

use_after_infer : Unit -> String
let use_after_infer () =
  let run_with = fun x ->
    let s = describe x
    let _ = x ()
    s
  run_with (fun () -> ())

use_int_after_infer : Unit -> String
let use_int_after_infer () =
  let run_with = fun x ->
    let s = describe x
    let _ = x + 1
    s
  run_with 42
";
    let (diags, dicts) = check_all(&[("App.Meta", src)]);
    assert!(diags.is_empty(), "{diags:?}");
    let chosen: Vec<&str> = dicts
        .iter()
        .filter_map(|d| match d {
            Dict::Instance { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        chosen.contains(&"Describe (Unit ->{Async} Unit)"),
        "{chosen:?}"
    );
    assert!(chosen.contains(&"Describe Int"), "{chosen:?}");
}

#[test]
fn functional_dependency_improvement_stays_equality_based() {
    let src = "\
module App.FdVar exposing (..)

class MapOut a b | a -> b =
  map_out : a -> b

instance MapOut Int String =
  let map_out _ = \"ok\"

instance MapOut (Unit ->{Async} Unit) Int =
  let map_out _ = 1

// Exact match on determining `Int` improves `b = String`.
exact_improved : String
let exact_improved = map_out 1

// Known `b = Int` allows variance-directed resolution for `Unit -> Unit <= Unit ->{Async} Unit`.
variance_known_target : Int
let variance_known_target = map_out (fun () -> ())
";
    let (diags, dicts) = check_all(&[("App.FdVar", src)]);
    assert!(diags.is_empty(), "{diags:?}");
    assert!(
        dicts
            .iter()
            .any(|d| matches!(d, Dict::Instance { name, .. } if name == "MapOut Int String")),
        "{dicts:?}"
    );
    assert!(
        dicts.iter().any(
            |d| matches!(d, Dict::Instance { name, .. } if name == "MapOut (Unit ->{Async} Unit) Int")
        ),
        "{dicts:?}"
    );

    // Subtyped determining argument does NOT trigger FD improvement to unify `b`
    // when `b` is completely unknown (equality-based improvement only).
    let no_sub_fd = "\
module App.FdNoSub exposing (..)

class MapOut a b | a -> b =
  map_out : a -> b

instance MapOut (Unit ->{Async} Unit) Int =
  let map_out _ = 1

// Without knowing `b`, `Unit -> Unit` does NOT equality-match `Unit ->{Async} Unit`
// for FD improvement; `b` remains unknown (C002) rather than silently improving.
ambiguous : Unit -> String
let ambiguous () =
  let x = map_out (fun () -> ())
  show x
";
    let (diags, _) = check_all(&[("App.FdNoSub", no_sub_fd)]);
    assert!(
        diags.iter().any(|d| d.code == codes::C002),
        "FD improvement must stay equality-based and not fire on strict subtype: {diags:?}"
    );
}

#[test]
fn safe_use_rejects_reach_dropping_contravariant_instances_and_givens() {
    let src = "\
module App.SafeUse exposing (..)

type File (s : Region)

service Files =
  open_f : forall s. Unit ->{Scope s} File s
  read_f : forall s. File s ->{} Int

class Describe a =
  describe : a -> String

instance Describe (Unit ->{Async} Unit) =
  let describe _ = \"async\"

// A scoped closure holding `s` cannot be consumed by a capture-free `-` instance.
drop_capture_inst : Unit ->{Files} String
let drop_capture_inst () =
  scope s {
    let f = Files.open_f ()
    thunk : (Unit -> Unit)^{s}
    let thunk () =
      let _ = f
      ()
    describe thunk
  }

// A capture-free `-` given cannot serve a captured query.
drop_capture_given : forall s. Describe (Unit -> Unit) => (Unit -> Unit)^{s} -> String
let drop_capture_given thunk = describe thunk
";
    let (diags, _) = check_all(&[("App.SafeUse", src)]);
    let c001_count = diags.iter().filter(|d| d.code == codes::C001).count();
    assert!(
        c001_count >= 2,
        "both reach-dropping `-` instance and given must fail with C001: {diags:?}"
    );
}
