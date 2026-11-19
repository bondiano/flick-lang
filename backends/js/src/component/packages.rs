//! The packages of a JavaScript project, read from the `package.json` files
//! the host hands over: the Flick libraries npm installed, which name their
//! sources with `"flick": { "source-dir" }`, and the graphs of the runtime
//! packages the generated code loads, resolved as Node resolves them. A
//! `package.json` the host has not read yet is asked for, by its canonical
//! path, in `needs`.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::command::Dependency;
use flick_backend::{
    Diagnostic, OptionValue, PackageRoot, Packages, ProjectFile, RuntimeGraph, RuntimeNode,
    Severity, Span,
};
use serde_json::Value;

/// The package `effect` mode loads beside the runtime.
const ADAPTER: &str = "@flick-lang/effect-ts";
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
    // the patterns read the project's manifest: one not read is absent
    let project =
        (known.files.get("package.json").copied().flatten()).map_or(Value::Null, manifest);
    let mut sources = Vec::new();
    for name in names(&project, &["dependencies", "devDependencies"]) {
        let path = format!("node_modules/{name}/package.json");
        // the patterns read every installed package: one not read is absent
        let installed = known.files.get(path.as_str()).copied().flatten();
        let (version, root) = installed.map_or_else(Default::default, |file| {
            let data = manifest(file);
            let source = data["flick"]["source-dir"]
                .as_str()
                .map(|dir| normal(&format!("node_modules/{name}/{dir}")));
            (text(&data, "version").to_owned(), source)
        });
        if installed.is_none() || root.is_some() {
            sources.push(PackageRoot {
                package: name,
                version,
                root: root.unwrap_or_default(),
            });
        }
    }
    let root = option("root").unwrap_or_default().to_owned();
    // `packages` names the graphs to resolve, as a launch rechecks those of its build
    let roots: BTreeSet<String> = if let Some(list) = option("packages") {
        (list.split(',').filter(|n| !n.is_empty()))
            .map(str::to_owned)
            .collect()
    } else {
        let mut roots: BTreeSet<String> = ["@flick-lang/runtime".to_owned()].into();
        if option("runtime") == Some("effect") {
            roots.insert(ADAPTER.to_owned());
        }
        roots.extend(names(&project, &["dependencies", "optionalDependencies"]));
        roots
    };
    let mut runtime = Vec::new();
    for name in roots {
        if let Ok(Some(file)) = resolve(&mut known, &root, &name, true)
            && let Ok(nodes) = graph(&mut known, file)
        {
            runtime.push(RuntimeGraph {
                package: name,
                nodes,
                skip: vec!["node_modules".into()],
            });
        }
    }
    let installed = option("verify") == Some("installed");
    let diagnostics = wanted(
        &project,
        dependencies,
        installed.then_some((&mut known, &*root)),
    );
    Packages {
        name: project["name"].as_str().map(str::to_owned),
        sources,
        runtime,
        diagnostics,
        needs: known.needs.into_iter().collect(),
    }
}

/// What is wrong with the npm packages `dependencies` ask for: a name or a
/// version that is no package of the registry, a version `package.json`
/// names otherwise and, with `known`, one that is not installed. What a
/// fetched library asks for reaches the project's `package.json`, so it
/// names a package of the registry: no path, no URL, no protocol.
fn wanted(
    project: &Value,
    dependencies: &[Dependency],
    mut installed: Option<(&mut Known, &str)>,
) -> Vec<Diagnostic> {
    let declared = project["dependencies"].as_object();
    let mut diagnostics = Vec::new();
    for (i, d) in dependencies.iter().enumerate() {
        let error = |code: &str, message: String| Diagnostic {
            code: code.into(),
            severity: Severity::Error,
            message,
            span: u32::try_from(i).ok().map(|file| Span {
                file,
                start: 0,
                end: 0,
            }),
            notes: Vec::new(),
        };
        let asker =
            d.by.as_ref()
                .map_or_else(|| "the project".to_owned(), |by| format!("`{by}`"));
        if !is_npm_name(&d.name) {
            let message = format!(
                "`[target.js.dependencies]`: `{}` is not the name of an npm package",
                d.name
            );
            diagnostics.push(error("P032", message));
            continue;
        }
        if !is_registry_version(&d.version) {
            let message = format!(
                "`[target.js.dependencies]`: `{} = \"{}\"` is not a version or range of the registry",
                d.name, d.version
            );
            diagnostics.push(error("P032", message));
            continue;
        }
        let written = declared.and_then(|m| m.get(&d.name));
        if let Some(version) = written.and_then(Value::as_str)
            && version != d.version
        {
            let mut conflict = error(
                "P040",
                format!(
                    "`package.json` names the npm package `{}` as `{version}`, and {asker} needs `{}`",
                    d.name, d.version
                ),
            );
            conflict.notes.push(
                "make them agree on one version, or use versions of the packages that do".into(),
            );
            diagnostics.push(conflict);
            continue;
        }
        // as Node finds it from the root; one not known yet is asked for
        let unmet = installed.as_mut().is_some_and(|(known, root)| {
            written.is_none() || matches!(resolve(known, root, &d.name, true), Ok(None))
        });
        if unmet {
            diagnostics.push(error(
                "P034",
                format!(
                    "{asker} needs the npm package `{}`, which is not installed: run `flick install`",
                    d.name
                ),
            ));
        }
    }
    diagnostics
}

/// `name` or `@scope/name`, which cannot leave `node_modules`.
pub(super) fn is_npm_name(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    };
    match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| part(scope) && part(name)),
        None => part(name),
    }
}

pub(super) fn is_registry_version(version: &str) -> bool {
    !version.trim().is_empty() && !version.contains([':', '/', '\\'])
}

/// `path` without `.` parts and with each `..` taking off the part before.
fn normal(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The manifest of `name` as Node finds it from the directory `from`, a
/// canonical path: in `node_modules` of it or of an ancestor, but never in
/// `node_modules/node_modules`. `project` names the project's own
/// `node_modules` by the relative paths the patterns read.
fn resolve<'a>(
    known: &mut Known<'a>,
    from: &str,
    name: &str,
    project: bool,
) -> Result<Option<&'a ProjectFile>, ()> {
    // every candidate an unknown one leaves open is asked for at once, so a
    // level of the graph takes one round
    let mut unknown = false;
    let mut dir = from;
    let mut first = true;
    loop {
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
        match dir.rsplit_once('/') {
            Some((parent, _)) if !parent.is_empty() => dir = parent,
            Some(_) if dir != "/" => dir = "/",
            _ => return if unknown { Err(()) } else { Ok(None) },
        }
    }
}

/// The runtime graph from the manifest `root`, each package once by its
/// canonical path, in the order a depth-first walk first meets them.
fn graph<'a>(known: &mut Known<'a>, root: &'a ProjectFile) -> Result<Vec<RuntimeNode>, ()> {
    let mut nodes: Vec<Option<RuntimeNode>> = Vec::new();
    let mut seen: BTreeMap<&'a str, u32> = BTreeMap::new();
    visit(known, root, &mut nodes, &mut seen)?;
    Ok(nodes.into_iter().flatten().collect())
}

fn visit<'a>(
    known: &mut Known<'a>,
    file: &'a ProjectFile,
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
        match resolve(known, from, &dependency, false) {
            Ok(Some(child)) => {
                let child = visit(known, child, nodes, seen)?;
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
