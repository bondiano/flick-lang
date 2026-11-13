//! The build cache in `.flick/cache/`: a build's artifacts by a key over
//! everything that decides them (sources, effective configuration, profile
//! and emit settings, compiler and runtime versions, the lockfile and the
//! interfaces and sources of dependencies), and the manifest of what each
//! build wrote, which `flick clean` follows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

use flick_core::plugin::reach::{SNAPSHOTS, hash};
use flick_core::project::cache::{self, Key, Manifest};
use flick_core::project::{self, Project, config};
use flick_core::source::SourceMap;

use crate::artifact::{self, Artifact};
use crate::builtins;
use crate::commands::inputs::{declared_name, sibling_root};
use crate::saved;
use crate::toolchain;
use flick_backend::Limits;

mod signed;

pub use signed::{Signed, missed, path as inputs_path};

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
pub fn key(project: &Project, roots: &[PathBuf], settings: &str, needs_effect: bool) -> Signed {
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
    key.add("implicit packages", builtins::implicit_hash());
    key.add(
        "compiler-binary",
        toolchain::fingerprint().unwrap_or_default(),
    );
    let packages = ["@flick-lang/runtime"]
        .into_iter()
        .chain(needs_effect.then_some("@flick-lang/effect-ts"))
        .chain(needs_effect.then_some("effect"));
    for package in packages {
        let version = toolchain::resolved(&project.root, package).unwrap_or_default();
        key.add(format!("version {package}"), version);
    }
    if needs_effect {
        // An absent contract is valid; unreadable or empty manifests are not.
        if let Err(error) = toolchain::adapter_requirements(&project.root) {
            key.add("invalid adapter contract", error);
        }
        for path in toolchain::contract_inputs(&project.root) {
            key.add(
                format!("contract {}", path.display()),
                fs::read(path).unwrap_or_default(),
            );
        }
    }
    // a variable a plugin may read, by its SHA-256: the key's hash is weak
    for name in project.config.plugins.values().flat_map(|g| &g.env) {
        let value = env::var(name).unwrap_or_default();
        key.add(format!("env {name}"), hash([value.as_bytes()]));
    }
    for path in input_files(project, &selected) {
        key.add(
            format!("input {}", path.display()),
            fs::read(path).unwrap_or_default(),
        );
    }
    key.into()
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
    // a module outside the project's roots imports its siblings, which the loader finds
    let project_roots = [project.source_root(), project.test_root()];
    for root in roots
        .iter()
        .filter(|r| !project_roots.iter().any(|p| r.starts_with(p)))
    {
        if let Some(name) = fs::read_to_string(root)
            .ok()
            .as_deref()
            .and_then(declared_name)
        {
            files_under(&sibling_root(root, &name), &mut files);
        }
    }
    // the files of other languages the plugins of the libraries load are modules too
    let (mut libraries, mut loaded) = (Vec::new(), Vec::new());
    library_files(
        &project.root,
        &project.config.dependencies,
        &mut libraries,
        &mut loaded,
    );
    files.retain(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e, "fl" | "go" | "mjs") || loaded.iter().any(|l| l == e))
    });
    files.extend(
        [
            project::CONFIG_FILE,
            project::lock::LOCK_FILE,
            "package.json",
            "pnpm-lock.yaml",
        ]
        .map(|file| project.root.join(file)),
    );
    files.extend(project.workspace.clone());
    files.extend(dependency_files(&project.root));
    files.extend(libraries);
    // what plugins read: the files the project grants them, and snapshots
    files_under(&project.root.join(SNAPSHOTS), &mut files);
    let granted = project.config.plugins.values().flat_map(|g| &g.fs_read);
    for path in granted.map(|p| project.root.join(p)) {
        if path.is_dir() {
            files_under(&path, &mut files);
        } else {
            files.push(path);
        }
    }
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
            let flick = serde_json::from_slice::<serde_json::Value>(&package)
                .ok()
                .and_then(|p| p.get("flick").cloned());
            let dir_of = |key: &str| Some(flick.as_ref()?.get(key)?.as_str()?.to_owned());
            // a source package is compiled with the project: its modules are inputs
            if let Some(source_dir) = dir_of("source-dir") {
                let mut sources = Vec::new();
                files_under(&dir.join(source_dir), &mut sources);
                out.extend(
                    sources
                        .into_iter()
                        .filter(|p| p.extension().is_some_and(|e| e == "fl" || e == "go")),
                );
            }
        }
    }
    out
}

