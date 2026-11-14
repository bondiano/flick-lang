use super::rename;
use crate::ir::ItemKind;
use crate::ir::tests::check_text;

/// A dictionary a matched value holds is a local of its arm: a copy of the
/// arm that inlining renames reads the dictionary its own pattern binds.
#[test]
fn renaming_an_arm_renames_the_dictionaries_its_pattern_binds() {
    let (ir, _) = check_text(
        "App",
        "module App exposing (describe)\n\
         type Showable =\n  | Show b => Showable b : Showable\n\
         describe : Showable -> String\n\
         let describe s =\n  match s with\n  | Showable x -> show x\n",
    )
    .expect("lowers");
    let mut body = ir
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Value { name, body, .. } if name == "describe" => Some(body.clone()),
            _ => None,
        })
        .expect("describe");
    let before = format!("{body:?}");
    assert!(before.contains("Field(\"$dict"), "{before}");
    rename(&mut body, "_copy");
    let after = format!("{body:?}");
    let binds = after.matches("Bind(\"$dict").count();
    let renamed_binds = after.matches("_0_copy\")").count();
    assert!(
        binds > 0 && after.contains("Field(\"$dict") && renamed_binds >= 2,
        "{after}"
    );
    assert!(
        !after.contains("_0\")"),
        "a dictionary local kept its old name: {after}"
    );
}
