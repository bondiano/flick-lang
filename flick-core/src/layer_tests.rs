//! The modules of this crate are layered: a module names, by `crate::`, only
//! modules of its own layer and of those below. A test reads the sources,
//! so an import against the order fails here and not in a review.

use std::fs;
use std::path::{Path, PathBuf};

/// The layers, lowest first. Tests name what they like.
const LAYERS: &[&[&str]] = &[
    &[
        "source",
        "diagnostics",
        "clock",
        "implicit_packages",
        "manifest",
        "markdown",
    ],
    &["syntax", "consteval", "ffi", "builtin_source"],
    &[
        "fmt", "types", "resolve", "iface", "plugin", "doc", "numeric", "api_diff",
    ],
    &["ir", "opt", "codegen", "effects"],
    &["project"],
    &["build", "projection"],
    &["ide"],
];

fn layer(module: &str) -> Option<usize> {
    LAYERS.iter().position(|names| names.contains(&module))
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("a source directory").flatten() {
        let (path, name) = (
            entry.path(),
            entry.file_name().to_string_lossy().into_owned(),
        );
        if name.contains("tests") || name == "snapshots" {
            continue;
        }
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// The modules a source names after `crate::`, one of a `{…}` group each.
fn named(text: &str) -> Vec<&str> {
    let ident = |s: &'_ str| -> usize {
        s.find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(s.len())
    };
    let mut found = Vec::new();
    for (at, _) in text.match_indices("crate::") {
        let rest = &text[at + "crate::".len()..];
        let Some(group) = rest.strip_prefix('{') else {
            found.push(&rest[..ident(rest)]);
            continue;
        };
        let mut depth = 0_usize;
        let mut start = Some(0);
        for (i, c) in group.char_indices() {
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                ',' if depth == 0 => start = Some(i + 1),
                c if depth == 0 && !c.is_whitespace() => {
                    if let Some(from) = start.take() {
                        let name = group[from..].trim_start();
                        found.push(&name[..ident(name)]);
                    }
                }
                _ => {}
            }
        }
    }
    found
}

#[test]
fn a_module_names_only_its_layer_and_those_below() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    files.sort();
    let mut against = Vec::new();
    for file in files {
        let path = file.strip_prefix(&root).expect("under src");
        let first = path.iter().next().expect("a name").to_string_lossy();
        let module = first.trim_end_matches(".rs");
        if module == "lib" {
            continue;
        }
        let own = layer(module).unwrap_or_else(|| panic!("give `{module}` a layer"));
        let text = fs::read_to_string(&file).expect("a readable source");
        // the tests of a file are its last module
        let code = text.split("\n#[cfg(test)]").next().unwrap_or(&text);
        for name in named(code) {
            if layer(name).is_some_and(|other| other > own) {
                against.push(format!("{} names `{name}`", path.display()));
            }
        }
    }
    against.dedup();
    assert!(
        against.is_empty(),
        "imports against the layers:\n{}",
        against.join("\n")
    );
}
