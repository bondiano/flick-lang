//! What a backend's packages give a project: the backend names the files it
//! reads, the host hands them over with their canonical paths, reads the
//! further ones each answer needs, and pins the contents of the runtime
//! graphs the backend resolves. The host knows nothing of the ecosystem: it
//! matches paths, reads files and hashes directories.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_backend::command::Dependency;
use flick_backend::{
    Backend, Diagnostic, OptionValue, PackageRoot, ProjectFile, RuntimeDependency, RuntimeGraph,
    sha256,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// How many times one report may ask for more files: a level of a runtime
/// graph takes one.
const ROUNDS: usize = 128;

/// The backend's packages, with each runtime graph pinned.
pub(crate) struct Report {
    pub(crate) name: Option<String>,
    pub(crate) sources: Vec<PackageRoot>,
    pub(crate) runtime: Vec<RuntimeDependency>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Asks `backend` for the packages of the project at `root`.
pub(crate) fn report(
    backend: &dyn Backend,
    root: &Path,
    dependencies: &[Dependency],
    options: &[OptionValue],
) -> Result<Report, String> {
    let patterns = backend.package_files();
    if patterns.is_empty() {
        return Ok(Report {
            name: None,
            sources: Vec::new(),
            runtime: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    // what a backend may ask for later: files of the names its patterns end in
    let names: BTreeSet<&str> = patterns
        .iter()
        .filter_map(|p| p.rsplit('/').next())
        .collect();
    let mut found = Vec::new();
    for pattern in &patterns {
        matching(root, Path::new(""), pattern, &mut found);
    }
    let mut files: Vec<ProjectFile> = (found.into_iter())
        .map(|path| read(&root.join(&path), slashed(&path)))
        .filter(|f| !f.real.is_empty())
        .collect();
    for _ in 0..ROUNDS {
        let answer = backend.packages(&files, dependencies, options);
        if answer.needs.is_empty() {
            let runtime = answer.runtime.iter().map(pin).collect::<Result<_, _>>()?;
            return Ok(Report {
                name: answer.name,
                sources: answer.sources,
                runtime,
                diagnostics: answer.diagnostics,
            });
        }
        for need in answer.needs {
            if files.iter().any(|f| f.path == need) {
                return Err(format!("the backend asked again for `{need}`"));
            }
            let name = need.rsplit('/').next().unwrap_or_default();
            if !names.contains(name) {
                return Err(format!(
                    "the backend asked for `{need}`, which none of its patterns names"
                ));
            }
            files.push(read(&root.join(&need), need));
        }
    }
    Err(format!("the backend asked for more files {ROUNDS} times"))
}

/// Whether any file under `root` matches one of `patterns`.
pub(crate) fn any_match(root: &Path, patterns: &[String]) -> bool {
    let mut files = Vec::new();
    patterns.iter().any(|p| {
        matching(root, Path::new(""), p, &mut files);
        !files.is_empty()
    })
}

/// The files under `root` that `patterns` name, relative to it, and those a
/// pattern without `*` names though absent: creating one matters too.
pub(crate) fn named(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for pattern in patterns {
        if pattern.contains('*') {
            matching(root, Path::new(""), pattern, &mut files);
        } else {
            files.push(PathBuf::from(pattern));
        }
    }
    files
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The file at `path`, asked for as `asked`; an absent one has no path.
fn read(path: &Path, asked: String) -> ProjectFile {
    match (fs::read(path), fs::canonicalize(path)) {
        (Ok(bytes), Ok(real)) => ProjectFile {
            path: asked,
            real: real.to_string_lossy().replace('\\', "/"),
            bytes,
        },
        _ => ProjectFile {
            path: asked,
            real: String::new(),
            bytes: Vec::new(),
        },
    }
}

/// The files under `root` that `pattern` names, relative to it, `*` standing
/// for any one name, part of a name with a prefix or suffix, as `@*`.
fn matching(root: &Path, at: &Path, pattern: &str, out: &mut Vec<PathBuf>) {
    let (part, rest) = pattern
        .split_once('/')
        .map_or((pattern, None), |(p, r)| (p, Some(r)));
    let names: Vec<String> = match part.split_once('*') {
        None => vec![part.to_owned()],
        Some((prefix, suffix)) => (fs::read_dir(root.join(at)).into_iter().flatten().flatten())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                n.len() >= prefix.len() + suffix.len()
                    && n.starts_with(prefix)
                    && n.ends_with(suffix)
            })
            .collect(),
    };
    let mut names = names;
    names.sort();
    for name in names {
        let path = at.join(&name);
        if let Some(rest) = rest {
            matching(root, &path, rest, out);
        } else if root.join(&path).is_file() {
            out.push(path);
        }
    }
}

#[derive(Serialize)]
struct Node {
    name: String,
    version: String,
    content: String,
    dependencies: Vec<(String, usize)>,
}

/// The pin of a runtime graph: its packages' contents and edges, never
/// their paths.
fn pin(graph: &RuntimeGraph) -> Result<RuntimeDependency, String> {
    let mut nodes = Vec::new();
    for node in &graph.nodes {
        nodes.push(Node {
            name: node.name.clone(),
            version: node.version.clone(),
            content: content(Path::new(&node.dir), &graph.skip)?,
            dependencies: (node.dependencies.iter())
                .map(|(name, index)| (name.clone(), *index as usize))
                .collect(),
        });
    }
    let version = (nodes.first())
        .map(|n| n.version.clone())
        .ok_or("an empty runtime graph")?;
    let mut bytes = Vec::new();
    ciborium::into_writer(&nodes, &mut bytes).map_err(|e| e.to_string())?;
    Ok(RuntimeDependency {
        package: graph.package.clone(),
        version,
        sha256: sha256(&bytes),
    })
}

/// The hash of the files of the package at `dir`, outside the directories
/// `skip` names and `.git`, by their relative names and sizes.
pub(crate) fn content(dir: &Path, skip: &[String]) -> Result<String, String> {
    let mut files = Vec::new();
    collect(dir, Path::new(""), skip, &mut files)
        .map_err(|e| format!("cannot pin the installed runtime: {e}"))?;
    files.sort();
    let mut hash = Sha256::new();
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let path = dir.join(&relative);
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        hash.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(size.to_le_bytes());
        io::copy(
            &mut fs::File::open(path).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn collect(
    package: &Path,
    relative: &Path,
    skip: &[String],
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(package.join(relative))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if name == ".git" || skip.iter().any(|s| name == s.as_str()) {
            continue;
        }
        let path = relative.join(name);
        if kind.is_dir() {
            collect(package, &path, skip, files)?;
        } else if kind.is_file() {
            files.push(path);
        } else {
            return Err(io::Error::other("runtime contains a nonregular file"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::{env, fs, process};

    use flick_backend::host::{Budget, WasmBackend};
    use flick_backend::{BackendPin, OptionValue};

    use super::report;
    use crate::backends::dependencies::installed_for;
    use crate::component;

    /// The JavaScript component of this checkout.
    pub(crate) fn js() -> Arc<WasmBackend> {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../backends/js/package");
        let pin: BackendPin =
            serde_json::from_slice(&fs::read(package.join("backend.json")).expect("pin"))
                .expect("JSON");
        let bytes = fs::read(package.join("component.wasm")).expect("component");
        let cancel = component::cancellation().expect("a cancellation").clone();
        let backend = WasmBackend::load_with_cancellation(
            component::engine().expect("an engine"),
            pin,
            &bytes,
            Budget::default(),
            cancel,
        );
        Arc::new(backend.expect("the JS component loads"))
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("dir")).expect("dir");
        fs::write(path, text).expect("write");
    }

    /// The JS backend resolves the runtime graphs the host pinned before,
    /// so every pin, and the build cache it keys, stays the same.
    #[test]
    fn the_backend_pins_the_runtime_as_the_host_did() {
        let root = env::temp_dir().join(format!("flick-packages-pins-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let modules = root.join("node_modules");
        write(
            &root.join("package.json"),
            r#"{"name":"app","dependencies":{"left":"1"},"devDependencies":{"lib":"1"}}"#,
        );
        write(
            &modules.join("@flick-lang/runtime/package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        );
        write(
            &modules.join("@flick-lang/runtime/dist/index.js"),
            "runtime",
        );
        write(
            &modules.join("@flick-lang/effect-ts/package.json"),
            r#"{"name":"@flick-lang/effect-ts","version":"0.1.0","dependencies":{"effect":"4"}}"#,
        );
        write(
            &modules.join("@flick-lang/effect-ts/node_modules/effect/package.json"),
            r#"{"name":"effect","version":"4.0.0","dependencies":{"left":"1"}}"#,
        );
        write(
            &modules.join("@flick-lang/effect-ts/node_modules/effect/dist/a.js"),
            "effect",
        );
        write(
            &modules.join("left/package.json"),
            r#"{"name":"left","version":"1.0.0","peerDependencies":{"effect":"4"}}"#,
        );
        write(
            &modules.join("lib/package.json"),
            r#"{"name":"lib","version":"1.2.0","flick":{"source-dir":"src"}}"#,
        );
        let real = fs::canonicalize(&root)
            .expect("root")
            .to_string_lossy()
            .into_owned();
        let backend = js();
        for effect in [false, true] {
            let options = [
                OptionValue {
                    key: "runtime".into(),
                    value: if effect { "effect" } else { "rt" }.into(),
                },
                OptionValue {
                    key: "root".into(),
                    value: real.clone(),
                },
            ];
            let reported = report(&*backend, &root, &[], &options).expect("a report");
            let mut pinned = reported.runtime;
            pinned.sort_by(|a, b| a.package.cmp(&b.package));
            let mut before = installed_for(&root, effect).expect("the host's pins");
            before.sort_by(|a, b| a.package.cmp(&b.package));
            assert_eq!(pinned, before, "effect {effect}");
            let lib = (reported.sources.iter())
                .find(|s| s.package == "lib")
                .expect("the Flick library");
            assert_eq!(
                (lib.version.as_str(), lib.root.as_str()),
                ("1.2.0", "node_modules/lib/src")
            );
        }
        fs::remove_dir_all(root).expect("cleanup");
    }
}
