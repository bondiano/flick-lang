use std::path::Path;
use std::sync::Arc;
use std::{env, fs, process};

use flick_backend::command::Dependency;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{
    Backend, BackendPin, BuildRequest, BuildResult, Descriptor, Diagnostic, Inputs, OptionValue,
    PackageRoot, Packages, ProjectFile, RuntimeGraph, RuntimeNode, TargetSupport,
};

use super::report;
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

/// The JS backend resolves and pins the runtime graphs and their transitive
/// dependencies, and changes to transitive files change the content digest.
#[test]
fn the_backend_pins_the_runtime_and_transitive_dependencies() {
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
    let real = super::canonical_slashed(&root);
    let backend = js();
    let pin = |runtime: &str| {
        let options = [
            OptionValue {
                key: "option.runtime".into(),
                value: runtime.into(),
            },
            OptionValue {
                key: "root".into(),
                value: real.clone(),
            },
        ];
        report(&*backend, &root, &[], &options).expect("a report")
    };
    let rt = pin("rt");
    let rt_names: Vec<&str> = rt.runtime.iter().map(|r| r.package.as_str()).collect();
    assert_eq!(rt_names, ["@flick-lang/runtime", "left"], "{rt_names:?}");
    let runtime_pin = rt
        .runtime
        .iter()
        .find(|r| r.package == "@flick-lang/runtime")
        .expect("runtime pin");
    assert_eq!(runtime_pin.version, "0.1.0");
    assert_eq!(runtime_pin.sha256.len(), 64);

    let effect = pin("effect");
    let names: Vec<&str> = effect.runtime.iter().map(|r| r.package.as_str()).collect();
    assert_eq!(
        names,
        ["@flick-lang/effect-ts", "@flick-lang/runtime", "left"],
        "{names:?}"
    );
    let adapter_before = effect
        .runtime
        .iter()
        .find(|r| r.package == "@flick-lang/effect-ts")
        .expect("effect adapter")
        .sha256
        .clone();
    write(
        &modules.join("@flick-lang/effect-ts/node_modules/effect/dist/a.js"),
        "effect-changed",
    );
    let effect_after = pin("effect");
    let adapter_after = effect_after
        .runtime
        .iter()
        .find(|r| r.package == "@flick-lang/effect-ts")
        .expect("effect adapter")
        .sha256
        .clone();
    assert_ne!(
        adapter_before, adapter_after,
        "transitive dependency change invalidates pin"
    );
    let lib = (effect.sources.iter())
        .find(|s| s.package == "lib")
        .expect("the Flick library");
    assert_eq!(
        (lib.version.as_str(), lib.root.as_str()),
        ("1.2.0", "node_modules/lib/src")
    );
    fs::remove_dir_all(root).expect("cleanup");
}

struct StubBackend {
    patterns: Vec<String>,
    answer: Packages,
}

impl Backend for StubBackend {
    fn describe(&self) -> Descriptor {
        unreachable!("stub backend only tests packages()")
    }

    fn check_target(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        unreachable!("stub backend only tests packages()")
    }

    fn generate(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        unreachable!("stub backend only tests packages()")
    }

    fn package_files(&self) -> Vec<String> {
        self.patterns.clone()
    }

    fn packages(&self, _: &[ProjectFile], _: &[Dependency], _: &[OptionValue]) -> Packages {
        self.answer.clone()
    }
}

fn empty_packages() -> Packages {
    Packages {
        name: None,
        version: None,
        declared: Vec::new(),
        sources: Vec::new(),
        runtime: Vec::new(),
        inputs: Vec::new(),
        diagnostics: Vec::new(),
        needs: Vec::new(),
    }
}

