//! Content pins for installed runtime roots and their actual dependency graphs.

mod graph;
use crate::toolchain;
use flick_backend::RuntimeDependency;
use graph::Graph;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(super) fn installed(root: &Path) -> Result<Vec<RuntimeDependency>, String> {
    Pins::default().installed(root)
}

#[derive(Default)]
pub(super) struct Pins {
    graph: Graph,
}

impl Pins {
    #[cfg(test)]
    pub(super) fn installed(&mut self, root: &Path) -> Result<Vec<RuntimeDependency>, String> {
        let Some(manifest) = resolve(root, "@flick-lang/runtime")? else {
            return Ok(vec![]);
        };
        if !manifest
            .parent()
            .expect("manifest directory")
            .join("dist")
            .is_dir()
        {
            return Ok(vec![]);
        }
        Ok(vec![self.graph.pin("@flick-lang/runtime", &manifest)?])
    }

    pub(super) fn verify(
        &mut self,
        root: &Path,
        pins: &[RuntimeDependency],
    ) -> Result<bool, String> {
        for pin in pins {
            let found = if pin.package == "@flick-lang/runtime" || pin.package == toolchain::ADAPTER
            {
                toolchain::dependency_manifest(root, &pin.package)
            } else {
                resolve(root, &pin.package)?
            };
            let Some(manifest) = found else {
                return Ok(false);
            };
            if self.graph.pin(&pin.package, &manifest)? != *pin {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub(super) fn installed_for(root: &Path, effect: bool) -> Result<Vec<RuntimeDependency>, String> {
    let mut roots: BTreeSet<String> = ["@flick-lang/runtime".into()].into();
    if effect {
        roots.insert(toolchain::ADAPTER.into());
    }
    if let Ok(bytes) = fs::read(root.join("package.json")) {
        let manifest: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        roots.extend(names(&manifest, false));
    }
    let mut graph = Graph::default();
    let mut pins = Vec::new();
    for name in roots {
        let found = if name == "@flick-lang/runtime" || name == toolchain::ADAPTER {
            toolchain::dependency_manifest(root, &name)
        } else {
            resolve(root, &name)?
        };
        if let Some(manifest) = found {
            pins.push(graph.pin(&name, &manifest)?);
        }
    }
    Ok(pins)
}

/// Launch roots already identify every package selected at build time.
/// The generated package.json need not repeat the source project's dependencies.
pub(super) fn verify(root: &Path, pins: &[RuntimeDependency]) -> Result<bool, String> {
    Pins::default().verify(root, pins)
}

fn names(manifest: &Value, peers: bool) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for key in ["dependencies", "optionalDependencies", "peerDependencies"] {
        if key == "peerDependencies" && !peers {
            continue;
        }
        if let Some(entries) = manifest[key].as_object() {
            names.extend(entries.keys().cloned());
        }
    }
    names
}

fn resolve(root: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let parts: Vec<_> = name.split('/').collect();
    let plain = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    let valid = match &parts[..] {
        [name] => plain(name),
        [scope, name] => scope.strip_prefix('@').is_some_and(plain) && plain(name),
        _ => false,
    };
    if !valid {
        return Err(format!("invalid runtime dependency name `{name}`"));
    }
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    Ok(root
        .ancestors()
        .filter(|p| p.file_name().is_none_or(|n| n != "node_modules"))
        .map(|p| p.join("node_modules").join(name).join("package.json"))
        .find(|p| p.is_file()))
}

fn content(package: &Path) -> Result<String, String> {
    let mut files = vec![PathBuf::from("package.json")];
    collect(package, Path::new(""), &mut files)
        .map_err(|e| format!("cannot pin the installed runtime: {e}"))?;
    files.sort();
    files.dedup();
    let mut hash = Sha256::new();
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let path = package.join(&relative);
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

fn collect(package: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(package.join(relative))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if name == "node_modules" || name == ".git" {
            continue;
        }
        let path = relative.join(name);
        if kind.is_dir() {
            collect(package, &path, files)?;
        } else if kind.is_file() {
            files.push(path);
        } else {
            return Err(io::Error::other("runtime contains a nonregular file"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::installed_for;
    use std::{env, fs, process};

    #[test]
    fn adapter_pins_include_transitive_runtime_content() {
        let root = env::temp_dir().join(format!("flick-adapter-content-{}", process::id()));
        let kernel = root.join("node_modules/@flick-lang/runtime");
        let adapter = root.join("node_modules/@flick-lang/effect-ts");
        let effect = adapter.join("node_modules/effect");
        for directory in [&kernel, &adapter, &effect] {
            fs::create_dir_all(directory.join("dist")).expect("package");
        }
        fs::write(
            kernel.join("package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        )
        .expect("kernel");
        fs::write(adapter.join("package.json"), r#"{"name":"@flick-lang/effect-ts","version":"0.1.0","dependencies":{"effect":"4.0.0"}}"#).expect("adapter");
        fs::write(
            effect.join("package.json"),
            r#"{"name":"effect","version":"4.0.0"}"#,
        )
        .expect("effect");
        fs::write(effect.join("dist/index.js"), "first").expect("content");
        let before = installed_for(&root, true).expect("pins");
        let pin = before
            .iter()
            .find(|p| p.package == "@flick-lang/effect-ts")
            .expect("adapter is pinned with its dependency graph");
        fs::write(effect.join("dist/index.js"), "other").expect("same-version change");
        let after = installed_for(&root, true).expect("changed pins");
        assert_ne!(
            pin.sha256,
            after
                .iter()
                .find(|p| p.package == pin.package)
                .expect("adapter pin")
                .sha256
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[cfg(all(test, unix))]
#[test]
fn the_kernel_pin_follows_project_resolution_with_a_symlinked_adapter() {
    use std::os::unix::fs::symlink;
    use std::{env, process};

    let root = env::temp_dir().join(format!("flick-kernel-resolution-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let project = root.join("project");
    let kernel = project.join("node_modules/@flick-lang/runtime");
    fs::create_dir_all(kernel.join("dist")).expect("kernel");
    fs::write(
        kernel.join("package.json"),
        r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
    )
    .expect("manifest");
    fs::write(kernel.join("dist/index.js"), "project kernel").expect("kernel bytes");
    let expected = installed(&project).expect("project pin");
    assert_eq!(expected.len(), 1);
    let adapter = root.join("external/adapter");
    let private = adapter.join("node_modules/@flick-lang/runtime");
    fs::create_dir_all(private.join("dist")).expect("private kernel");
    fs::write(
        private.join("package.json"),
        r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
    )
    .expect("manifest");
    fs::write(private.join("dist/index.js"), "private kernel").expect("private bytes");
    symlink(&adapter, project.join("node_modules/@flick-lang/effect-ts")).expect("adapter symlink");
    assert_eq!(installed(&project).expect("project pin"), expected);
    fs::write(kernel.join("dist/index.js"), "updated project kernel").expect("kernel change");
    assert_ne!(installed(&project).expect("changed project pin"), expected);
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(all(test, unix))]
#[test]
fn a_project_alias_pins_the_kernel_hoisted_above_the_real_importer() {
    use std::os::unix::fs::symlink;
    use std::{env, process};
    let root = env::temp_dir().join(format!("flick-project-kernel-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    for parent in ["real", "alias-parent"] {
        let kernel = root.join(parent).join("node_modules/@flick-lang/runtime");
        fs::create_dir_all(kernel.join("dist")).expect("kernel");
        fs::write(
            kernel.join("package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        )
        .expect("manifest");
        fs::write(kernel.join("dist/index.js"), parent).expect("different kernel bytes");
    }
    let project = root.join("real/project");
    fs::create_dir_all(&project).expect("project");
    let alias = root.join("alias-parent/project");
    symlink(&project, &alias).expect("alias");
    let pin = installed(&project).expect("real pin");
    assert_eq!(installed(&alias).expect("alias pin"), pin);
    fs::write(
        root.join("real/node_modules/@flick-lang/runtime/dist/index.js"),
        "changed",
    )
    .expect("update actual runtime");
    assert_ne!(installed(&alias).expect("updated pin"), pin);
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(all(test, unix))]
#[test]
fn runtime_pins_share_one_canonical_package_snapshot_per_launch() {
    use std::os::unix::fs::symlink;
    use std::{env, process};
    let root = env::temp_dir().join(format!("flick-launch-pin-snapshot-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let kernel = root.join("store/kernel");
    fs::create_dir_all(kernel.join("dist")).expect("runtime");
    fs::write(
        kernel.join("package.json"),
        r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
    )
    .expect("manifest");
    fs::write(kernel.join("dist/index.js"), "first").expect("bytes");
    for directory in ["A", "B"] {
        let modules = root.join(directory).join("node_modules/@flick-lang");
        fs::create_dir_all(&modules).expect("importer");
        symlink(&kernel, modules.join("runtime")).expect("same canonical runtime");
    }
    let mut pins = Pins::default();
    let first = pins.installed(&root.join("A")).expect("first importer");
    assert_eq!(first.len(), 1);
    fs::write(kernel.join("dist/index.js"), "second").expect("change bytes between queries");
    assert_eq!(
        pins.installed(&root.join("B")).expect("second importer"),
        first,
        "one launch hashes each resolved package once"
    );
    assert_ne!(
        installed(&root.join("B")).expect("next launch"),
        first,
        "the next launch takes a fresh snapshot"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
