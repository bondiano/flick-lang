//! The build cache in `.flick/cache/`: a build's artifacts by a key over
//! everything that decides them (sources, effective configuration, profile
//! and emit settings, compiler and runtime versions, the lockfile and the
//! interfaces of installed dependencies), and the manifest of what each
//! build wrote, which `flick clean` follows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use flick_core::build::Artifact;
use flick_core::project::cache::{self, Key, Manifest};
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

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
    let mut selected: Vec<PathBuf> = roots.iter().map(|p| project::normalize(p)).collect();
    selected.sort();
    selected.dedup();
    for path in &selected {
        key.add("selected-root", path.to_string_lossy().as_bytes());
    }
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
    for path in input_files(project, &selected) {
        key.add(
            format!("input {}", path.display()),
            fs::read(path).unwrap_or_default(),
        );
    }
    key.finish()
}

/// Whether each input the compiler read still has the bytes it compiled.
/// The key is computed from the disk after the build, so a file saved
/// meanwhile would file the artifacts under sources they were not built
/// from; such a build is not cached. Display paths are relative to `cwd`.
pub fn read_unchanged(
    project: &Project,
    roots: &[PathBuf],
    cwd: &Path,
    sources: &SourceMap,
) -> bool {
    let inputs: HashSet<PathBuf> = input_files(project, roots).into_iter().collect();
    sources.files().all(|file| {
        let path = project::normalize(&cwd.join(&file.path));
        !inputs.contains(&path) || fs::read(&path).is_ok_and(|bytes| bytes == file.text.as_bytes())
    })
}

/// Files that affect a build, including absent manifests whose creation matters.
pub fn input_files(project: &Project, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in [project.source_root(), project.test_root()] {
        files_under(&root, &mut files);
    }
    files.extend(roots.iter().cloned());
    files.retain(|p| p.extension().is_some_and(|e| e == "fl"));
    files.extend(
        [project::CONFIG_FILE, "package.json", "pnpm-lock.yaml"]
            .map(|file| project.root.join(file)),
    );
    files.extend(dependency_files(&project.root));
    for path in &mut files {
        *path = project::normalize(path);
    }
    files.sort();
    files.dedup();
    files
}

/// Manifests of direct dependencies and their installed Flick interfaces.
fn dependency_files(root: &Path) -> Vec<PathBuf> {
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
            out.push(dir.join("package.json"));
            let Ok(package) = fs::read(dir.join("package.json")) else {
                continue;
            };
            let iface_dir = serde_json::from_slice::<serde_json::Value>(&package)
                .ok()
                .and_then(|p| Some(p.get("flick")?.get("iface-dir")?.as_str()?.to_owned()));
            if let Some(iface_dir) = iface_dir {
                files_under(&dir.join(iface_dir), &mut out);
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

/// Removes obsolete build-owned files and records a successful publication.
pub fn record(root: &Path, out_dir: &Path, files: Vec<String>) -> io::Result<()> {
    let path = root.join(cache::MANIFEST);
    let mut manifest = manifest(root);
    let dir = out_dir.strip_prefix(root).unwrap_or(out_dir);
    let dir = dir.to_string_lossy();
    let current: HashSet<&str> = files.iter().map(String::as_str).collect();
    if let Some((_, previous)) = manifest.outputs.iter().find(|(output, _)| output == &dir) {
        for stale in previous
            .iter()
            .filter(|file| !current.contains(file.as_str()))
        {
            let path = out_dir.join(stale);
            match fs::remove_file(&path) {
                Ok(()) => remove_empty_parents(&path, out_dir),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
    }
    manifest.record(&dir, files);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(path, json)
}

/// Removes the directories of `path` up to `top` that are left empty.
pub fn remove_empty_parents(path: &Path, top: &Path) {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == top || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// The manifest of the project.
pub fn manifest(root: &Path) -> Manifest {
    fs::read_to_string(root.join(cache::MANIFEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use flick_core::project::{self, discover};
    use flick_core::source::SourceMap;

    use super::read_unchanged;
    use crate::sources::FsHost;

    #[test]
    fn a_source_saved_after_it_was_read_keeps_the_build_out_of_the_cache() {
        let root = std::env::temp_dir().join(format!("flick-cache-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/App")).expect("dir");
        fs::write(
            root.join("flick.toml"),
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
        )
        .expect("write");
        let helper = root.join("src/App/Helper.fl");
        fs::write(
            &helper,
            "module App.Helper exposing (n)\n\nn : Number\nlet n = 1\n",
        )
        .expect("write");
        fs::write(
            root.join("src/App/Main.fl"),
            "module App.Main exposing (main)\n\nimport App.Helper\n\nmain : Unit -> Unit\nlet main () =\n  let _ = App.Helper.n\n  ()\n",
        )
        .expect("write");
        let host = FsHost::new();
        let mut sources = SourceMap::default();
        let project = discover(&host, &mut sources, &root, None)
            .expect("project")
            .expect("found");
        let roots = [root.join("src/App/Main.fl")];
        let (diagnostics, _) = project::compile(&host, &mut sources, Some(&project), &roots);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(sources.files().any(|f| f.path.ends_with("Helper.fl")));
        assert!(read_unchanged(&project, &roots, &host.cwd, &sources));
        // the editor saves the helper after the compiler read it
        fs::write(
            &helper,
            "module App.Helper exposing (n)\n\nn : Number\nlet n = 2\n",
        )
        .expect("write");
        assert!(!read_unchanged(&project, &roots, &host.cwd, &sources));
        let _ = fs::remove_dir_all(&root);
    }
}
