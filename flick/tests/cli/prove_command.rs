//! `flick prove` checks the proof modules of the project's proof root;
//! `flick check` never reads that root, so a false proof there fails only
//! the command that asks for proofs.

use std::fs;

use crate::common;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const PROOF: &str = "module App.Laws exposing (..)\n\nproof right_identity (v : Option a)\nwhere equal (flat_map pure v) v =\n  match v with\n  | None -> refl\n  | Some x -> refl\n";

const LIBRARY: &str = "module App.Lib exposing (..)\n\nanswer : Int\nlet answer = 42\n";

#[test]
fn prove_checks_the_proof_root_and_check_leaves_it_alone() {
    let dir = project(
        "prove_command",
        &[
            ("flick.toml", LIB),
            ("src/App/Lib.fl", LIBRARY),
            ("proofs/App/Laws.fl", PROOF),
        ],
    );
    let proved = flick_in(&dir, &["prove"]);
    assert_eq!(proved.status.code(), Some(0), "{proved:?}");
    let false_proof = PROOF.replace("equal (flat_map pure v) v", "equal (flat_map pure v) None");
    fs::write(dir.join("proofs/App/Laws.fl"), false_proof).expect("write the proof");
    let refuted = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&refuted.stderr);
    assert_eq!(refuted.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("FLICK-T08"), "{stderr}");
    // the ordinary check does not read the proof root
    let checked = flick_in(&dir, &["check"]);
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    fs::remove_dir_all(dir.join("proofs")).expect("remove the proofs");
    let nothing = flick_in(&dir, &["prove"]);
    assert_eq!(nothing.status.code(), Some(0), "{nothing:?}");
    assert!(String::from_utf8_lossy(&nothing.stdout).contains("no proofs"));
}

/// The laws of the `Option` instances of the core hold of their definitions:
/// `flick prove` of `packages/core` checks every one of them.
#[test]
fn the_option_laws_hold_of_the_core_instances() {
    let proved = common::flick(&["prove", "--project", "packages/core"]);
    let stderr = String::from_utf8_lossy(&proved.stderr);
    assert_eq!(proved.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("FLICK-"), "{stderr}");
}
