//! The compiler owns the runtime ABI; the adapter owns its npm requirements.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use std::{env, fs, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes};

pub const COMPILER: &str = env!("CARGO_PKG_VERSION");
pub const RUNTIME: &str = "0.1.0";
pub const ADAPTER: &str = "@flick-lang/effect-ts";
const ADAPTER_MANIFEST: &str = include_str!("../../packages/effect-ts/package.json");

/// Default for newly scaffolded projects, supplied by the adapter package.
pub fn adapter_version() -> String {
    let manifest: serde_json::Value =
        serde_json::from_str(ADAPTER_MANIFEST).expect("adapter manifest");
    manifest["version"]
        .as_str()
        .expect("adapter version")
        .to_owned()
}
pub const PNPM: &str = "12.16.1";
pub const NODE: &str = "24.18.0";
/// The TypeScript release `flick content-mapper` is verified against: the
/// first with Content Mappers.
pub const TYPESCRIPT_GO: &str = "7.1.0-beta";
/// React of the `fullstack` template, the version the runtime is tested with.
pub const REACT: &str = "19.3.1";
/// Vite of the `fullstack` template.
pub const VITE: &str = "8.3.5";

/// This compiler as caches tell binaries apart: its version, and the size
/// and modification time of its executable, which a rebuild without a new
/// version still changes.
pub fn fingerprint() -> Option<String> {
    let exe = fs::metadata(env::current_exe().ok()?).ok()?;
    let modified = exe.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!(
        "{COMPILER}-{:x}-{:x}",
        exe.len(),
        modified.as_nanos()
    ))
}

fn version_at(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("version")?.as_str().map(str::to_owned)
}

/// The version of an installed package, from `root/node_modules`.
pub fn installed(root: &Path, package: &str) -> Option<String> {
    version_at(&root.join("node_modules").join(package).join("package.json"))
}

/// Node walks the real directory's ancestors, skipping `node_modules` itself.
fn dependency_candidates(root: &Path, package: &str) -> Vec<PathBuf> {
    let adapter = root.join("node_modules").join(ADAPTER);
    let real = fs::canonicalize(&adapter).unwrap_or(adapter);
    real.ancestors()
        .filter(|dir| dir.file_name().is_none_or(|name| name != "node_modules"))
        .map(|dir| dir.join("node_modules").join(package).join("package.json"))
        .collect()
}

/// A failed real-path resolution stays unresolved, including external links.
pub fn dependency_manifest(root: &Path, package: &str) -> Option<PathBuf> {
    dependency_candidates(root, package)
        .into_iter()
        .find(|path| path.exists())
}

/// The adapter and kernel resolve from the project; adapter dependencies from its real path.
pub fn resolved(root: &Path, package: &str) -> Option<String> {
    if package == ADAPTER || package == "@flick-lang/runtime" {
        return installed(root, package);
    }
    dependency_manifest(root, package)
        .as_deref()
        .and_then(version_at)
}

/// Exact production dependency pins of an installed adapter, without compiler-owned versions.
/// No installed adapter is normal for compile-only projects.
pub fn adapter_requirements(root: &Path) -> Result<Vec<(String, String)>, String> {
    let path = root.join("node_modules").join(ADAPTER).join("package.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                && path
                    .parent()
                    .is_none_or(|dir| fs::symlink_metadata(dir).is_err()) =>
        {
            return Ok(Vec::new());
        }
        Err(error) => return Err(format!("cannot read `{}`: {error}", path.display())),
    };
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid `{}`: {error}", path.display()))?;
    if json.get("version").and_then(|v| v.as_str()).is_none() {
        return Err(format!("`{ADAPTER}` has no package version"));
    }
    let deps = json
        .get("dependencies")
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("`{ADAPTER}` has no dependency contract"))?;
    if !deps.contains_key("effect") {
        return Err(format!("`{ADAPTER}` has no `effect` requirement"));
    }
    deps.iter()
        .map(|(name, value)| {
            if name == "@flick-lang/runtime" && value.as_str() == Some("workspace:*") {
                let runtime: serde_json::Value =
                    serde_json::from_str(include_str!("../../runtime/package.json"))
                        .expect("runtime manifest");
                return Ok((
                    name.clone(),
                    runtime["version"]
                        .as_str()
                        .expect("runtime version")
                        .to_owned(),
                ));
            }
            let version = value
                .as_str()
                .filter(|v| {
                    let parts: Vec<_> = v.split('.').collect();
                    parts.len() == 3
                        && parts
                            .iter()
                            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                })
                .ok_or_else(|| {
                    format!("`{ADAPTER}` must pin `{name}` to an exact release version")
                })?;
            Ok((name.clone(), version.to_owned()))
        })
        .collect()
}

/// Inputs include manifest contents, so changing requirements at the same version invalidates caches.
pub fn contract_inputs(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.join("node_modules").join(ADAPTER).join("package.json")];
    if let Ok(requirements) = adapter_requirements(root) {
        paths.extend(
            requirements
                .iter()
                .flat_map(|(name, _)| dependency_candidates(root, name)),
        );
    }
    paths
}

/// `P015` for a runtime ABI mismatch or an unsatisfied installed adapter contract.
pub fn check_runtime(root: &Path, rt: bool) -> Option<Diagnostic> {
    let error = |message| Diagnostic::new(codes::P015, Severity::Error, message);
    if let Some(found) = installed(root, "@flick-lang/runtime")
        && found != RUNTIME
    {
        return Some(error(format!(
            "the installed `@flick-lang/runtime` is {found}, but flick {COMPILER} needs {RUNTIME}: install `@flick-lang/runtime@{RUNTIME}`"
        )));
    }
    if rt {
        return None;
    }
    let requirements = match adapter_requirements(root) {
        Ok(requirements) => requirements,
        Err(message) => return Some(error(message)),
    };
    for (package, wanted) in requirements {
        if package == "@flick-lang/runtime" && wanted != RUNTIME {
            return Some(error(format!(
                "`{ADAPTER}` needs runtime {wanted}, but flick {COMPILER} supports runtime {RUNTIME}"
            )));
        }
        let found = dependency_manifest(root, &package)
            .as_deref()
            .and_then(version_at);
        if found.as_deref() != Some(&wanted) {
            return Some(error(format!(
                "`{ADAPTER}` needs `{package}@{wanted}`, but resolves {}: run `flick install`",
                found.as_deref().unwrap_or("no installed package")
            )));
        }
    }
    None
}
