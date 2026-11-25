//! Host-owned launch metadata, cached with the binary output. The guest
//! chooses artifact ids; only the host resolves tools to executable paths.

use std::collections::BTreeSet;
use std::fs;
use std::iter;
use std::path::Path;
use std::process::Command;

use flick_backend::{
    ArtifactKind, BackendPin, BuildResult, LaunchKind, Limits, OptionValue, RuntimeDependency,
    Toolchain,
};
use serde::{Deserialize, Serialize};

use super::tools::{self, Tool};
use crate::artifact::Artifact;

pub(super) const FILE: &str = ".flick-backend.cbor";

#[derive(Serialize, Deserialize)]
struct Manifest {
    backend: BackendPin,
    runtime_dependencies: Vec<RuntimeDependency>,
    importers: Vec<String>,
    launches: Vec<Launch>,
}

#[derive(Serialize, Deserialize)]
struct Launch {
    name: String,
    kind: LaunchKind,
    path: String,
    runtime: Option<Toolchain>,
    #[serde(default)]
    runtime_arguments: Vec<String>,
    arguments: Vec<String>,
    #[serde(default)]
    environment: Vec<OptionValue>,
}

pub(super) fn artifact(pin: &BackendPin, result: &BuildResult) -> Result<Artifact, String> {
    let launches = result
        .launches
        .iter()
        .map(|launch| {
            let outputs = result.steps.iter().flat_map(|s| &s.outputs);
            let path = (result.artifacts.iter())
                .map(|a| (&a.id, &a.path))
                .chain(outputs.map(|o| (&o.id, &o.path)))
                .find(|(id, _)| **id == launch.artifact)
                .map(|(_, path)| path.clone())
                .ok_or("backend launch names an unknown artifact")?;
            Ok(Launch {
                name: launch.name.clone(),
                kind: launch.kind,
                path,
                runtime: launch.runtime.clone(),
                runtime_arguments: launch.runtime_arguments.clone(),
                arguments: launch.arguments.clone(),
                environment: launch.environment.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let manifest = Manifest {
        backend: pin.clone(),
        runtime_dependencies: result.runtime_dependencies.clone(),
        importers: result.artifacts.iter().map(|a| a.path.clone()).collect(),
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
    installed: &[RuntimeDependency],
) -> Result<(), String> {
    let file = files
        .iter()
        .find(|f| f.path == FILE)
        .ok_or("component build has no launch manifest")?;
    let manifest = read(&file.contents, pin)?;
    if !manifest
        .runtime_dependencies
        .iter()
        .all(|pin| installed.contains(pin))
    {
        return Err("cached runtime dependency differs from its host pin".into());
    }
    for launch in manifest.launches {
        let artifact = files
            .iter()
            .find(|a| a.path == launch.path)
            .ok_or("launch artifact is missing")?;
        tools::authorize_launch_parts(
            launch.runtime.as_ref(),
            &launch.runtime_arguments,
            &launch.environment,
        )?;
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
    flick_backend::validate_output_paths(
        manifest.importers.iter().map(String::as_str),
        Limits::default(),
    )?;
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

fn load_manifest(root: &Path, pin: &BackendPin) -> Result<Manifest, String> {
    let path = root.join(FILE);
    let size = fs::metadata(&path)
        .map_err(|e| format!("cannot read launch manifest: {e}"))?
        .len();
    if size > u64::try_from(Limits::default().artifact_bytes).unwrap_or(u64::MAX) {
        return Err("launch manifest exceeds its size limit".into());
    }
    let manifest = read(&fs::read(&path).map_err(|e| e.to_string())?, pin)?;
    Ok(manifest)
}

fn verify_runtime_manifest(root: &Path, manifest: &Manifest) -> Result<(), String> {
    if !manifest.runtime_dependencies.is_empty() {
        let directories: BTreeSet<_> = manifest
            .importers
            .iter()
            .map(|path| {
                root.join(path)
                    .parent()
                    .expect("validated artifact path")
                    .to_path_buf()
            })
            .chain(iter::once(root.to_path_buf()))
            .collect();
        let names: Vec<String> = (manifest.runtime_dependencies.iter())
            .map(|d| d.package.clone())
            .collect();
        for directory in directories {
            let installed =
                super::runtimes::runtime_from(&manifest.backend, &directory, root, &names)?;
            if !manifest
                .runtime_dependencies
                .iter()
                .all(|pin| installed.contains(pin))
            {
                return Err(
                    "launch runtime dependency differs from its pinned version and digest".into(),
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn command(
    root: &Path,
    pin: &BackendPin,
    kind: LaunchKind,
    name: &str,
) -> Result<Command, String> {
    let manifest = load_manifest(root, pin)?;
    verify_runtime_manifest(root, &manifest)?;
    let launch = manifest
        .launches
        .iter()
        .find(|l| l.kind == kind && l.name == name)
        .ok_or_else(|| format!("backend supplied no {kind:?} launch named `{name}`"))?;
    tools::authorize_launch_parts(
        launch.runtime.as_ref(),
        &launch.runtime_arguments,
        &launch.environment,
    )?;
    let artifact = root.join(&launch.path);
    let mut command = if let Some(runtime) = &launch.runtime {
        let mut command = Tool::find(&runtime.id, Some(runtime))?.command();
        command.args(&launch.runtime_arguments);
        command.arg(artifact);
        command
    } else {
        Command::new(artifact)
    };
    command.args(&launch.arguments);
    command.envs(launch.environment.iter().map(|v| (&v.key, &v.value)));
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::{FILE, artifact, command};
    use crate::backends::{packages, runtimes};
    use flick_backend::{
        BackendId, BackendPin, BuildResult, Launch, LaunchKind, OptionValue, sha256,
    };
    use std::path::Path;
    use std::{env, fs, process};

    fn installed_rt(root: &Path) -> Vec<flick_backend::RuntimeDependency> {
        let backend = packages::tests::js();
        let options = [
            OptionValue {
                key: "option.runtime".into(),
                value: "rt".into(),
            },
            OptionValue {
                key: "root".into(),
                value: packages::canonical_slashed(root),
            },
        ];
        packages::report(&*backend, root, &[], &options)
            .expect("runtime pin")
            .runtime
    }

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
        let runtimes = installed_rt(&root);
        let pin = BackendPin {
            identity: BackendId {
                id: "test/executable".into(),
                version: "1.0.0".into(),
            },
            sha256: sha256(b"component"),
        };
        runtimes::loaded(&pin, &packages::tests::js());
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
                runtime_arguments: vec![],
                arguments: vec![],
                environment: vec![],
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

    #[test]
    fn a_launch_may_run_the_output_of_a_step() {
        let pin = BackendPin {
            identity: BackendId {
                id: "test/steps".into(),
                version: "1.0.0".into(),
            },
            sha256: sha256(b"component"),
        };
        let result = BuildResult {
            artifacts: vec![],
            steps: vec![flick_backend::ToolchainStep {
                id: "link".into(),
                dependencies: vec![],
                toolchain: flick_backend::Toolchain {
                    id: "go".into(),
                    version: "1.23.0".into(),
                    sha256: "0".repeat(64),
                },
                arguments: vec![],
                environment: vec![],
                inputs: vec![],
                outputs: vec![flick_backend::StepOutput {
                    id: "exe:main".into(),
                    path: "flick-main".into(),
                    kind: flick_backend::ArtifactKind::Executable,
                }],
            }],
            runtime_dependencies: vec![],
            launches: vec![Launch {
                name: "main".into(),
                kind: LaunchKind::Run,
                artifact: "exe:main".into(),
                runtime: None,
                runtime_arguments: vec![],
                arguments: vec![],
                environment: vec![],
            }],
            diagnostics: vec![],
        };
        let manifest = artifact(&pin, &result).expect("a launch of a step output");
        assert_eq!(
            super::run_paths(&[manifest], &pin),
            [("main".to_owned(), "flick-main".to_owned())]
        );
    }

    #[test]
    fn runtime_pins_are_checked_from_nested_importers() {
        let root = env::temp_dir().join(format!("flick-launch-shadow-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let kernel = root.join("node_modules/@flick-lang/runtime");
        fs::create_dir_all(kernel.join("dist")).expect("kernel");
        fs::create_dir_all(root.join("Suite")).expect("module directory");
        fs::write(
            kernel.join("package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        )
        .expect("manifest");
        fs::write(kernel.join("dist/index.js"), "pinned runtime").expect("runtime");
        let pin = BackendPin {
            identity: BackendId {
                id: "test/shadow".into(),
                version: "1.0.0".into(),
            },
            sha256: sha256(b"component"),
        };
        runtimes::loaded(&pin, &packages::tests::js());
        let result = BuildResult {
            artifacts: vec![flick_backend::Artifact {
                id: "entry".into(),
                path: "Suite/Entry.js".into(),
                kind: flick_backend::ArtifactKind::Executable,
                bytes: vec![],
            }],
            steps: vec![],
            runtime_dependencies: installed_rt(&root),
            launches: vec![Launch {
                name: "main".into(),
                kind: LaunchKind::Run,
                artifact: "entry".into(),
                runtime: None,
                runtime_arguments: vec![],
                arguments: vec![],
                environment: vec![],
            }],
            diagnostics: vec![],
        };
        fs::write(
            root.join(FILE),
            artifact(&pin, &result).expect("manifest").contents,
        )
        .expect("launch metadata");
        command(&root, &pin, LaunchKind::Run, "main").expect("matching resolution");
        let shadow = root.join("Suite/node_modules/@flick-lang/runtime");
        fs::create_dir_all(shadow.join("dist")).expect("shadow runtime");
        fs::copy(kernel.join("package.json"), shadow.join("package.json")).expect("same version");
        fs::write(shadow.join("dist/index.js"), "unapproved runtime").expect("shadow bytes");
        let error = command(&root, &pin, LaunchKind::Run, "main")
            .expect_err("nested importer resolves a different runtime");
        assert!(error.contains("runtime dependency"), "{error}");
        fs::remove_dir_all(root).expect("cleanup");
    }

    /// `flick run` and `flick test` launch from a directory under `.flick` of
    /// a project with no `.git` ancestor: its runtime is still found in the
    /// project's own `node_modules`.
    #[test]
    fn a_launch_below_a_standalone_project_finds_its_runtime() {
        let project = env::temp_dir().join(format!("flick-launch-standalone-{}", process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(&project).expect("project");
        fs::write(project.join("flick.toml"), "[package]\nname = \"app\"\n").expect("manifest");
        let kernel = project.join("node_modules/@flick-lang/runtime");
        fs::create_dir_all(kernel.join("dist")).expect("runtime");
        fs::write(
            kernel.join("package.json"),
            r#"{"name":"@flick-lang/runtime","version":"0.1.0"}"#,
        )
        .expect("runtime manifest");
        fs::write(kernel.join("dist/index.js"), "runtime").expect("runtime");
        let pin = BackendPin {
            identity: BackendId {
                id: "test/standalone".into(),
                version: "1.0.0".into(),
            },
            sha256: sha256(b"component"),
        };
        runtimes::loaded(&pin, &packages::tests::js());
        let result = BuildResult {
            artifacts: vec![flick_backend::Artifact {
                id: "main".into(),
                path: "entry".into(),
                kind: flick_backend::ArtifactKind::Executable,
                bytes: vec![],
            }],
            steps: vec![],
            runtime_dependencies: installed_rt(&project),
            launches: vec![Launch {
                name: "main".into(),
                kind: LaunchKind::Run,
                artifact: "main".into(),
                runtime: None,
                runtime_arguments: vec![],
                arguments: vec![],
                environment: vec![],
            }],
            diagnostics: vec![],
        };
        let manifest = artifact(&pin, &result).expect("manifest");
        for generation in [project.join(".flick/run/1-1"), project.join(".flick/test")] {
            fs::create_dir_all(&generation).expect("generation");
            fs::write(generation.join(FILE), &manifest.contents).expect("launch metadata");
            if let Err(error) = command(&generation, &pin, LaunchKind::Run, "main") {
                panic!("{}: {error}", generation.display());
            }
        }
        fs::remove_dir_all(project).expect("cleanup");
    }
}
