//! The Go runtime, written beside every program at `flickrt/`.

use std::collections::{BTreeMap, BTreeSet};

/// The runtime's files: name and contents, every `.go` file of
/// `runtime-go/flickrt` but its tests (`build.rs`).
pub const RUNTIME: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/go_runtime.rs"));

/// An optional package of the runtime, written beside a program that
/// reaches it, such as the `PostgreSQL` driver `flickpg`.
pub struct Package {
    pub name: &'static str,
    /// Its `.go` files but tests, by path in the program's Go module.
    pub files: &'static [(&'static str, &'static str)],
    /// The modules it needs, one path a line (`modules.txt`); `GO_MOD`
    /// has their versions.
    pub modules: &'static str,
}

/// Every directory of `runtime-go` but `flickrt` (`build.rs`).
pub const PACKAGES: &[Package] = include!(concat!(env!("OUT_DIR"), "/go_packages.rs"));

/// The runtime's `go.mod` and `go.sum`, which require and pin the modules
/// of every package; a program's `go.mod` keeps those of its packages.
pub const GO_MOD: &str = include_str!("../../../../runtime-go/go.mod");
pub const GO_SUM: &str = include_str!("../../../../runtime-go/go.sum");

/// The `go.mod` of a program with optional runtime `packages` alone.
pub fn go_mod(packages: &[&Package]) -> String {
    go_mod_with_deps(packages, &BTreeMap::new())
}

/// The `go.mod` of a program with optional runtime `packages` and declared
/// `[target.go.dependencies]` (`extra_deps`). Without either, emits an offline
/// zero-requirement `go 1.23` module.
pub fn go_mod_with_deps(packages: &[&Package], extra_deps: &BTreeMap<String, String>) -> String {
    if packages.is_empty() && extra_deps.is_empty() {
        return "module flick.app\n\ngo 1.23\n".to_owned();
    }
    let needed: Vec<&str> = packages.iter().flat_map(|p| p.modules.lines()).collect();
    let mut header = String::new();
    let mut requires: BTreeMap<String, String> = BTreeMap::new();
    let mut block = false;
    for line in GO_MOD.lines() {
        let line = line.trim();
        let require = if block {
            block = line != ")";
            line
        } else if line == "require (" {
            block = true;
            continue;
        } else if let Some(rest) = line.strip_prefix("require ") {
            rest
        } else {
            if line.starts_with("module ") || line.starts_with("go ") {
                header.push_str(line);
                header.push_str("\n\n");
            }
            continue;
        };
        if let Some((module, rest)) = require.split_once(char::is_whitespace)
            && needed.contains(&module)
        {
            requires.insert(module.to_owned(), rest.trim().to_owned());
        }
    }
    if header.is_empty() {
        "module flick.app\n\ngo 1.23\n\n".clone_into(&mut header);
    }
    for (module, version) in extra_deps {
        requires.insert(module.clone(), version.clone());
    }
    if requires.is_empty() {
        return "module flick.app\n\ngo 1.23\n".to_owned();
    }
    let mut body = String::new();
    for (module, version) in requires {
        body.push('\t');
        body.push_str(&module);
        body.push(' ');
        body.push_str(&version);
        body.push('\n');
    }
    format!("{header}require (\n{body})\n")
}

/// Merges optional runtime and project `go.sum` contents deterministically,
/// deduplicating lines in sorted order.
pub fn merge_go_sum(runtime_sum: Option<&str>, project_sum: Option<&str>) -> Option<String> {
    let mut lines = BTreeSet::new();
    for text in [runtime_sum, project_sum].into_iter().flatten() {
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.insert(trimmed.to_owned());
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    Some(out)
}
