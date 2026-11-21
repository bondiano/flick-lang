use super::*;
use serde_json::{Value, json};

#[test]
fn inspect_variance_updates_after_edit_and_covers_builtins_and_conservative_limits() {
    let initial = "\
module Lib.Var exposing (Box, AWrapSink, Sink, Cell, Phantom, WrapPhantom, DoubleSink, Secret, Thunk, BoxErr(..), SinkErr(..), Alias, Abstract, Pair)

opaque type Box a = Box { value : a }
opaque type AWrapSink a = AWrapSink (Sink a)
opaque type Sink a = Sink (a -> Unit)
opaque type Cell a = Cell (Ref a)
opaque type Phantom a = Phantom
opaque type WrapPhantom a = WrapPhantom (Phantom a)
opaque type DoubleSink a = DoubleSink (Sink (Sink a))
type Secret a = Secret a
opaque type Thunk (r : ServiceRow) a = Thunk (Unit ->{r} a)
type error BoxErr a = BoxErr a
type error SinkErr a = SinkErr (a -> Unit)
opaque type alias Alias a = List a
type Abstract a
type alias Pair a b = (a, b)
";
    let dir = project(
        "cli_inspect_variance",
        &[("flick.toml", LIB), ("src/Lib/Var.fl", initial)],
    );

    let out = flick_in(&dir, &["inspect", "Lib.Var", "--emit", "variance"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let shown = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "module Lib.Var\n",
        "opaque type Box : a (+, covariant)\n",
        "opaque type AWrapSink : a (-, contravariant)\n",
        "opaque type Sink : a (-, contravariant)\n",
        "opaque type Cell : a (inv, invariant)\n",
        "opaque type Phantom : a (bi, bivariant)\n",
        "opaque type WrapPhantom : a (+, covariant)\n",
        "opaque type DoubleSink : a (inv, invariant)\n",
        "type Secret : a (+, covariant)\n",
        "opaque type Thunk : r (inv, invariant), a (+, covariant)\n",
        "type error BoxErr : a (+, covariant)\n",
        "type error SinkErr : a (-, contravariant)\n",
        "opaque type Alias : a (inv, unknown)\n",
        "type Abstract : a (inv, unknown)\n",
    ] {
        assert!(shown.contains(expected), "`{expected}` in\n{shown}");
    }
    assert!(
        !shown.contains("Pair"),
        "transparent aliases expand and have no nominal variance entry:\n{shown}"
    );

    let iface_out = flick_in(&dir, &["inspect", "Lib.Var", "--emit", "iface"]);
    assert_eq!(code(&iface_out), Some(0), "{iface_out:?}");
    let iface_json: Value = serde_json::from_slice(&iface_out.stdout).expect("valid iface json");
    let box_variance = iface_json["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .find(|e| e["name"] == "Box")
        .expect("Box")["definition"]["variance"]
        .clone();
    assert_eq!(box_variance, json!(["covariant"]));

    fs::write(
        dir.join("src/Lib/Var.fl"),
        initial.replace(
            "opaque type Box a = Box { value : a }",
            "opaque type Box a = Box (Ref a)",
        ),
    )
    .expect("edit");
    let after = flick_in(&dir, &["inspect", "Lib.Var", "--emit", "variance"]);
    assert_eq!(code(&after), Some(0), "{after:?}");
    let after_text = String::from_utf8_lossy(&after.stdout);
    assert!(
        after_text.contains("opaque type Box : a (inv, invariant)\n"),
        "{after_text}"
    );

    let after_iface = flick_in(&dir, &["inspect", "Lib.Var", "--emit", "iface"]);
    assert_eq!(code(&after_iface), Some(0), "{after_iface:?}");
    let after_json: Value = serde_json::from_slice(&after_iface.stdout).expect("valid iface json");
    let box_variance_after = after_json["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .find(|e| e["name"] == "Box")
        .expect("Box")["definition"]["variance"]
        .clone();
    assert_eq!(box_variance_after, json!(["invariant"]));

    let prelude = flick_in(&dir, &["inspect", "Prelude", "--emit", "variance"]);
    assert_eq!(code(&prelude), Some(0), "{prelude:?}");
    let prelude_text = String::from_utf8_lossy(&prelude.stdout);
    assert!(
        prelude_text.contains("type List : a (+, covariant)\n")
            && prelude_text.contains("type Option : a (+, covariant)\n")
            && prelude_text.contains("class Show : a (-, contravariant)\n")
            && prelude_text.contains("class Eq : a (-, contravariant)\n")
            && prelude_text.contains("class Ord : a (-, contravariant)\n")
            && prelude_text.contains("class Into : a (-, contravariant), b (+, covariant)\n")
            && prelude_text.contains("class Semigroup : a (inv, invariant)\n")
            && prelude_text.contains("class Functor : f (inv, invariant)\n"),
        "{prelude_text}"
    );
}

#[test]
fn inspect_and_doc_show_class_variance_and_update_after_edit() {
    let initial = "\
module Lib.ClsVar exposing (Default(..), Describe(..), Convert(..), Combine(..), Marker(..), MyFunctor(..))

class Default a =
  default : a

class Describe a =
  describe : a -> String

class Convert a b =
  convert : a -> b

class Combine a =
  combine : a -> a -> a

class Marker a =
  marker : String

class MyFunctor (f : Type -> Type) =
  my_map : (a -> b) -> f a -> f b
";
    let dir = project(
        "cli_inspect_class_variance",
        &[("flick.toml", LIB), ("src/Lib/ClsVar.fl", initial)],
    );

    let out = flick_in(&dir, &["inspect", "Lib.ClsVar", "--emit", "variance"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let shown = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "module Lib.ClsVar\n",
        "class Default : a (+, covariant)\n",
        "class Describe : a (-, contravariant)\n",
        "class Convert : a (-, contravariant), b (+, covariant)\n",
        "class Combine : a (inv, invariant)\n",
        "class Marker : a (bi, bivariant)\n",
        "class MyFunctor : f (inv, invariant)\n",
    ] {
        assert!(shown.contains(expected), "`{expected}` in\n{shown}");
    }

    let iface_out = flick_in(&dir, &["inspect", "Lib.ClsVar", "--emit", "iface"]);
    assert_eq!(code(&iface_out), Some(0), "{iface_out:?}");
    let iface_json: Value = serde_json::from_slice(&iface_out.stdout).expect("valid iface json");
    let describe_var = iface_json["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .find(|e| e["name"] == "Describe")
        .expect("Describe")["definition"]["variance"]
        .clone();
    assert_eq!(describe_var, json!(["contravariant"]));

    let doc_out = flick_in(&dir, &["doc", "--out-dir", "docs"]);
    assert_eq!(code(&doc_out), Some(0), "{doc_out:?}");
    let doc_page = fs::read_to_string(dir.join("docs/Lib.ClsVar.md")).expect("doc page");
    for expected in [
        "Variance: `a` (`+`, covariant)",
        "Variance: `a` (`-`, contravariant)",
        "Variance: `a` (`-`, contravariant), `b` (`+`, covariant)",
        "Variance: `a` (`inv`, invariant)",
        "Variance: `a` (`bi`, bivariant)",
        "Variance: `f` (`inv`, invariant)",
    ] {
        assert!(doc_page.contains(expected), "`{expected}` in\n{doc_page}");
    }

    fs::write(
        dir.join("src/Lib/ClsVar.fl"),
        initial.replace("describe : a -> String", "describe : a -> a"),
    )
    .expect("edit class");
    let after = flick_in(&dir, &["inspect", "Lib.ClsVar", "--emit", "variance"]);
    assert_eq!(code(&after), Some(0), "{after:?}");
    let after_text = String::from_utf8_lossy(&after.stdout);
    assert!(
        after_text.contains("class Describe : a (inv, invariant)\n"),
        "{after_text}"
    );
}
