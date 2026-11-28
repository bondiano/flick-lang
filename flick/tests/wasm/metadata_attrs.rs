//! End-to-end WASM plugin attribute validation and `0.5.0` incompatibility tests.

use std::fs;

use super::metadata::{META_APP, meta_app, run_flick};
use super::{APP, LIB, MAIN, PAIRS, component as pairs_component, flick};
use crate::common::project;

#[test]
fn external_wasm_rejects_unknown_misplaced_and_malformed_custom_attributes_at_exact_spans() {
    // 1. Unknown and misplaced plugin attributes produce FLICK-S014 at exact spans.
    let misplaced = r#"module App.Main exposing (main, Bad(..))

import Meta

@rename "on_decl"
type Bad = Bad {
  @tag "on_field"
  @bogus
  x : Int
}

main : Unit ->{; AppError} Unit
let main () = ()
"#;
    let root = meta_app(
        "meta-bad-attr-placement",
        META_APP,
        &[("app/src/App/Main.fl", misplaced)],
    );
    let out = run_flick(&root, &["check", "--diagnostics", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-S014"), "{stderr}");
    assert!(stderr.contains("belongs on a record field"), "{stderr}");
    assert!(stderr.contains("belongs on a declaration"), "{stderr}");
    assert!(stderr.contains("unknown attribute `@bogus`"), "{stderr}");
    let rename_start = misplaced.find("@rename \"on_decl\"").expect("rename start");
    let rename_end = rename_start + "@rename \"on_decl\"".len();
    assert!(
        stderr.contains(&format!("\"start\":{rename_start}"))
            && stderr.contains(&format!("\"end\":{rename_end}")),
        "expected exact span {rename_start}..{rename_end}:\n{stderr}"
    );

    // 2. Malformed plugin field attribute argument points at exact `attr.span` (`meta/M004`).
    let malformed = r"module App.Main exposing (main, Bad(..))

import Meta

type Bad = Bad {
  @rename 123
  x : Int
} deriving Meta.Codec

main : Unit ->{; AppError} Unit
let main () = ()
";
    let root_bad = meta_app(
        "meta-bad-attr-arg",
        META_APP,
        &[("app/src/App/Main.fl", malformed)],
    );
    let out_bad = run_flick(&root_bad, &["check", "--diagnostics", "json"]);
    let stderr_bad = String::from_utf8_lossy(&out_bad.stderr);
    assert!(!out_bad.status.success(), "{stderr_bad}");
    assert!(stderr_bad.contains("meta/M004"), "{stderr_bad}");
    let attr_start = malformed.find("@rename 123").expect("attr start");
    let attr_end = attr_start + "@rename 123".len();
    assert!(
        stderr_bad.contains(&format!("\"start\":{attr_start}"))
            && stderr_bad.contains(&format!("\"end\":{attr_end}")),
        "M004 did not point at exact attribute span {attr_start}..{attr_end}:\n{stderr_bad}"
    );
}

#[test]
fn incompatible_0_5_0_wasm_component_reports_p035_rebuild_diagnostic() {
    let root = project(
        "plugins-incompatible-0-5-0",
        &[
            ("app/flick.toml", APP),
            ("app/src/App/Main.fl", MAIN),
            ("lib/flick.toml", LIB),
            ("lib/src/Pairs.fl", PAIRS),
        ],
    );
    let mut bytes = fs::read(pairs_component()).expect("read pairs component");
    let needle = b"flick:plugin/compiler@0.6.0";
    let pos = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("component imports flick:plugin/compiler@0.6.0");
    bytes[pos..pos + needle.len()].copy_from_slice(b"flick:plugin/compiler@0.5.0");
    fs::write(root.join("lib/pairs.wasm"), bytes).expect("write patched component");

    let out = flick(&root, &["check", "--diagnostics", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P035"), "{stderr}");
    assert!(stderr.contains("flick:plugin@0.5.0"), "{stderr}");
    assert!(stderr.contains("0.6.0"), "{stderr}");
    assert!(stderr.contains("rebuild it"), "{stderr}");
}
