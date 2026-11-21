use super::*;

const VARIANCE_SECRET: &str = "\
private module Geo.Secret exposing (Inner, Key(..))

type Inner a = Inner a
type Key = Key Int
";

const VARIANCE_LIB: &str = "\
module Geo exposing (Box, Sink, Cell, Abstract, Wrapper(..), Tag(..), BoxErr(..))

import Geo.Secret exposing (Inner, Key)

opaque type Box a = Box { value : a }
opaque type Sink a = Sink (a -> Unit)
opaque type Cell a = Cell (Ref a)
type Abstract a
type Wrapper a = Wrapper (Inner a)
type Tag a =
  | TagKey : Tag Key
  | TagVal a : Tag a
type error BoxErr a = BoxErr (Inner a)
";

const VARIANCE_APP: &str = "\
module App.Main exposing (main, widen_box, narrow_sink, widen_wrapper, widen_tag, widen_err)

import Geo exposing (Box, Sink, Wrapper(..), Tag(..), BoxErr(..))

widen_box : Box (Float -> Float) -> Box (Float ->{Async} Float)
let widen_box b = b

narrow_sink : Sink (Float ->{Async} Float) -> Sink (Float -> Float)
let narrow_sink s = s

widen_wrapper : Wrapper (Float -> Float) -> Wrapper (Float ->{Async} Float)
let widen_wrapper w = w

widen_tag : Tag (Float -> Float) -> Tag (Float ->{Async} Float)
let widen_tag t = t

widen_err : BoxErr (Float -> Float) -> BoxErr (Float ->{Async} Float)
let widen_err e = e

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line \"ok\"
";

