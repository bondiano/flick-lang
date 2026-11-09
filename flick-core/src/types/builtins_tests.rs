use std::collections::BTreeSet;

use crate::resolve;

#[test]
fn built_in_signatures_write_nothing_they_get_anyway() {
    super::check_allowing(super::builtin_sources(), &[], false, |_, _| {});
}

#[cfg(feature = "diagnostic-fixes")]
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
