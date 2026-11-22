//! The packages of a JavaScript project, read from the `package.json` files
//! the host hands over: the Flick libraries npm installed, which name their
//! sources with `"flick": { "source-dir" }`, the graphs of the runtime
//! packages the generated code loads, resolved as Node resolves them, build
//! cache inputs, runtime contract verification and `flick doctor` inspection.
//! A `package.json` the host has not read yet is asked for, by its canonical
//! path, in `needs`.

mod doctor;
#[cfg(test)]
mod tests;
mod wanted;

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::command::Dependency;
use flick_backend::{
    Diagnostic, OptionValue, PackageRoot, Packages, ProjectFile, RuntimeGraph, RuntimeNode,
    Severity,
};
use serde_json::Value;

use doctor::{
    ADAPTER, adapter_requirements, check_runtime_diagnostics, doctor_package_diagnostics,
};
pub(super) use doctor::{doctor_descriptor, run_doctor};

/// How many packages one runtime graph may hold.
const LIMIT: usize = 10_000;

pub(super) fn files() -> Vec<String> {
    [
        "package.json",
        "node_modules/*/package.json",
        "node_modules/@*/*/package.json",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The files of this call by the path they were asked with: `None` for one
/// the host found absent.
struct Known<'a> {
    files: BTreeMap<&'a str, Option<&'a ProjectFile>>,
    needs: BTreeSet<String>,
}

impl<'a> Known<'a> {
    fn new(files: &'a [ProjectFile]) -> Self {
        let files = (files.iter())
            .map(|f| (f.path.as_str(), (!f.real.is_empty()).then_some(f)))
            .collect();
        Known {
            files,
            needs: BTreeSet::new(),
        }
    }

    /// The manifest at `path`: `Err` when it is not known yet, which asks
    /// for it.
    fn get(&mut self, path: &str) -> Result<Option<&'a ProjectFile>, ()> {
        if let Some(found) = self.files.get(path) {
            Ok(*found)
        } else {
            self.needs.insert(path.to_owned());
            Err(())
        }
    }
}

fn manifest(file: &ProjectFile) -> Value {
    serde_json::from_slice(&file.bytes).unwrap_or(Value::Null)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

/// The names of `manifest`'s dependencies of `keys`, sorted.
fn names(manifest: &Value, keys: &[&str]) -> BTreeSet<String> {
    (keys.iter())
        .filter_map(|key| manifest[*key].as_object())
        .flat_map(|entries| entries.keys().cloned())
        .collect()
}

/// The directory of a canonical `package.json` path.
fn dir(real: &str) -> &str {
    real.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn installed_file<'a>(known: &Known<'a>, package: &str) -> Option<&'a ProjectFile> {
    let path = format!("node_modules/{package}/package.json");
    known.files.get(path.as_str()).copied().flatten()
}

fn installed_version(known: &Known<'_>, package: &str) -> Option<String> {
    let file = installed_file(known, package)?;
    let data: Value = serde_json::from_slice(&file.bytes).ok()?;
    data.get("version")?.as_str().map(str::to_owned)
}