#[test]
fn fetched_package_preserves_opaque_variance_offline_and_in_inspect() {
    let (root, geo) = fetched("package-cache-opaque-variance");
    let secret = geo.parent().expect("src dir").join("Geo/Secret.fl");
    fs::write(&secret, VARIANCE_SECRET).expect("write fetched secret");
    fs::write(&geo, VARIANCE_LIB).expect("write fetched lib");
    fs::write(root.join("app/src/App/Main.fl"), VARIANCE_APP).expect("write app");

    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(0), "cold fetched check: {stderr}");

    // Narrowing a covariant visible-constructor wrapper or GADT must fail (fresh).
    let bad_wrap = VARIANCE_APP.replace(
        "widen_wrapper : Wrapper (Float -> Float) -> Wrapper (Float ->{Async} Float)",
        "widen_wrapper : Wrapper (Float ->{Async} Float) -> Wrapper (Float -> Float)",
    );
    fs::write(root.join("app/src/App/Main.fl"), &bad_wrap).expect("write bad wrap");
    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(1), "fresh narrowing must fail: {stderr}");
    assert!(stderr.contains("FLICK-T003"), "{stderr}");
    fs::write(root.join("app/src/App/Main.fl"), VARIANCE_APP).expect("restore app");

    fs::remove_file(&geo).expect("remove fetched source");
    fs::remove_file(&secret).expect("remove fetched secret source");
    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(0), "offline cached check: {stderr}");

    let bad_tag = VARIANCE_APP.replace(
        "widen_tag : Tag (Float -> Float) -> Tag (Float ->{Async} Float)",
        "widen_tag : Tag (Float ->{Async} Float) -> Tag (Float -> Float)",
    );
    fs::write(root.join("app/src/App/Main.fl"), &bad_tag).expect("write bad tag");
    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(1), "offline narrowing must fail: {stderr}");
    assert!(stderr.contains("FLICK-T003"), "{stderr}");
    fs::write(root.join("app/src/App/Main.fl"), VARIANCE_APP).expect("restore app");

    let inspect_var = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["inspect", "Geo", "--emit", "variance"])
        .current_dir(root.join("app"))
        .env("FLICK_STORE", root.join("store"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("NO_COLOR", "1")
        .output()
        .expect("inspect variance");
    assert_eq!(inspect_var.status.code(), Some(0), "{inspect_var:?}");
    let var_out = String::from_utf8_lossy(&inspect_var.stdout);
    for expected in [
        "module Geo\n",
        "opaque type Box : a (+, covariant)\n",
        "opaque type Sink : a (-, contravariant)\n",
        "opaque type Cell : a (inv, invariant)\n",
        "type Abstract : a (inv, unknown)\n",
        "type Wrapper : a (+, covariant)\n",
        "type Tag : a (+, covariant)\n",
        "type error BoxErr : a (+, covariant)\n",
    ] {
        assert!(var_out.contains(expected), "`{expected}` in\n{var_out}");
    }

    let inspect_iface = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["inspect", "Geo", "--emit", "iface"])
        .current_dir(root.join("app"))
        .env("FLICK_STORE", root.join("store"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("NO_COLOR", "1")
        .output()
        .expect("inspect iface");
    assert_eq!(inspect_iface.status.code(), Some(0), "{inspect_iface:?}");
    let iface_json: Value =
        serde_json::from_slice(&inspect_iface.stdout).expect("valid iface json");
    assert_eq!(iface_json["iface-version"], json!(IFACE_VERSION));
    for name in ["Box", "Wrapper", "Tag", "BoxErr"] {
        let got = iface_json["exports"]
            .as_array()
            .expect("exports")
            .iter()
            .find(|e| e["name"] == name)
            .expect(name)["definition"]["variance"]
            .clone();
        assert_eq!(got, json!(["covariant"]), "{name}");
    }
}

#[test]
fn path_package_variance_edit_invalidates_caches_and_updates_inspect() {
    let path_root = project(
        "package-cache-variance-invalidate",
        &[
            (
                "app/flick.toml",
                &APP.replace("DEPENDENCY", "{ path = \"../lib\" }"),
            ),
            ("app/src/App/Main.fl", VARIANCE_APP),
            ("lib/flick.toml", LIB),
            ("lib/src/Geo/Secret.fl", VARIANCE_SECRET),
            ("lib/src/Geo.fl", VARIANCE_LIB),
        ],
    );
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(status, Some(0), "initial path check: {stderr}");
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(status, Some(0), "warm path check: {stderr}");

    fs::write(
        path_root.join("lib/src/Geo.fl"),
        VARIANCE_LIB.replace(
            "opaque type Box a = Box { value : a }",
            "opaque type Box a = Box (Ref a)",
        ),
    )
    .expect("edit lib");
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(
        status,
        Some(1),
        "edited variance must invalidate cache: {stderr}"
    );
    assert!(stderr.contains("FLICK-T003"), "{stderr}");

    fs::write(
        path_root.join("app/src/App/Main.fl"),
        VARIANCE_APP.replace(
            "widen_box : Box (Float -> Float) -> Box (Float ->{Async} Float)",
            "widen_box : Box (Float -> Float) -> Box (Float -> Float)",
        ),
    )
    .expect("update app");
    let inspect_after = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["inspect", "Geo", "--emit", "variance"])
        .current_dir(path_root.join("app"))
        .env("XDG_CACHE_HOME", path_root.join("cache"))
        .env("NO_COLOR", "1")
        .output()
        .expect("inspect variance after edit");
    assert_eq!(inspect_after.status.code(), Some(0), "{inspect_after:?}");
    let after_out = String::from_utf8_lossy(&inspect_after.stdout);
    assert!(
        after_out.contains("opaque type Box : a (inv, invariant)\n"),
        "{after_out}"
    );
}

#[test]
fn class_variance_survives_offline_cache_and_invalidates_on_edit() {
    let lib = "\
module Geo exposing (Box(..), Default(..), Describe(..), MyFunctor(..))

type Box a = Box { value : a }

class Default a =
  default : a

class Describe a =
  describe : a -> String

class MyFunctor (f : Type -> Type) =
  my_map : (a -> b) -> f a -> f b

instance Default (Box (Unit -> Int)) =
  let default = Box { value = fun () -> 41 }

instance Describe (Unit ->{Async} Unit) =
  let describe _ = \"async\"
";
    let app = "\
module App.Main exposing (main, box_async, desc_pure)

import Geo exposing (..)

box_async : Box (Unit ->{Async} Int)
let box_async = default

desc_pure : String
let desc_pure = describe (fun () -> ())

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line desc_pure
";

    let (root, geo) = fetched("package-cache-class-variance");
    fs::write(&geo, lib).expect("write fetched lib");
    fs::write(root.join("app/src/App/Main.fl"), app).expect("write app");

    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(
        status,
        Some(0),
        "cold fetched class variance check: {stderr}"
    );

    fs::remove_file(&geo).expect("remove fetched source");
    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(
        status,
        Some(0),
        "offline cached class variance check: {stderr}"
    );

    let inspect_var = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["inspect", "Geo", "--emit", "variance"])
        .current_dir(root.join("app"))
        .env("FLICK_STORE", root.join("store"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("NO_COLOR", "1")
        .output()
        .expect("inspect variance");
    assert_eq!(inspect_var.status.code(), Some(0), "{inspect_var:?}");
    let var_out = String::from_utf8_lossy(&inspect_var.stdout);
    for expected in [
        "class Default : a (+, covariant)\n",
        "class Describe : a (-, contravariant)\n",
        "class MyFunctor : f (inv, invariant)\n",
    ] {
        assert!(var_out.contains(expected), "`{expected}` in\n{var_out}");
    }

    // Editing a path dependency class method to change variance invalidates cache.
    let path_root = project(
        "package-cache-class-variance-invalidate",
        &[
            (
                "app/flick.toml",
                &APP.replace("DEPENDENCY", "{ path = \"../lib\" }"),
            ),
            ("app/src/App/Main.fl", app),
            ("lib/flick.toml", LIB),
            ("lib/src/Geo.fl", lib),
        ],
    );
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(status, Some(0), "initial path check: {stderr}");
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(status, Some(0), "warm path check: {stderr}");

    fs::write(
        path_root.join("lib/src/Geo.fl"),
        lib.replace("describe : a -> String", "describe : a -> a")
            .replace("let describe _ = \"async\"", "let describe f = f"),
    )
    .expect("edit lib");
    fs::write(
        path_root.join("app/src/App/Main.fl"),
        app.replace(
            "desc_pure : String\nlet desc_pure = describe (fun () -> ())",
            "desc_pure : Unit -> Unit\nlet desc_pure = describe (fun () -> ())",
        )
        .replace(
            "let main () = Platform.stdout_line desc_pure",
            "let main () = Platform.stdout_line \"ok\"",
        ),
    )
    .expect("update app signature");
    let (status, stderr) = flick_output(&path_root, "cache", &["check"]);
    assert_eq!(
        status,
        Some(1),
        "edited class variance must invalidate cache and reject invariant mismatch: {stderr}"
    );
    assert!(stderr.contains("FLICK-C001"), "{stderr}");
}
