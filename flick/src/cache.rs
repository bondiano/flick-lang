//! The build cache in `.flick/cache/`: a build's artifacts by a key over
//! everything that decides them (sources, effective configuration, profile
//! and emit settings, compiler and runtime versions, the lockfile and the
//! interfaces of installed dependencies), and the manifest of what each
//! build wrote, which `flick clean` follows.

use std::path::{Path, PathBuf};
use std::{env, fs};

use flick_core::build::Artifact;
use flick_core::project::Project;
use flick_core::project::cache::{self, Key, Manifest};

use crate::toolchain;

/// Files under `dir`, recursively, in a stable order.
pub fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            files_under(&entry, out);
        } else {
            out.push(entry);
        }
    }
}

/// The key of a build of `project` with `settings`, a description of the
/// command-line settings that change its output.
pub fn key(project: &Project, roots: &[PathBuf], settings: &str) -> String {
    let mut key = Key::default();
    key.add("settings", settings);
    key.add("config", format!("{:?}", project.config));
    key.add("compiler", toolchain::COMPILER);
    key.add("language", flick_core::LANGUAGE_VERSION);
    // a compiler rebuilt without a new version still changes the key
    if let Ok(meta) = env::current_exe().and_then(fs::metadata) {
        key.add(
            "compiler-binary",
            format!("{} {:?}", meta.len(), meta.modified().ok()),
        );
    }
    for package in ["@flick-lang/runtime", "effect"] {
        let version = toolchain::resolved(&project.root, package).unwrap_or_default();
        key.add(format!("version {package}"), version);
    }
    let mut sources = Vec::new();
    for root in [project.source_root(), project.test_root()] {
        files_under(&root, &mut sources);
    }
    sources.extend(roots.iter().cloned());
    sources.sort();
    sources.dedup();
    for path in sources
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "fl"))
    {
        key.add(
            format!("source {}", path.display()),
            fs::read(path).unwrap_or_default(),
        );
    }
    for file in ["package.json", "pnpm-lock.yaml"] {
        key.add(file, fs::read(project.root.join(file)).unwrap_or_default());
    }
    for (name, bytes) in dependency_files(&project.root) {
        key.add(name, bytes);
    }
    key.finish()
}

/// `package.json` of each installed direct dependency and the files of its
/// Flick interfaces.
fn dependency_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for field in ["dependencies", "devDependencies"] {
        let Some(deps) = manifest.get(field).and_then(|d| d.as_object()) else {
            continue;
        };
        for name in deps.keys() {
            let dir = root.join("node_modules").join(name);
            let Ok(package) = fs::read(dir.join("package.json")) else {
                continue;
            };
            let iface_dir = serde_json::from_slice::<serde_json::Value>(&package)
                .ok()
                .and_then(|p| Some(p.get("flick")?.get("iface-dir")?.as_str()?.to_owned()));
            out.push((format!("dependency {name}"), package));
            if let Some(iface_dir) = iface_dir {
                let mut files = Vec::new();
                files_under(&dir.join(iface_dir), &mut files);
                for f in files {
                    out.push((
                        format!("interface {}", f.display()),
                        fs::read(&f).unwrap_or_default(),
                    ));
                }
            }
        }
    }
    out
}

fn entry_path(root: &Path, key: &str) -> PathBuf {
    root.join(cache::DIR)
        .join("cache")
        .join(format!("{key}.json"))
}

/// The artifacts cached under `key`.
pub fn load(root: &Path, key: &str) -> Option<Vec<Artifact>> {
    let text = fs::read_to_string(entry_path(root, key)).ok()?;
    let pairs: Vec<(String, String)> = serde_json::from_str(&text).ok()?;
    Some(
        pairs
            .into_iter()
            .map(|(path, contents)| Artifact { path, contents })
            .collect(),
    )
}

/// Caches `artifacts` under `key`; a cache that cannot be written is only
/// slower.
pub fn store(root: &Path, key: &str, artifacts: &[Artifact]) {
    let pairs: Vec<(&str, &str)> = artifacts
        .iter()
        .map(|a| (a.path.as_str(), a.contents.as_str()))
        .collect();
    let path = entry_path(root, key);
    if let Some(dir) = path.parent()
        && fs::create_dir_all(dir).is_ok()
        && let Ok(json) = serde_json::to_string(&pairs)
    {
        let _ = fs::write(path, json);
    }
}

/// Records the files a build wrote into `out_dir`.
pub fn record(root: &Path, out_dir: &Path, artifacts: &[Artifact]) {
    let path = root.join(cache::MANIFEST);
    let mut manifest: Manifest = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let dir = out_dir.strip_prefix(root).unwrap_or(out_dir);
    manifest.record(
        &dir.to_string_lossy(),
        artifacts.iter().map(|a| a.path.clone()).collect(),
    );
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_ok()
        && let Ok(json) = serde_json::to_string_pretty(&manifest)
    {
        let _ = fs::write(path, json);
    }
}

/// The manifest of the project.
pub fn manifest(root: &Path) -> Manifest {
    fs::read_to_string(root.join(cache::MANIFEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}
