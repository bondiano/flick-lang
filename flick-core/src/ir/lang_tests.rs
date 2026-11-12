//! The compiler finds the built-in entities it relies on by lang item, not
//! by name: core modules that rename them still check, and a program over
//! them checks and lowers.

use crate::diagnostics::codes;
use crate::types::lang::{self, Lang, Table};
use crate::types::{builtin_sources, check_sources, implicit_package_of};

/// The built-in modules up to this one hold the core and what it reaches.
const LAST: &str = "Validation";

const PROGRAM: &str = "module Main exposing (bump, next, evens, half)

bump : Maybe Int32 -> Maybe Int32
let bump x =
  match x with
  | Just n -> Just (n + 1)
  | None -> None

next : Maybe Int32 -> Maybe Int32
let next x =
  let n = x?
  Just (n * 2)

evens : List Int32
let evens = [for n <- [1..<10] yield n * 2]

half : Float -> Float
let half x = x / 2.0
";

/// `text` with each whole word `from` spelled `to`.
fn rename(text: &str, renames: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let hit = renames.iter().find(|(from, _)| *from == word.as_str());
        out.push_str(hit.map_or(word.as_str(), |(_, to)| to));
        word.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[test]
fn renamed_lang_items_still_compile() {
    let renames = [("Option", "Maybe"), ("Some", "Just"), ("Int", "Int32")];
    let last = builtin_sources()
        .iter()
        .position(|(name, _)| *name == LAST)
        .expect("built in");
    let sources: Vec<(String, String)> = builtin_sources()[..=last]
        .iter()
        .map(|(module, text)| (rename(module, &renames), rename(text, &renames)))
        .collect();
    let sources: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    lang::with_table(Table::of(&sources), || {
        assert_eq!(lang::item(Lang::Option).name, "Maybe");
        assert_eq!(lang::item(Lang::Some).name, "Just");
        let builtins = check_sources(&sources);
        let lowered = super::tests::check_text_with("Main", PROGRAM, &builtins);
        assert!(lowered.is_ok(), "{:?}", lowered.err());
    });
}

/// Without std there is no `Fiber.Outlives` to give a `retention owner`
/// declaration: it is an error to report, not a lang item to panic over.
#[test]
fn retention_owner_without_std_is_reported() {
    let core: Vec<(&str, &str)> = builtin_sources()
        .iter()
        .copied()
        .take_while(|(module, _)| implicit_package_of(module).is_none())
        .collect();
    let text = "module Main exposing (keep)\n\n\
                keep : (Eff r none Unit)^c ->{Scope s | r} Unit\n\
                foreign effect keep from \"./keep.js\" \"keep\"\n  retention owner\n";
    lang::with_table(Table::of(&core), || {
        assert_eq!(lang::module_of(Lang::Outlives), None, "std is not loaded");
        let builtins = check_sources(&core);
        let errors = super::tests::check_text_with("Main", text, &builtins).err();
        let codes: Vec<_> = errors.iter().flatten().map(|d| d.code).collect();
        assert_eq!(codes, [codes::F010], "{errors:?}");
    });
}
