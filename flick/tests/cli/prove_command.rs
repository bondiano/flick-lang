//! `flick prove` checks the proof modules of the project's proof root;
//! `flick check` never reads that root, so a false proof there fails only
//! the command that asks for proofs.

use std::fs;

use serde_json::{Value, json};

use crate::common;
use crate::prove_contracts::json_diagnostics;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const PROOF: &str = "module App.Laws exposing (..)\n\nproof right_identity (v : Option a)\nwhere equal (flat_map pure v) v =\n  match v with\n  | None -> refl\n  | Some x -> refl\n";

const LIBRARY: &str = "module App.Lib exposing (..)\n\nanswer : Int\nlet answer = 42\n";

pub(crate) const STEP_PROOFS: &str = "module App.Laws exposing (..)\n\
\n\
proof given (x : Bool) (h : equal x true) where equal x true = h\n\
proof wrong_step where equal true true = trans false refl refl\n\
proof wrong_premise where equal true true = given false refl\n\
proof refuted where equal true false = refl\n";

pub(crate) fn proof_step_diagnostic<'a>(diagnostics: &'a [Value], name: &str) -> &'a Value {
    diagnostics
        .iter()
        .find(|d| d["message"].as_str().unwrap_or_default().contains(name))
        .unwrap_or_else(|| panic!("missing {name} in {diagnostics:?}"))
}

#[test]
fn prove_points_to_the_obligation_that_failed() {
    let dir = project(
        "prove_failed_obligation",
        &[("flick.toml", LIB), ("proofs/App/Laws.fl", STEP_PROOFS)],
    );
    let broken = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(broken.status.code(), Some(1), "{broken:?}");
    let diagnostics = json_diagnostics(&broken.stderr);
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    let offset = |needle: &str| STEP_PROOFS.find(needle).expect("fixture text");
    for (name, obligation, argument) in [
        ("wrong_step", "true = false", "trans false "),
        ("wrong_premise", "false = true", "given false "),
    ] {
        let d = proof_step_diagnostic(&diagnostics, name);
        let start = offset(argument) + argument.len();
        assert_eq!(d["code"], "FLICK-T090", "{d}");
        assert_eq!(d["range"], json!({ "start": start, "end": start + 4 }));
        let message = d["message"].as_str().expect("message");
        assert!(message.contains(obligation), "{d}");
        assert!(message.contains("different values"), "{d}");
        let related = d["related"].as_array().expect("related locations");
        let declaration = offset(&format!("proof {name}"));
        assert!(related.iter().any(|r| r["range"]["start"] == declaration));
        if name == "wrong_premise" {
            let lemma = offset("proof given");
            assert!(related.iter().any(|r| r["range"]["start"] == lemma));
        }
    }
    let d = proof_step_diagnostic(&diagnostics, "refuted");
    assert_eq!(d["code"], "FLICK-T080");
    assert!(
        d["message"]
            .as_str()
            .unwrap()
            .contains("`refuted` is false")
    );

    let fixed = STEP_PROOFS
        .replace("trans false refl refl", "refl")
        .replace("given false refl", "refl")
        .replace("equal true false", "equal true true");
    fs::write(dir.join("proofs/App/Laws.fl"), fixed).expect("replace proof bodies");
    let proved = flick_in(&dir, &["prove"]);
    assert_eq!(proved.status.code(), Some(0), "{proved:?}");
}

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

const STEPS: &str =
    "module App.Steps exposing (..)\n\ntwice : (a -> a) -> a -> a\nlet twice f x = f (f x)\n";

const BASE: &str = "module App.Base exposing (..)\n\nimport App.Steps exposing (twice)\n\nproof unfolded (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl\n";

const USES: &str = "module App.Uses exposing (..)\n\nimport App.Steps exposing (twice)\nimport App.Base as B\n\nproof again (g : Bool -> Bool) (b : Bool) where equal (twice g b) (g (g b)) = B.unfolded g b\n";

/// A proof unfolds a definition of another module and uses a lemma of
/// another proof module, which it replays. An edit that keeps the
/// signature but breaks the law fails both proofs, the lemma's user
/// because no lemma checked; restoring it proves them again.
#[test]
fn proofs_read_other_modules_and_recheck_after_a_body_edit() {
    let dir = project(
        "prove_across_modules",
        &[
            ("flick.toml", LIB),
            ("src/App/Steps.fl", STEPS),
            ("proofs/App/Base.fl", BASE),
            ("proofs/App/Uses.fl", USES),
        ],
    );
    let proved = flick_in(&dir, &["prove"]);
    assert_eq!(proved.status.code(), Some(0), "{proved:?}");
    fs::write(
        dir.join("src/App/Steps.fl"),
        STEPS.replace("f (f x)", "f x"),
    )
    .expect("edit the body");
    let broken = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&broken.stderr);
    assert_eq!(broken.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("FLICK-T081") && stderr.contains("`unfolded`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`App.Base.unfolded` is no proof checked before this one"),
        "{stderr}"
    );
    // an ordinary check of the same edit is clean: no proof is its business
    assert_eq!(flick_in(&dir, &["check"]).status.code(), Some(0));
    fs::write(dir.join("src/App/Steps.fl"), STEPS).expect("restore the body");
    let restored = flick_in(&dir, &["prove"]);
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
}

/// Only a proof names another module's proof: in ordinary code `M.name` of
/// a proof is a name the module does not export.
#[test]
fn a_proof_is_no_value_outside_a_proof() {
    let leak = format!("{USES}\nleaked : Int\nlet leaked = B.unfolded\n");
    let dir = project(
        "prove_lemma_outside_a_proof",
        &[
            ("flick.toml", LIB),
            ("src/App/Steps.fl", STEPS),
            ("proofs/App/Base.fl", BASE),
            ("proofs/App/Uses.fl", &leak),
        ],
    );
    let refused = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("FLICK-R004") && stderr.contains("does not export `unfolded`"),
        "{stderr}"
    );
}
