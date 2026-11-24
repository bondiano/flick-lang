use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::{Class, MANIFEST, Manifest, VERSION, manifest};

/// The `(specifier, export)` pair a `foreign` line binds, `global` for a
/// JS global; `None` for a line that is no `foreign` declaration.
fn binding(line: &str) -> Option<Result<(String, String), String>> {
    let line = line.trim_start();
    if !line.starts_with("foreign ") {
        return None;
    }
    let quoted = |s: &str| -> Option<(String, usize)> {
        let rest = s.strip_prefix('"')?;
        let end = rest.find('"')?;
        Some((rest[..end].to_owned(), end + 2))
    };
    let parsed = line.rsplit_once(" from ").and_then(|(_, from)| {
        if let Some(name) = from.strip_prefix("global ") {
            return Some(("global".to_owned(), quoted(name)?.0));
        }
        let (specifier, used) = quoted(from)?;
        Some((specifier, quoted(from[used..].trim_start())?.0))
    });
    Some(parsed.ok_or_else(|| format!("cannot read the binding of `{line}`")))
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("a package directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

/// Every binding of `packages/*/src`, with how many declarations name it.
fn std_bindings() -> BTreeMap<(String, String), usize> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
    let mut files = Vec::new();
    for entry in fs::read_dir(&packages).expect("packages/") {
        let src = entry.expect("a package").path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut out = BTreeMap::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("a source");
        for (n, line) in text.lines().enumerate() {
            match binding(line) {
                Some(Ok(pair)) => *out.entry(pair).or_default() += 1,
                Some(Err(why)) => panic!("{}:{}: {why}", file.display(), n + 1),
                None => {}
            }
        }
    }
    out
}

/// The pairs the manifest gives no class.
fn unclassified<'p>(
    manifest: &Manifest<'_>,
    pairs: impl IntoIterator<Item = &'p (String, String)>,
) -> Vec<&'p (String, String)> {
    pairs
        .into_iter()
        .filter(|(s, e)| manifest.class(s, e).is_none())
        .collect()
}

#[test]
fn the_manifest_states_the_handshake_version() {
    assert_eq!(manifest().version, VERSION);
    assert!(
        MANIFEST
            .lines()
            .any(|line| line == format!("version {VERSION}"))
    );
}

#[test]
fn every_binding_of_std_has_a_class() {
    let bindings = std_bindings();
    let missing = unclassified(&manifest(), bindings.keys());
    assert!(
        missing.is_empty(),
        "bindings the kernel manifest does not classify; add their specifier or export to flick-backend-support/src/kernel/manifest.txt: {missing:?}"
    );
}

#[test]
fn an_unclassified_binding_is_reported() {
    let pairs = [
        ("flick:rt".to_owned(), "refMake".to_owned()),
        ("flick:Unknown".to_owned(), "thing".to_owned()),
    ];
    assert_eq!(unclassified(&manifest(), &pairs), [&pairs[1]]);
}

#[test]
fn the_manifest_names_only_what_std_binds() {
    let bindings = std_bindings();
    let specifiers: BTreeSet<&str> = bindings.keys().map(|(s, _)| s.as_str()).collect();
    let manifest = manifest();
    let stale: Vec<_> = (manifest.specifiers())
        .filter(|s| !specifiers.contains(s))
        .map(str::to_owned)
        .chain(
            manifest
                .exports()
                .filter(|&(s, e, _)| !bindings.contains_key(&(s.to_owned(), e.to_owned())))
                .map(|(s, e, _)| format!("{s} {e}")),
        )
        .collect();
    assert!(stale.is_empty(), "named but not bound by std: {stale:?}");
}

#[test]
fn the_kernel_is_a_small_fraction_of_the_bindings() {
    let bindings = std_bindings();
    let manifest = manifest();
    let mut pairs: BTreeMap<&str, usize> = BTreeMap::new();
    let mut declarations: BTreeMap<&str, usize> = BTreeMap::new();
    for ((s, e), count) in &bindings {
        let class = manifest.class(s, e).map_or("unclassified", Class::name);
        *pairs.entry(class).or_default() += 1;
        *declarations.entry(class).or_default() += count;
    }
    // `cargo test -p flick-backend-support kernel -- --nocapture` prints them
    println!(
        "kernel {VERSION}: {} pairs {pairs:?}; {} declarations {declarations:?}",
        bindings.len(),
        declarations.values().sum::<usize>()
    );
    let kernel = pairs.get("kernel").copied().unwrap_or_default();
    assert!(kernel > 0 && kernel * 20 < bindings.len(), "{pairs:?}");
}

#[test]
fn a_malformed_manifest_is_refused() {
    for (text, why) in [
        ("[flick:rt] flick\n", "states no version"),
        ("version 0.1.0\n[flick:rt] kernel\n", "a line of its own"),
        (
            "version 0.1.0\n[flick:rt] flick\nshow flick\n",
            "its specifier's class",
        ),
        (
            "version 0.1.0\n[flick:rt] flick\nshow kernel\nshow host\n",
            "named twice",
        ),
        (
            "version 0.1.0\n[flick:rt] flick\n[flick:rt] host\n",
            "named twice",
        ),
        ("version 0.1.0\nshow kernel\n", "before any specifier"),
        ("version 0.1.0\n[flick:rt] native\n", "unknown class"),
    ] {
        let error = Manifest::parse(text).expect_err(text);
        assert!(error.contains(why), "{text}: {error}");
    }
    let parsed = Manifest::parse("version 1.2.3\n[global] flick\nNumber kernel\n").expect("valid");
    assert_eq!(parsed.version, "1.2.3");
    assert_eq!(parsed.class("global", "Number"), Some(Class::Kernel));
    assert_eq!(parsed.class("global", "Math.PI"), Some(Class::Flick));
    assert_eq!(parsed.class("flick:rt", "show"), None);
}

#[test]
fn foreign_lines_are_read_by_specifier_and_export() {
    assert_eq!(
        binding("foreign pure length from \"flick:String\" \"length\""),
        Some(Ok(("flick:String".into(), "length".into())))
    );
    assert_eq!(
        binding("  foreign pure pi from global \"Math.PI\""),
        Some(Ok(("global".into(), "Math.PI".into())))
    );
    assert!(matches!(
        binding("foreign pure f from inline \"x\""),
        Some(Err(_))
    ));
    assert_eq!(
        binding("foreign pure from from \"flick:rt\" \"seqFrom\""),
        Some(Ok(("flick:rt".into(), "seqFrom".into())))
    );
    assert_eq!(binding("let foreign_name = 1"), None);
}
