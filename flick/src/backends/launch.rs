//! Host-owned launch metadata, cached with the binary output. The guest
//! chooses artifact ids; only the host resolves tools to executable paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

use flick_backend::{
    ArtifactKind, BackendPin, BuildResult, LaunchKind, Limits, RuntimeDependency, Toolchain,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifact::Artifact;

pub(super) const FILE: &str = ".flick-backend.cbor";

#[derive(Serialize, Deserialize)]
struct Manifest {
    backend: BackendPin,
    runtime_dependencies: Vec<RuntimeDependency>,
    launches: Vec<Launch>,
}

#[derive(Serialize, Deserialize)]
struct Launch {
    name: String,
    kind: LaunchKind,
    path: String,
    runtime: Option<Toolchain>,
    arguments: Vec<String>,
}

pub(super) struct Tool {
    pub pin: Toolchain,
    path: PathBuf,
}

impl Tool {
    pub(super) fn node() -> Result<Self, String> {
        let name = if cfg!(windows) { "node.exe" } else { "node" };
        let path = env::var_os("PATH")
            .into_iter()
            .flat_map(|p| env::split_paths(&p).collect::<Vec<_>>())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
            .ok_or("cannot find Node.js on PATH")?;
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let version = Command::new(&path)
            .arg("--version")
            .output()
            .map_err(|e| e.to_string())?;
        if !version.status.success() {
            return Err("cannot query Node.js version".into());
        }
        let version =
            String::from_utf8(version.stdout).map_err(|_| "Node.js version is not UTF-8")?;
        let version = version
            .trim()
            .strip_prefix('v')
            .ok_or("invalid Node.js version")?
            .to_owned();
        let mut hash = Sha256::new();
        io::copy(
            &mut fs::File::open(&path).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            pin: Toolchain {
                id: "node".into(),
                version,
                sha256: format!("{:x}", hash.finalize()),
            },
            path,
        })
    }
}

