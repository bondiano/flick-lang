//! The Effect coverage linter: every export of the pinned Effect's snapshot
//! has exactly one decision in the manifest `stdlib/Effect/surface/`, and
//! every `wrapped` decision names a symbol of the standard library that a
//! stdlib test uses, reviewed against the pinned release.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "ide")]
use std::fmt::Write;

use serde::Deserialize;

/// `stdlib/Effect/snapshot.json`, written by `tools/dts-importer/index.mjs surface`.
#[derive(Debug, Deserialize)]
pub struct Snapshot {
    pub version: String,
    pub entrypoints: Vec<String>,
    /// Entrypoint → `Module.name` → `kind fingerprint`.
    pub modules: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Wrapped { target: String, fingerprint: String },
    Language(String),
    Excluded(String),
    Pending(String),
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Wrapped { .. } => "wrapped",
            Status::Language(_) => "language",
            Status::Excluded(_) => "excluded",
            Status::Pending(_) => "pending",
        }
    }
}

/// The decisions of the manifest.
#[derive(Debug, Default)]
pub struct Manifest {
    /// Entrypoint → the manifest directory of its modules, or why it is
    /// excluded as a whole.
    pub entrypoints: BTreeMap<String, Result<String, String>>,
    /// (directory, module) → export name (`*` for every other one) → status.
    pub modules: BTreeMap<(String, String), BTreeMap<String, Status>>,
    /// Problems of the files themselves.
    pub problems: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entrypoint {
    modules: Option<String>,
    excluded: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ModuleFile {
    #[serde(default)]
    wrapped: BTreeMap<String, String>,
    #[serde(default)]
    language: BTreeMap<String, String>,
    #[serde(default)]
    excluded: BTreeMap<String, String>,
    #[serde(default)]
    pending: BTreeMap<String, String>,
}

impl Manifest {
    /// Reads `entrypoints.toml` and the module files: `(path, text)` with
    /// paths relative to the manifest directory, `<dir>/<Module>.toml` or
    /// `<dir>/<Module>.<part>.toml` for a module split into parts.
    pub fn parse(files: &[(String, String)]) -> Manifest {
        let mut manifest = Manifest::default();
        for (path, text) in files {
            if path == "entrypoints.toml" {
                manifest.entrypoints_file(text);
            } else if let Some((dir, file)) = path.split_once('/')
                && let Some(stem) = file.strip_suffix(".toml")
            {
                let module = stem.split('.').next().unwrap_or(stem);
                manifest.module_file(path, dir, module, text);
            } else {
                manifest
                    .problems
                    .push(format!("{path}: not a manifest file"));
            }
        }
        manifest
    }

    fn entrypoints_file(&mut self, text: &str) {
        match toml::from_str::<BTreeMap<String, Entrypoint>>(text) {
            Ok(entries) => {
                for (name, e) in entries {
                    let decision = match (e.modules, e.excluded) {
                        (Some(dir), None) => Ok(dir),
                        (None, Some(reason)) if !reason.trim().is_empty() => Err(reason),
                        _ => {
                            self.problems.push(format!(
                                "entrypoint `{name}`: give exactly one of `modules` or a non-empty `excluded`"
                            ));
                            continue;
                        }
                    };
                    self.entrypoints.insert(name, decision);
                }
            }
            Err(err) => self.problems.push(format!("entrypoints.toml: {err}")),
        }
    }

    fn module_file(&mut self, path: &str, dir: &str, module: &str, text: &str) {
        let file: ModuleFile = match toml::from_str(text) {
            Ok(file) => file,
            Err(err) => {
                self.problems.push(format!("{path}: {err}"));
                return;
            }
        };
        let decisions = self
            .modules
            .entry((dir.to_owned(), module.to_owned()))
            .or_default();
        let mut add = |name: String, status: Status, problems: &mut Vec<String>| {
            if decisions.insert(name.clone(), status).is_some() {
                problems.push(format!("{module}.{name}: classified twice"));
            }
        };
        for (name, value) in file.wrapped {
            let status = match value.split_once(' ') {
                Some((target, fingerprint)) if !target.is_empty() && !fingerprint.is_empty() => {
                    Status::Wrapped {
                        target: target.to_owned(),
                        fingerprint: fingerprint.to_owned(),
                    }
                }
                _ => {
                    self.problems.push(format!(
                        "{module}.{name}: `wrapped` takes \"<Flick.Name> <fingerprint>\""
                    ));
                    continue;
                }
            };
            add(name, status, &mut self.problems);
        }
        for (name, why) in file.language {
            add(name, Status::Language(why), &mut self.problems);
        }
        for (name, why) in file.excluded {
            add(name, Status::Excluded(why), &mut self.problems);
        }
        for (name, stage) in file.pending {
            add(name, Status::Pending(stage), &mut self.problems);
        }
    }
}

/// A symbol of the standard library as a binding.
pub enum Binding {
    /// Defined in Flick, or not a value.
    Flick,
    /// A foreign binding without `reviewed`.
    Unreviewed,
    /// A foreign binding with the provenance of its `reviewed` line.
    Reviewed(String),
}

/// What the linter needs of the standard library.
pub struct Stdlib<'a> {
    /// Whether `Module.name` is an exported symbol.
    pub exists: &'a dyn Fn(&str) -> bool,
    /// What kind of binding `Module.name` is.
    pub binding: &'a dyn Fn(&str) -> Binding,
    /// Whether a stdlib test refers to `Module.name`.
    pub used: &'a dyn Fn(&str) -> bool,
}

/// Every problem, each naming `Module.name` and the reason.
pub fn lint(
    snapshot: &Snapshot,
    manifest: &Manifest,
    stdlib: &Stdlib,
    pinned: &str,
) -> Vec<String> {
    let mut out = manifest.problems.clone();
    if snapshot.version != pinned {
        out.push(format!(
            "the snapshot is of effect {}, the runtime pins {pinned}: run `just effect-bump {pinned}`",
            snapshot.version
        ));
    }
    for entrypoint in &snapshot.entrypoints {
        match manifest.entrypoints.get(entrypoint) {
            None => out.push(format!("entrypoint `{entrypoint}`: not classified")),
            Some(Ok(_)) if !snapshot.modules.contains_key(entrypoint) => out.push(format!(
                "entrypoint `{entrypoint}`: has module decisions, but the snapshot does not list its modules"
            )),
            Some(Err(_)) if snapshot.modules.contains_key(entrypoint) => out.push(format!(
                "entrypoint `{entrypoint}`: excluded as a whole, but its exports are in the snapshot"
            )),
            Some(_) => {}
        }
    }
    for name in manifest.entrypoints.keys() {
        if !snapshot.entrypoints.contains(name) {
            out.push(format!(
                "entrypoint `{name}`: no such entrypoint in effect {}",
                snapshot.version
            ));
        }
    }
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for (entrypoint, exports) in &snapshot.modules {
        let Some(Ok(dir)) = manifest.entrypoints.get(entrypoint) else {
            continue;
        };
        for (key, value) in exports {
            let (module, name) = key.split_once('.').unwrap_or((key, ""));
            let fingerprint = value.split_whitespace().nth(1).unwrap_or_default();
            let decisions = manifest.modules.get(&(dir.clone(), module.to_owned()));
            seen.insert((dir.clone(), module.to_owned()));
            let status = decisions.and_then(|d| d.get(name).or_else(|| d.get("*")));
            match status {
                None => out.push(format!("{key}: not classified")),
                Some(status) => out.extend(check(key, fingerprint, status, stdlib, pinned)),
            }
        }
    }
    for ((dir, module), decisions) in &manifest.modules {
        let entrypoint = manifest
            .entrypoints
            .iter()
            .find(|(_, d)| d.as_ref().is_ok_and(|d| d == dir))
            .map(|(e, _)| e);
        let exports = entrypoint.and_then(|e| snapshot.modules.get(e));
        if !seen.contains(&(dir.clone(), module.clone())) || exports.is_none() {
            out.push(format!("{module}: no such module in {dir}"));
            continue;
        }
        for name in decisions.keys().filter(|n| *n != "*") {
            if !exports.is_some_and(|e| e.contains_key(&format!("{module}.{name}"))) {
                out.push(format!("{module}.{name}: no such export"));
            }
        }
    }
    out
}

fn check(
    key: &str,
    fingerprint: &str,
    status: &Status,
    stdlib: &Stdlib,
    pinned: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    match status {
        Status::Wrapped {
            target,
            fingerprint: reviewed,
        } => {
            if reviewed != fingerprint {
                out.push(format!(
                    "{key}: the signature changed (fingerprint {fingerprint}, reviewed {reviewed}): review `{target}`"
                ));
            }
            if !(stdlib.exists)(target) {
                out.push(format!(
                    "{key}: wrapped by `{target}`, which the standard library does not export"
                ));
            } else if !(stdlib.used)(target) {
                out.push(format!("{key}: no stdlib test uses `{target}`"));
            }
            match (stdlib.binding)(target) {
                Binding::Unreviewed => out.push(format!(
                    "{key}: the binding `{target}` has no `reviewed \"effect@{pinned} …\"`"
                )),
                Binding::Reviewed(p) if !p.starts_with(&format!("effect@{pinned} ")) => out.push(format!(
                    "{key}: the binding `{target}` was reviewed against `{p}`, not effect@{pinned}"
                )),
                Binding::Reviewed(_) | Binding::Flick => {}
            }
        }
        Status::Pending(stage) => {
            let owned = stage
                .strip_prefix('M')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if !owned {
                out.push(format!(
                    "{key}: pending without an owning stage (`M<n>`), got `{stage}`"
                ));
            }
        }
        Status::Language(why) | Status::Excluded(why) if why.trim().is_empty() => {
            out.push(format!("{key}: {} without a reason", status.label()));
        }
        Status::Language(_) | Status::Excluded(_) => {}
    }
    out
}

#[cfg(feature = "ide")]
/// A Markdown table of the decisions by entrypoint and module.
pub fn coverage(snapshot: &Snapshot, manifest: &Manifest) -> String {
    let mut out = String::from(
        "| Module | wrapped | language | excluded | pending | unclassified |\n| - | - | - | - | - | - |\n",
    );
    let mut totals = [0usize; 5];
    for (entrypoint, exports) in &snapshot.modules {
        let dir = manifest
            .entrypoints
            .get(entrypoint)
            .and_then(|d| d.as_ref().ok());
        let mut rows: BTreeMap<&str, [usize; 5]> = BTreeMap::new();
        for key in exports.keys() {
            let (module, name) = key.split_once('.').unwrap_or((key, ""));
            let status = dir
                .and_then(|d| manifest.modules.get(&(d.clone(), module.to_owned())))
                .and_then(|d| d.get(name).or_else(|| d.get("*")));
            let column = match status {
                Some(Status::Wrapped { .. }) => 0,
                Some(Status::Language(_)) => 1,
                Some(Status::Excluded(_)) => 2,
                Some(Status::Pending(_)) => 3,
                None => 4,
            };
            rows.entry(module).or_default()[column] += 1;
            totals[column] += 1;
        }
        for (module, counts) in rows {
            let _ = writeln!(out, "| {entrypoint} {module} | {} |", cells(counts));
        }
    }
    let _ = writeln!(out, "| **total** | {} |", cells(totals));
    out
}

#[cfg(feature = "ide")]
fn cells(counts: [usize; 5]) -> String {
    counts.map(|n| n.to_string()).join(" | ")
}