/// The configuration and modules of the libraries `dependencies` of the
/// manifest in `dir` name by path, and of theirs, and into `loaded` the
/// extensions of the files their plugins load; a fetched library never
/// changes but with `flick.lock`.
// shortcut: a fetched library's `load` is not read, so a file it loads changes
// no build until `flick.lock` does; read fetched manifests when one loads files
fn library_files(
    dir: &Path,
    dependencies: &[config::Dependency],
    out: &mut Vec<PathBuf>,
    loaded: &mut Vec<String>,
) {
    for dependency in dependencies {
        let Some(path) = dependency.path() else {
            continue;
        };
        let at = project::normalize(&dir.join(path));
        let file = at.join(project::CONFIG_FILE);
        if out.contains(&file) {
            continue;
        }
        out.push(file.clone());
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let mut sources = SourceMap::default();
        let id = sources.add(file.display().to_string(), text.clone());
        let Ok(library) = config::parse(&text, id) else {
            continue;
        };
        // a plugin's component expands blocks, so a new build of it is an input
        let component = library.package.as_ref().and_then(|p| p.plugin.as_ref());
        out.extend(component.map(|c| at.join(c)));
        loaded.extend(library.package.iter().flat_map(|p| p.load.iter().cloned()));
        // the foreign files a build ships or compiles are inputs as its modules are
        let mut modules = Vec::new();
        files_under(&at.join(&library.source_dir), &mut modules);
        out.extend(modules.into_iter().filter(|p| {
            p.extension()
                .is_some_and(|e| e == "fl" || e == "go" || e == "mjs" || e == "cjs")
        }));
        library_files(&at, &library.dependencies, out, loaded);
    }
}

fn entry_path(root: &Path, key: &str) -> PathBuf {
    root.join(cache::DIR)
        .join("cache")
        .join(format!("{key}.cbor"))
}

/// The artifacts cached under `key`.
pub fn load(root: &Path, key: &str) -> Option<Vec<Artifact>> {
    let path = entry_path(root, key);
    if fs::metadata(&path).ok()?.len() > Limits::default().result_bytes as u64 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let files: Vec<Artifact> = ciborium::from_reader(bytes.as_slice()).ok()?;
    artifact::validate(&files).ok()?;
    Some(files)
}

/// How many files of builds a project keeps: each state of its inputs
/// leaves a whole copy of what it built, and a Go build its stamp beside it.
const KEPT: usize = 16;

/// Caches `artifacts` under `key` when the build read the inputs the key is
/// of: `before` is the key from before it ran, and `read` the roots, the
/// working directory and the sources it compiled. Says whether it did.
pub fn store_built(
    project: &Project,
    read: (&[PathBuf], &Path, &SourceMap),
    before: &Signed,
    key: &Signed,
    artifacts: &[Artifact],
) -> bool {
    let (roots, cwd, sources) = read;
    let current = before == key && read_unchanged(project, roots, cwd, sources);
    if current {
        store(&project.root, key, artifacts);
    }
    current
}

/// Caches `artifacts` under `key`; a cache that cannot be written is only
/// slower.
pub fn store(root: &Path, key: &Signed, artifacts: &[Artifact]) {
    if artifact::validate(artifacts).is_err() {
        return;
    }
    let path = entry_path(root, key);
    let mut bytes = Vec::new();
    if ciborium::into_writer(artifacts, &mut bytes).is_ok() && saved::write(&path, &bytes).is_ok() {
        signed::save(root, key);
        if let Some(dir) = path.parent() {
            saved::keep_newest(dir, KEPT);
        }
    }
}

/// Whether all cached files still have the bytes we published.
pub fn artifacts_current(out_dir: &Path, artifacts: &[Artifact]) -> bool {
    artifacts
        .iter()
        .all(|artifact| artifact.current(&out_dir.join(&artifact.path)))
}

/// A Go executable is reusable only with the same toolchain and unchanged bytes.
fn go_stamp(out_dir: &Path, executable: &str) -> Option<String> {
    let mut command = Command::new("go");
    command
        .args([
            "env",
            "GOVERSION",
            "GOOS",
            "GOARCH",
            "GOAMD64",
            "GOARM",
            "GO386",
            "GOEXPERIMENT",
            "CGO_ENABLED",
        ])
        .env("GOFLAGS", "-mod=mod")
        .env("GOTOOLCHAIN", "local");
    if env::var_os("CGO_ENABLED").is_none() {
        command.env("CGO_ENABLED", "0");
    }
    let version = command.output().ok()?;
    if !version.status.success() {
        return None;
    }
    let mut key = Key::default();
    key.add("toolchain", version.stdout);
    key.add("binary", fs::read(out_dir.join(executable)).ok()?);
    Some(key.finish())
}

fn go_stamp_path(root: &Path, key: &str, executable: &str) -> PathBuf {
    root.join(cache::DIR)
        .join("cache")
        .join(format!("{key}-{executable}.go-stamp"))
}

pub fn go_current(root: &Path, key: &str, out_dir: &Path, executable: &str) -> bool {
    let path = go_stamp_path(root, key, executable);
    fs::read_to_string(path)
        .ok()
        .zip(go_stamp(out_dir, executable))
        .is_some_and(|(saved, current)| saved == current)
}

pub fn mark_go(root: &Path, key: &str, out_dir: &Path, executable: &str) {
    let Some(stamp) = go_stamp(out_dir, executable) else {
        return;
    };
    let path = go_stamp_path(root, key, executable);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, stamp);
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

pub fn outputs_current(root: &Path, out_dir: &Path, files: &[String]) -> bool {
    let dir = out_dir
        .strip_prefix(root)
        .unwrap_or(out_dir)
        .to_string_lossy();
    manifest(root)
        .outputs
        .iter()
        .any(|(output, recorded)| output == &dir && recorded == files)
}

#[cfg(test)]
mod tests;
