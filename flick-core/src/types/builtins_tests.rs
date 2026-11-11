use std::collections::BTreeSet;

use crate::resolve;

#[test]
fn built_in_signatures_write_nothing_they_get_anyway() {
    super::check_allowing(super::builtin_sources(), &[], false, |_, _| {});
}

mod fixes {
    use std::cmp::Reverse;
    use std::fs;
    use std::path::Path;

    use crate::diagnostics::{Diagnostic, Edit, codes};
    use crate::types::builtins::{builtin_sources, check_reporting};

    /// Writes the fixes of what `built_in_signatures_write_nothing_they_get_anyway`
    /// refuses into `packages`, then `flick fmt packages` lays them out. A project
    /// of std's own source does not get as far: its instances overlap the built-in
    /// ones, so their methods are not checked.
    #[test]
    #[ignore = "writes packages/*/src"]
    fn write_fixes_of_built_in_signatures() {
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
        let path = |module: &str| {
            let file = format!("src/{}.fl", module.replace('.', "/"));
            (["core", "std"].iter())
                .map(|package| packages.join(package).join(&file))
                .find(|path| path.is_file())
                .expect("the source of a built-in module")
        };
        let mut modules: Vec<(String, String)> = (builtin_sources().iter())
            .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
            .collect();
        loop {
            let mut fixes: Vec<(String, Vec<Edit>)> = Vec::new();
            let sources: Vec<(&str, &str)> = (modules.iter())
                .map(|(name, text)| (name.as_str(), text.as_str()))
                .collect();
            let report = |name: &str, diagnostics: Vec<Diagnostic>| {
                let (ours, other): (Vec<_>, Vec<_>) = (diagnostics.into_iter())
                    .partition(|d| [codes::T059, codes::R012].contains(&d.code));
                assert!(other.is_empty(), "{name}: {other:?}");
                let edits: Vec<Edit> = (ours.iter().flat_map(|d| &d.fixes))
                    .flat_map(|fix| fix.edits.iter().cloned())
                    .collect();
                if !edits.is_empty() {
                    fixes.push((name.to_owned(), edits));
                }
            };
            check_reporting(&sources, report, false, |_, _| {});
            if fixes.is_empty() {
                return;
            }
            for (name, mut edits) in fixes {
                let (_, text) = (modules.iter_mut())
                    .find(|(module, _)| *module == name)
                    .expect("a checked module");
                edits.sort_by_key(|e| Reverse(e.span.start));
                for edit in edits {
                    text.replace_range(edit.span.range(), &edit.replacement);
                }
                fs::write(path(&name), &*text).expect("the source is written");
            }
        }
    }
}

/// A module of an implicit package uses another of them only through an
/// `import`, so what a package needs is written down; the core modules are
/// what every module sees. A qualifier may also name a type or a service
/// under a module: the longest prefix that is a module decides.
#[test]
fn implicit_packages_use_each_other_through_imports() {
    let core = |name: &str| super::CORE.iter().any(|(n, _)| *n == name);
    // the uses no import can say yet: `Bool` names the type `Reducer.Reducer`, and a
    // module with a build without Effect imports only others that have one
    let unwritten = [("Bool", "Reducer")];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut missing: Vec<(String, BTreeSet<String>)> = Vec::new();
    super::check_each(super::builtin_sources(), false, |module, _| {
        let imported = |prefix: &str| {
            (module.syntax.imports.iter()).any(|i| i.qualifier() == prefix || i.module == prefix)
        };
        let unimported: BTreeSet<String> = resolve::qualifiers(module)
            .into_iter()
            .filter_map(|qualifier| {
                let prefixes = (qualifier.match_indices('.').map(|(i, _)| &qualifier[..i]))
                    .chain([qualifier.as_str()])
                    .rev();
                for prefix in prefixes {
                    if imported(prefix) || prefix == module.name || core(prefix) {
                        return None;
                    }
                    if unwritten.contains(&(module.name, prefix)) {
                        return None;
                    }
                    if seen.contains(prefix) {
                        return Some(prefix.to_owned());
                    }
                }
                None
            })
            .collect();
        if !core(module.name) && !unimported.is_empty() {
            missing.push((module.name.to_owned(), unimported));
        }
        seen.insert(module.name.to_owned());
    });
    let lines: Vec<String> = missing
        .iter()
        .map(|(module, uses)| format!("{module}\t{}", Vec::from_iter(uses.clone()).join(" ")))
        .collect();
    assert!(
        lines.is_empty(),
        "modules that use others without an import:\n{}",
        lines.join("\n")
    );
}

