use std::fs;

use serde_json::{Value, json};

use crate::common::{flick_in, project};
use crate::prove_contracts::json_diagnostics;

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
pub(crate) const PROOF_LINT_CONFIG: &str =
    "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[lint]\nunused-proof-premises = true\n";

pub(crate) const PROOF_LINTS: &str = r"module App.Laws exposing (..)

class Keep a =
  keep : a -> a
  law keeps (x : a) (h : equal x x) where equal (keep x) x

instance Keep Bool =
  let keep x = x

proof from_law (x : Bool) (extra : equal x x) where equal (keep x) x = refl
proof instance_laws where lawful Keep Bool = { keeps = from_law }

proof two_swaps (x : Bool) (y : Bool) (h : equal x y) where equal x y = symm (symm h)
proof one_swap (x : Bool) (y : Bool) (h : equal x y) where equal y x = symm h
proof in_goal (h : holds true) where holds ((fun _ -> true) h) = refl
proof in_later (h : holds true) (p : holds ((fun _ -> true) h)) where holds true = p
";

pub(crate) fn by_code<'a>(diagnostics: &'a [Value], code: &str) -> &'a Value {
    diagnostics
        .iter()
        .find(|d| d["code"] == code)
        .unwrap_or_else(|| panic!("missing {code}: {diagnostics:?}"))
}

#[test]
fn prove_applies_the_project_warning_policy_and_safe_proof_edit() {
    let path = "proofs/App/Laws.fl";
    let dir = project("prove_lints", &[("flick.toml", LIB), (path, PROOF_LINTS)]);
    let prove = |extra: &[&str]| {
        let mut args = vec!["--diagnostics", "json", "prove"];
        args.extend_from_slice(extra);
        let run = flick_in(&dir, &args);
        (run.status.code(), json_diagnostics(&run.stderr))
    };
    let (status, diagnostics) = prove(&[]);
    assert_eq!(status, Some(0), "{diagnostics:?}");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "FLICK-T091");
    assert_eq!(diagnostics[0]["severity"], "warning");

    fs::write(dir.join("flick.toml"), PROOF_LINT_CONFIG).expect("enable unused premises");
    let (status, diagnostics) = prove(&["--warnings-as-errors"]);
    assert_eq!(status, Some(1), "{diagnostics:?}");
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    let swapped = by_code(&diagnostics, "FLICK-T091");
    let unused = by_code(&diagnostics, "FLICK-T092");
    assert_eq!(unused["severity"], "error");
    let premise = PROOF_LINTS.find("extra :").expect("premise binder");
    assert_eq!(
        unused["range"],
        json!({ "start": premise, "end": premise + 5 })
    );
    assert_eq!(unused["fixes"], json!([]));

    let source = "symm (symm h)";
    let start = PROOF_LINTS.find(source).expect("redundant proof");
    let end = start + source.len();
    assert_eq!(swapped["range"], json!({ "start": start, "end": end }));
    let edit = &swapped["fixes"][0]["edits"][0];
    assert_eq!(edit["range"], swapped["range"]);
    assert_eq!(edit["replacement"], "h");

    fs::write(
        dir.join("flick.toml"),
        format!("{PROOF_LINT_CONFIG}allow = [\"T091\", \"FLICK-T092\"]\n"),
    )
    .expect("suppress both warnings");
    assert_eq!(prove(&["--warnings-as-errors"]), (Some(0), vec![]));

    let mut fixed = PROOF_LINTS.to_owned();
    fixed.replace_range(
        start..end,
        edit["replacement"].as_str().expect("replacement"),
    );
    fs::write(dir.join(path), fixed).expect("apply fix");
    fs::write(dir.join("flick.toml"), PROOF_LINT_CONFIG).expect("restore warnings");
    let (status, diagnostics) = prove(&[]);
    assert_eq!(status, Some(0), "{diagnostics:?}");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "FLICK-T092");
    assert_eq!(diagnostics[0]["severity"], "warning");
}