pub(super) fn packages(
    files: &[ProjectFile],
    dependencies: &[Dependency],
    options: &[OptionValue],
) -> Packages {
    let mut known = Known::new(files);
    let option = |key: &str| {
        (options.iter())
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    let mut diagnostics = Vec::new();
    // the patterns read the project's manifest: one not read is absent
    let project = match known.files.get("package.json").copied().flatten() {
        Some(file) => match serde_json::from_slice::<Value>(&file.bytes) {
            Ok(value) => value,
            Err(err) => {
                if option("inspect").is_none() && option("check-runtime").is_none() {
                    diagnostics.push(Diagnostic {
                        code: "P005".into(),
                        severity: Severity::Error,
                        message: format!("invalid `package.json`: {err}"),
                        span: None,
                        notes: Vec::new(),
                    });
                }
                Value::Null
            }
        },
        None => Value::Null,
    };

    let root = option("root").unwrap_or_default().to_owned();
    let boundary = option("boundary").filter(|b| !b.is_empty());

    if matches!(option("inspect"), Some("doctor-packages" | "doctor")) {
        let doctor = doctor_package_diagnostics(&mut known, options, boundary);
        return Packages {
            name: project["name"].as_str().map(str::to_owned),
            version: project["version"].as_str().map(str::to_owned),
            declared: Vec::new(),
            sources: Vec::new(),
            runtime: Vec::new(),
            inputs: Vec::new(),
            diagnostics: doctor,
            needs: known.needs.into_iter().collect(),
        };
    }

    if let Some(mode) = option("check-runtime") {
        let compiler = option("compiler").unwrap_or("0.1.0");
        let rt = match mode {
            "rt" => true,
            "effect" => false,
            _ => {
                option("option.runtime") != Some("effect")
                    && option("option.schema-aot") != Some("true")
            }
        };
        let check = check_runtime_diagnostics(&mut known, rt, compiler, boundary);
        return Packages {
            name: project["name"].as_str().map(str::to_owned),
            version: project["version"].as_str().map(str::to_owned),
            declared: Vec::new(),
            sources: Vec::new(),
            runtime: Vec::new(),
            inputs: Vec::new(),
            diagnostics: check,
            needs: known.needs.into_iter().collect(),
        };
    }

    let declared: Vec<String> = names(&project, &["dependencies"]).into_iter().collect();
    let sources = source_roots(&known, &project, &mut diagnostics);
    let effect = option("option.runtime") == Some("effect")
        || option("option.schema-aot") == Some("true")
        || option("runtime") == Some("effect")
        || option("all-runtimes") == Some("true")
        || names(&project, &["dependencies", "optionalDependencies"]).contains(ADAPTER);
    let inputs = build_inputs(&known, &project, effect, boundary);
    let runtime = runtime_graphs(
        &mut known,
        &project,
        &root,
        boundary,
        effect,
        option("packages"),
        option("skip-runtime") == Some("true"),
    );
    let installed = option("verify") == Some("installed");
    diagnostics.extend(wanted::wanted(
        &project,
        dependencies,
        installed.then_some((&mut known, &*root, boundary)),
    ));
    Packages {
        name: project["name"].as_str().map(str::to_owned),
        version: project["version"].as_str().map(str::to_owned),
        declared,
        sources,
        runtime,
        inputs,
        diagnostics,
        needs: known.needs.into_iter().collect(),
    }
}

fn source_roots(
    known: &Known<'_>,
    project: &Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PackageRoot> {
    let mut sources = Vec::new();
    for name in names(project, &["dependencies", "devDependencies"]) {
        let path = format!("node_modules/{name}/package.json");
        // the patterns read every installed package: one not read is absent
        let installed = known.files.get(path.as_str()).copied().flatten();
        let (version, source_root) = installed.map_or_else(Default::default, |file| {
            let data = match serde_json::from_slice::<Value>(&file.bytes) {
                Ok(data) => data,
                Err(err) => {
                    diagnostics.push(Diagnostic {
                        code: "P005".into(),
                        severity: Severity::Error,
                        message: format!("invalid `{path}`: {err}"),
                        span: None,
                        notes: Vec::new(),
                    });
                    Value::Null
                }
            };
            let source = match data["flick"]["source-dir"].as_str() {
                Some(dir) => match safe_subpath(dir) {
                    Some(sub) if sub.is_empty() => Some(format!("node_modules/{name}")),
                    Some(sub) => Some(format!("node_modules/{name}/{sub}")),
                    None => {
                        diagnostics.push(Diagnostic {
                            code: "P005".into(),
                            severity: Severity::Error,
                            message: format!(
                                "invalid `flick.source-dir` `{dir}` in `{path}`: must stay inside the package directory"
                            ),
                            span: None,
                            notes: Vec::new(),
                        });
                        None
                    }
                },
                None => None,
            };
            (text(&data, "version").to_owned(), source)
        });
        if installed.is_none() || source_root.is_some() {
            sources.push(PackageRoot {
                package: name,
                version,
                root: source_root.unwrap_or_default(),
            });
        }
    }
    sources
}

fn build_inputs(
    known: &Known<'_>,
    project: &Value,
    effect: bool,
    boundary: Option<&str>,
) -> Vec<String> {
    let mut inputs: BTreeSet<String> = [
        "package.json".to_owned(),
        "pnpm-lock.yaml".to_owned(),
        "node_modules/@flick-lang/runtime/package.json".to_owned(),
    ]
    .into();
    for name in names(project, &["dependencies", "devDependencies"]) {
        inputs.insert(format!("node_modules/{name}/package.json"));
    }
    if effect {
        inputs.insert(format!("node_modules/{ADAPTER}/package.json"));
        if let Some(adapter) = installed_file(known, ADAPTER) {
            let from = dir(&adapter.real);
            if let Ok(requirements) = adapter_requirements(known) {
                for (dep, _) in requirements {
                    inputs.extend(ancestor_candidates(from, &dep, boundary));
                }
            }
        }
    }
    inputs.into_iter().collect()
}

fn runtime_graphs(
    known: &mut Known<'_>,
    project: &Value,
    root: &str,
    boundary: Option<&str>,
    effect: bool,
    explicit: Option<&str>,
    skip_runtime: bool,
) -> Vec<RuntimeGraph> {
    // `packages` names the graphs to resolve, as a launch rechecks those of its build
    let roots: BTreeSet<String> = if let Some(list) = explicit {
        (list.split(',').filter(|n| !n.is_empty()))
            .map(str::to_owned)
            .collect()
    } else if skip_runtime {
        BTreeSet::new()
    } else {
        let mut roots: BTreeSet<String> = ["@flick-lang/runtime".to_owned()].into();
        if effect {
            roots.insert(ADAPTER.to_owned());
        }
        roots.extend(names(project, &["dependencies", "optionalDependencies"]));
        roots
    };
    let mut runtime = Vec::new();
    for name in roots {
        if let Ok(Some(file)) = resolve(known, root, &name, true, boundary)
            && let Ok(nodes) = graph(known, file, boundary)
        {
            runtime.push(RuntimeGraph {
                package: name,
                nodes,
                skip: vec!["node_modules".into()],
            });
        }
    }
    runtime
}

fn is_drive_root(dir: &str) -> bool {
    let bytes = dir.as_bytes();
    (bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || (bytes.len() == 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/')
}

fn is_unc_root(dir: &str) -> bool {
    let Some(rest) = dir.strip_prefix("//") else {
        return false;
    };
    let mut parts = rest.trim_end_matches('/').split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(s), Some(sh), None) if !s.is_empty() && !sh.is_empty()
    )
}

fn trim_dir(dir: &str) -> &str {
    if dir == "/" {
        "/"
    } else {
        dir.trim_end_matches('/')
    }
}

fn parent_dir(dir: &str) -> Option<&str> {
    let trimmed = trim_dir(dir);
    if trimmed.is_empty() || trimmed == "/" || is_drive_root(trimmed) || is_unc_root(trimmed) {
        return None;
    }
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        Some("/")
    } else {
        Some(parent)
    }
}

fn path_within(path: &str, dir: &str) -> bool {
    let path = trim_dir(path);
    let dir = trim_dir(dir);
    if dir == "/" {
        return path.starts_with('/');
    }
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn walk_ceiling<'a>(from: &'a str, boundary: Option<&'a str>) -> Option<&'a str> {
    let boundary = boundary.map(trim_dir)?;
    if path_within(from, boundary) {
        return Some(boundary);
    }
    if let Some((before, _)) = from.split_once("/node_modules/")
        && !before.is_empty()
    {
        return Some(before);
    }
    Some(trim_dir(from))
}

fn ancestor_candidates(from: &str, package: &str, boundary: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let ceiling = walk_ceiling(from, boundary);
    let mut dir = trim_dir(from);
    while !dir.is_empty() {
        if dir.rsplit('/').next() != Some("node_modules") {
            out.push(format!(
                "{}/node_modules/{package}/package.json",
                dir.trim_end_matches('/')
            ));
        }
        if ceiling.is_some_and(|c| dir == c) {
            break;
        }
        let Some(parent) = parent_dir(dir) else {
            break;
        };
        dir = parent;
    }
    out
}

/// Normalizes a relative `source-dir` inside a package directory; returns
/// `None` if it is absolute, contains backslashes/NULs, or escapes above the
/// package root via `..`.
fn safe_subpath(path: &str) -> Option<String> {
    if path.contains(['\\', '\0']) || path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The manifest of `name` as Node finds it from the directory `from`, a
/// canonical path: in `node_modules` of it or of an ancestor up to `boundary`,
/// never in `node_modules/node_modules`. `project` names the project's own
/// `node_modules` by the relative paths the patterns read.
fn resolve<'a>(
    known: &mut Known<'a>,
    from: &str,
    name: &str,
    project: bool,
    boundary: Option<&str>,
) -> Result<Option<&'a ProjectFile>, ()> {
    // every candidate an unknown one leaves open is asked for at once, so a
    // level of the graph takes one round
    let mut unknown = false;
    let ceiling = walk_ceiling(from, boundary);
    let mut dir = trim_dir(from);
    let mut first = true;
    while !dir.is_empty() {
        let is_modules = dir.rsplit('/').next() == Some("node_modules");
        if !is_modules {
            let found = if first && project {
                // the patterns read the project's own node_modules entirely
                let path = format!("node_modules/{name}/package.json");
                Ok(known.files.get(path.as_str()).copied().flatten())
            } else {
                known.get(&format!(
                    "{}/node_modules/{name}/package.json",
                    dir.trim_end_matches('/')
                ))
            };
            match found {
                Ok(Some(file)) if !unknown => return Ok(Some(file)),
                Ok(_) => {}
                Err(()) => unknown = true,
            }
        }
        first = false;
        if ceiling.is_some_and(|c| dir == c) {
            break;
        }
        let Some(parent) = parent_dir(dir) else {
            break;
        };
        dir = parent;
    }
    if unknown { Err(()) } else { Ok(None) }
}

/// The runtime graph from the manifest `root`, each package once by its
/// canonical path, in the order a depth-first walk first meets them.
fn graph<'a>(
    known: &mut Known<'a>,
    root: &'a ProjectFile,
    boundary: Option<&str>,
) -> Result<Vec<RuntimeNode>, ()> {
    let mut nodes: Vec<Option<RuntimeNode>> = Vec::new();
    let mut seen: BTreeMap<&'a str, u32> = BTreeMap::new();
    visit(known, root, boundary, &mut nodes, &mut seen)?;
    Ok(nodes.into_iter().flatten().collect())
}