#[test]
fn builtins_check_cleanly() {
    let names: Vec<&str> = super::builtins()
        .iter()
        .map(|i| i.module.as_str())
        .collect();
    let sources: Vec<&str> = super::builtin_sources().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, sources);
    assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
    assert_eq!(names[..2], ["Prelude", "Instances"]);
}

/// `Live` in a row runs nothing: `Atom.value`, whose arrow needs the atom's
/// qualifier live once elision reads it, is still read at once (Sync), and a
/// row of `Live` alone is no Sync stage, as the empty row is not.
#[test]
fn live_in_a_row_keeps_a_read_sync() {
    use crate::types::Elem;
    use crate::types::lang::{self, Lang};
    use crate::types::ty::{Row, Type, sync_rows};
    let read = Elem::named(&lang::item(Lang::AtomRead).label);
    let live = (Elem::Live, Type::Var("d".into()));
    let row = |entries| Type::Row(Row::closed(entries));
    let none = Type::empty_row();
    assert!(sync_rows(&row(vec![(read.clone(), none.clone())]), &none));
    assert!(sync_rows(
        &row(vec![live.clone(), (read, none.clone())]),
        &none
    ));
    assert!(!sync_rows(&row(vec![live]), &none));
}

/// A built-in module sees only the ones before it: an import of a later
/// one is unknown there, and lowering would find none of its names.
#[test]
fn a_built_in_module_imports_only_the_ones_before_it() {
    use crate::diagnostics::codes;
    let early = "module Early exposing (..)\n\nimport List.Core\n\n\
                 count : List Int -> Int\nlet count xs = List.Core.length xs\n";
    let mut modules: Vec<(&str, &str)> = super::CORE.to_vec();
    let at = (modules.iter().position(|(n, _)| *n == "List.Core")).expect("a core module");
    modules.insert(at, ("Early", early));
    let mut said = Vec::new();
    let report = |name: &str, diagnostics: Vec<_>| {
        if name == "Early" {
            said = diagnostics;
        }
    };
    super::check_reporting(&modules, report, false, |_, _| {});
    let found: Vec<_> = (said.iter())
        .map(|d| (d.code, &early[d.span.expect("a span").range()]))
        .collect();
    assert_eq!(found, [(codes::R003, "List.Core")], "{said:?}");
}

/// A built-in module that names a later one without importing it hears
/// that the order of the built-in modules is why the name is unknown.
#[test]
fn a_built_in_module_names_only_the_ones_before_it() {
    use crate::diagnostics::codes;
    let early = "module Early exposing (..)\n\n\
                 known : Option Int -> Bool\nlet known value = Option.is_some value\n";
    let mut modules: Vec<(&str, &str)> = super::CORE.to_vec();
    let at = (modules.iter().position(|(n, _)| *n == "Option")).expect("a core module");
    modules.insert(at, ("Early", early));
    let mut said = Vec::new();
    let report = |name: &str, diagnostics: Vec<_>| {
        if name == "Early" {
            said = diagnostics;
        }
    };
    super::check_reporting(&modules, report, false, |_, _| {});
    let found: Vec<_> = (said.iter())
        .map(|d| {
            (
                d.code,
                &early[d.span.expect("a span").range()],
                d.message.as_str(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [(
            codes::T001,
            "Option.is_some",
            "module `Option` comes after this one among the built-in modules: a built-in module names only the ones before it"
        )],
        "{said:?}"
    );
}