pub(super) fn artifact(pin: &BackendPin, result: &BuildResult) -> Result<Artifact, String> {
    let launches = result
        .launches
        .iter()
        .map(|launch| {
            let path = result
                .artifacts
                .iter()
                .find(|a| a.id == launch.artifact)
                .map(|a| a.path.clone())
                .ok_or("backend launch requires an unavailable tool output")?;
            Ok(Launch {
                name: launch.name.clone(),
                kind: launch.kind,
                path,
                runtime: launch.runtime.clone(),
                arguments: launch.arguments.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let manifest = Manifest {
        backend: pin.clone(),
        runtime_dependencies: result.runtime_dependencies.clone(),
        launches,
    };
    let mut contents = vec![];
    ciborium::into_writer(&manifest, &mut contents).map_err(|e| e.to_string())?;
    Ok(Artifact {
        path: FILE.into(),
        contents,
        kind: ArtifactKind::Resource,
    })
}

pub(super) fn validate(
    files: &[Artifact],
    pin: &BackendPin,
    tools: &[Toolchain],
    runtimes: &[RuntimeDependency],
) -> Result<(), String> {
    let file = files
        .iter()
        .find(|f| f.path == FILE)
        .ok_or("component build has no launch manifest")?;
    let manifest = read(&file.contents, pin)?;
    if manifest
        .runtime_dependencies
        .iter()
        .any(|r| !runtimes.contains(r))
    {
        return Err("cached runtime dependency differs from its host pin".into());
    }
    for launch in manifest.launches {
        let artifact = files
            .iter()
            .find(|a| a.path == launch.path)
            .ok_or("launch artifact is missing")?;
        if let Some(runtime) = launch.runtime {
            if !tools.contains(&runtime) {
                return Err("cached launch toolchain differs from the selected toolchain".into());
            }
        } else if artifact.kind != ArtifactKind::Executable {
            return Err("launch without a runtime requires an executable".into());
        }
    }
    Ok(())
}

fn read(bytes: &[u8], pin: &BackendPin) -> Result<Manifest, String> {
    if bytes.len() > Limits::default().artifact_bytes {
        return Err("launch manifest exceeds its size limit".into());
    }
    let manifest: Manifest =
        ciborium::from_reader(bytes).map_err(|e| format!("invalid launch manifest: {e}"))?;
    if manifest.backend != *pin {
        return Err("launch manifest differs from the selected backend pin".into());
    }
    let mut names = BTreeSet::new();
    for launch in &manifest.launches {
        if launch.name.is_empty() || !names.insert((&launch.name, launch.kind == LaunchKind::Test))
        {
            return Err("duplicate or empty launch name".into());
        }
        flick_backend::validate_output_paths([launch.path.as_str()], Limits::default())?;
    }
    Ok(manifest)
}

pub(super) fn run_paths(files: &[Artifact], pin: &BackendPin) -> Vec<(String, String)> {
    files
        .iter()
        .find(|f| f.path == FILE)
        .and_then(|file| read(&file.contents, pin).ok())
        .map(|m| {
            m.launches
                .into_iter()
                .filter(|l| l.kind == LaunchKind::Run)
                .map(|l| (l.name, l.path))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn command(
    root: &Path,
    pin: &BackendPin,
    kind: LaunchKind,
    name: &str,
) -> Result<Command, String> {
    let path = root.join(FILE);
    let size = fs::metadata(&path)
        .map_err(|e| format!("cannot read launch manifest: {e}"))?
        .len();
    if size > u64::try_from(Limits::default().artifact_bytes).unwrap_or(u64::MAX) {
        return Err("launch manifest exceeds its size limit".into());
    }
    let manifest = read(&fs::read(&path).map_err(|e| e.to_string())?, pin)?;
    if !manifest.runtime_dependencies.is_empty() {
        let installed = super::dependencies::installed(root)?;
        if manifest
            .runtime_dependencies
            .iter()
            .any(|r| !installed.contains(r))
        {
            return Err(
                "launch runtime dependency differs from its pinned version and digest".into(),
            );
        }
    }
    let launch = manifest
        .launches
        .iter()
        .find(|l| l.kind == kind && l.name == name)
        .ok_or_else(|| format!("backend supplied no {kind:?} launch named `{name}`"))?;
    let artifact = root.join(&launch.path);
    let mut command = if let Some(runtime) = &launch.runtime {
        let tool = match runtime.id.as_str() {
            "node" => Tool::node()?,
            _ => return Err(format!("no host runtime adapter for `{}`", runtime.id)),
        };
        if tool.pin != *runtime {
            return Err("launch runtime differs from its pinned version and digest".into());
        }
        let mut command = Command::new(tool.path);
        if runtime.id == "node" {
            command.arg("--enable-source-maps");
        }
        command.arg(artifact);
        command
    } else {
        Command::new(artifact)
    };
    command.args(&launch.arguments);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::{FILE, artifact, command};
    use crate::backends::dependencies;
    use flick_backend::{BackendId, BackendPin, BuildResult, Launch, LaunchKind, sha256};
    use std::{env, fs, process};

    #[test]
    fn a_launch_rechecks_the_content_pin_of_its_runtime_dependencies() {
        let root = env::temp_dir().join(format!("flick-launch-runtime-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let kernel = root.join("node_modules/@flick-lang/runtime");
        fs::create_dir_all(kernel.join("dist")).expect("runtime");
        fs::write(
            kernel.join("package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        )
        .expect("manifest");
        fs::write(kernel.join("dist/index.js"), "first").expect("original runtime");
        let runtimes = dependencies::installed(&root).expect("runtime pin");
        let pin = BackendPin {
            identity: BackendId {
                id: "test/executable".into(),
                version: "1.0.0".into(),
            },
            sha256: sha256(b"component"),
        };
        let result = BuildResult {
            artifacts: vec![flick_backend::Artifact {
                id: "main".into(),
                path: "entry".into(),
                kind: flick_backend::ArtifactKind::Executable,
                bytes: vec![],
            }],
            steps: vec![],
            runtime_dependencies: runtimes,
            launches: vec![Launch {
                name: "main".into(),
                kind: LaunchKind::Run,
                artifact: "main".into(),
                runtime: None,
                arguments: vec![],
            }],
            diagnostics: vec![],
        };
        let manifest = artifact(&pin, &result).expect("manifest");
        fs::write(root.join(FILE), manifest.contents).expect("launch metadata");
        command(&root, &pin, LaunchKind::Run, "main").expect("matching runtime");
        fs::write(kernel.join("dist/index.js"), "changed")
            .expect("change runtime without changing version");
        let error = command(&root, &pin, LaunchKind::Run, "main").expect_err("a stale runtime pin");
        assert!(error.contains("runtime dependency"), "{error}");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