fn visit<'a>(
    known: &mut Known<'a>,
    file: &'a ProjectFile,
    boundary: Option<&str>,
    nodes: &mut Vec<Option<RuntimeNode>>,
    seen: &mut BTreeMap<&'a str, u32>,
) -> Result<u32, ()> {
    if let Some(&index) = seen.get(file.real.as_str()) {
        return Ok(index);
    }
    if nodes.len() >= LIMIT {
        return Err(());
    }
    let index = u32::try_from(nodes.len()).map_err(|_| ())?;
    seen.insert(&file.real, index);
    nodes.push(None);
    let data = manifest(file);
    let from = dir(&file.real);
    let mut dependencies = Vec::new();
    let mut missing = false;
    for dependency in names(
        &data,
        &["dependencies", "optionalDependencies", "peerDependencies"],
    ) {
        match resolve(known, from, &dependency, false, boundary) {
            Ok(Some(child)) => {
                let child = visit(known, child, boundary, nodes, seen)?;
                dependencies.push((dependency, child));
            }
            Ok(None) => {}
            Err(()) => missing = true,
        }
    }
    if missing {
        return Err(());
    }
    nodes[index as usize] = Some(RuntimeNode {
        name: text(&data, "name").to_owned(),
        version: text(&data, "version").to_owned(),
        dir: from.to_owned(),
        dependencies,
    });
    Ok(index)
}
