use super::*;

const VARIANCE_LIB: &str = "\
module Geo exposing (Box, Sink, Cell, Abstract)

opaque type Box a = Box { value : a }
opaque type Sink a = Sink (a -> Unit)
opaque type Cell a = Cell (Ref a)
type Abstract a
";

const VARIANCE_APP: &str = "\
module App.Main exposing (main, widen_box, narrow_sink)

import Geo exposing (Box, Sink)

widen_box : Box (Float -> Float) -> Box (Float ->{Async} Float)
let widen_box b = b

narrow_sink : Sink (Float ->{Async} Float) -> Sink (Float -> Float)
let narrow_sink s = s

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line \"ok\"
";

#[test]
fn fetched_package_preserves_opaque_variance_offline_and_in_inspect() {
    let (root, geo) = fetched("package-cache-opaque-variance");
    fs::write(&geo, VARIANCE_LIB).expect("write fetched lib");
    fs::write(root.join("app/src/App/Main.fl"), VARIANCE_APP).expect("write app");

    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(0), "cold fetched check: {stderr}");

    fs::remove_file(&geo).expect("remove fetched source");
    let (status, stderr) = flick_output(&root, "cache", &["check"]);
    assert_eq!(status, Some(0), "offline cached check: {stderr}");

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
    let box_variance = iface_json["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .find(|e| e["name"] == "Box")
        .expect("Box")["definition"]["variance"]
        .clone();
    assert_eq!(box_variance, json!(["covariant"]));
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
