//! Source-package ownership is independent of the public module spelling.

#![cfg(feature = "native-project")]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use flick_core::implicit_packages::IMPLICIT_PACKAGES;
use flick_core::types;

#[test]
fn standard_library_sources_have_one_dependency_ordered_package_owner() {
    assert_eq!(
        IMPLICIT_PACKAGES,
        [
            "std",
            "stream",
            "process",
            "ui",
            "http",
            "rpc",
            "observability",
            "cli"
        ]
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
    let expected = [
        ("std", "Duration"),
        ("stream", "Stream"),
        ("process", "ChildProcess"),
        ("ui", "Ui"),
        ("http", "HttpApi"),
        ("rpc", "Rpc"),
        ("rpc", "Config"),
        ("observability", "Prometheus"),
        ("cli", "Cli"),
    ];
    let mut all = BTreeSet::new();
    for name in IMPLICIT_PACKAGES {
        let dir = root.join(name);
        let package = types::read_package(name, &dir).expect("a complete source package");
        let listed: BTreeSet<_> = package
            .modules
            .iter()
            .map(|(module, _)| module.clone())
            .collect();
        assert_eq!(
            listed.len(),
            package.modules.len(),
            "duplicate module in {name}"
        );
        for module in &listed {
            assert!(
                all.insert(module.clone()),
                "{module} belongs to more than one package"
            );
        }
        let mut files = BTreeSet::new();
        source_modules(&dir.join("src"), &dir.join("src"), &mut files);
        assert_eq!(
            listed, files,
            "every source of {name} is listed exactly once"
        );
        for (_, module) in expected.iter().filter(|(owner, _)| owner == name) {
            assert!(listed.contains(*module), "{module} belongs to {name}");
        }
    }
    assert_eq!(
        all.len(),
        208,
        "the source split preserves every standard-library module"
    );
}

fn source_modules(root: &Path, dir: &Path, modules: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            source_modules(root, &path, modules);
        } else if path.extension().is_some_and(|extension| extension == "fl") {
            let relative = path.strip_prefix(root).expect("source under root");
            let module = relative
                .with_extension("")
                .to_string_lossy()
                .replace(['/', '\\'], ".");
            assert!(modules.insert(module), "a unique source module");
        }
    }
}