#[test]
fn report_rejects_escaping_patterns_roots_needs_inputs_and_runtime_dirs() {
    let base = env::temp_dir().join(format!("flick-packages-sandbox-{}", process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("work/app");
    let outside = base.join("secret");
    write(&root.join("package.json"), r#"{"name":"app"}"#);
    write(&outside.join("id_rsa"), "secret-key");
    write(&base.join(".ssh/id_rsa"), "ssh-secret");

    // 1. Escaping or hidden `package-files` patterns are rejected.
    for bad in [
        "../../../etc/passwd",
        "/etc/passwd",
        "node_modules/../../etc/passwd",
        ".ssh/id_rsa",
        "./package.json",
    ] {
        let stub = StubBackend {
            patterns: vec![bad.into()],
            answer: empty_packages(),
        };
        let err = report(&stub, &root, &[], &[])
            .err()
            .expect("invalid pattern rejected");
        assert!(err.contains("invalid package file pattern"), "{bad}: {err}");
        assert!(!super::any_match(&root, &[bad.into()]), "{bad}");
        assert!(super::named(&root, &[bad.into()]).is_empty(), "{bad}");
    }

    // 2. Escaping `PackageRoot.root` is rejected.
    for bad_root in ["/etc", "../../secret", "node_modules/../../secret", "./src"] {
        let mut answer = empty_packages();
        answer.sources.push(PackageRoot {
            package: "evil".into(),
            version: "1.0.0".into(),
            root: bad_root.into(),
        });
        let stub = StubBackend {
            patterns: vec!["package.json".into()],
            answer,
        };
        let err = report(&stub, &root, &[], &[])
            .err()
            .expect("escaping root rejected");
        assert!(err.contains("outside the project"), "{bad_root}: {err}");
    }

    // 3. `needs` cannot read arbitrary files outside package scope even if basename matches.
    let canonical_base = fs::canonicalize(&base).expect("base");
    let ssh_key = canonical_base
        .join(".ssh/id_rsa")
        .to_string_lossy()
        .replace('\\', "/");
    let outside_pkg = canonical_base
        .join("secret/node_modules/evil/package.json")
        .to_string_lossy()
        .replace('\\', "/");
    for (patterns, bad_need) in [
        (vec!["id_rsa".into()], ssh_key),
        (vec!["passwd".into()], "/etc/passwd".into()),
        (
            vec!["package.json".into(), "node_modules/*/package.json".into()],
            outside_pkg,
        ),
        (
            vec!["package.json".into(), "node_modules/*/package.json".into()],
            "node_modules/../../secret/package.json".into(),
        ),
    ] {
        let mut answer = empty_packages();
        answer.needs.push(bad_need.clone());
        let stub = StubBackend { patterns, answer };
        let err = report(&stub, &root, &[], &[])
            .err()
            .expect("out-of-scope need rejected");
        assert!(
            err.contains("outside its package scope"),
            "{bad_need}: {err}"
        );
    }

    // 4. `inputs` cannot escape package scope.
    let mut bad_inputs = empty_packages();
    bad_inputs.inputs.push("/etc/passwd".into());
    let stub = StubBackend {
        patterns: vec!["package.json".into()],
        answer: bad_inputs,
    };
    assert!(report(&stub, &root, &[], &[]).is_err());

    // 5. `RuntimeNode.dir` must match a canonical parent directory supplied by the host.
    let mut bad_runtime = empty_packages();
    bad_runtime.runtime.push(RuntimeGraph {
        package: "evil".into(),
        nodes: vec![RuntimeNode {
            name: "evil".into(),
            version: "1.0.0".into(),
            dir: canonical_base
                .join("secret")
                .to_string_lossy()
                .replace('\\', "/"),
            dependencies: Vec::new(),
        }],
        skip: vec!["node_modules".into()],
    });
    let stub = StubBackend {
        patterns: vec!["package.json".into()],
        answer: bad_runtime,
    };
    let err = report(&stub, &root, &[], &[])
        .err()
        .expect("unsupplied runtime dir rejected");
    assert!(err.contains("host did not supply"), "{err}");

    fs::remove_dir_all(base).expect("cleanup");
}

#[test]
fn canonical_normalization_strips_windows_verbatim_prefixes_and_blocks_drive_paths() {
    assert_eq!(
        super::normalize_canonical_str(r"\\?\C:\Users\runner\project"),
        "C:/Users/runner/project"
    );
    assert_eq!(
        super::normalize_canonical_str(r"\\?\UNC\server\share\dir"),
        "//server/share/dir"
    );
    assert_eq!(
        super::normalize_canonical_str("//?/C:/Users/runner/project"),
        "C:/Users/runner/project"
    );
    assert_eq!(
        super::normalize_canonical_str("/Users/alice/project"),
        "/Users/alice/project"
    );
    assert!(!super::is_safe_relative("C:/Windows/System32"));
    assert!(!super::is_safe_relative("C:relative"));
    assert!(super::scope::is_absolute_slashed("C:/Users/runner/app"));
    assert!(super::scope::is_absolute_slashed("//server/share/app"));
}

struct EscalatingBackend {
    patterns: Vec<String>,
    round1_need: String,
    round2_need: String,
}

impl Backend for EscalatingBackend {
    fn describe(&self) -> Descriptor {
        unreachable!()
    }
    fn check_target(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        unreachable!()
    }
    fn generate(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        unreachable!()
    }
    fn package_files(&self) -> Vec<String> {
        self.patterns.clone()
    }
    fn packages(&self, files: &[ProjectFile], _: &[Dependency], _: &[OptionValue]) -> Packages {
        let mut answer = empty_packages();
        if !files.iter().any(|f| f.path == self.round1_need) {
            answer.needs.push(self.round1_need.clone());
        } else if !files.iter().any(|f| f.path == self.round2_need) {
            answer.needs.push(self.round2_need.clone());
        }
        answer
    }
}

#[test]
fn crafted_patterns_cannot_walk_to_system_home_or_escalate_from_supplied_dirs() {
    let base = env::temp_dir().join(format!("flick-packages-escalate-{}", process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("work/app");
    write(&root.join("package.json"), r#"{"name":"app"}"#);
    write(
        &base.join("secret/node_modules/evil/package.json"),
        r#"{"name":"evil"}"#,
    );
    let canonical_base = super::canonical_slashed(&base);

    // 1. Crafted multi-segment patterns cannot reach `/private/etc/passwd`,
    //    `/Users/...`, `$HOME/Documents/...`, or multi-literal `secret/node_modules/*`.
    for (patterns, bad_need) in [
        (
            vec!["private/*/passwd".into()],
            "/private/etc/passwd".to_owned(),
        ),
        (
            vec!["Users/*/*/credentials.json".into()],
            "/Users/alice/work/credentials.json".to_owned(),
        ),
        (
            vec!["Documents/*/secret.txt".into()],
            format!("{canonical_base}/Documents/taxes/secret.txt"),
        ),
        (
            vec!["secret/node_modules/*/package.json".into()],
            format!("{canonical_base}/secret/node_modules/evil/package.json"),
        ),
    ] {
        let mut answer = empty_packages();
        answer.needs.push(bad_need.clone());
        let stub = StubBackend { patterns, answer };
        let err = report(&stub, &root, &[], &[])
            .err()
            .expect("crafted ancestor escape rejected");
        assert!(
            err.contains("outside its package scope"),
            "{bad_need}: {err}"
        );
    }

    // 2. Even inside an enclosing git repo ceiling, reading a valid external manifest
    //    in round 1 cannot escalate in round 2 to read arbitrary basename files or
    //    dot-prefixed files from that external directory.
    let repo = base.join("monorepo");
    let app = repo.join("apps/web");
    fs::create_dir_all(repo.join(".git")).expect("git marker");
    write(&app.join("package.json"), r#"{"name":"web"}"#);
    write(&repo.join("pkg/foo/manifest.json"), r#"{"name":"foo"}"#);
    write(&repo.join("pkg/foo/secret.txt"), "top-secret");
    write(&repo.join("pkg/foo/.env/secret.txt"), "dot-secret");
    let canonical_repo = super::canonical_slashed(&repo);
    for round2 in [
        format!("{canonical_repo}/pkg/foo/secret.txt"),
        format!("{canonical_repo}/pkg/foo/.env/secret.txt"),
    ] {
        let backend = EscalatingBackend {
            patterns: vec!["secret.txt".into(), "pkg/*/manifest.json".into()],
            round1_need: format!("{canonical_repo}/pkg/foo/manifest.json"),
            round2_need: round2.clone(),
        };
        let err = report(&backend, &app, &[], &[])
            .err()
            .expect("round-2 escalation rejected");
        assert!(err.contains("outside its package scope"), "{round2}: {err}");
    }

    fs::remove_dir_all(base).expect("cleanup");
}
