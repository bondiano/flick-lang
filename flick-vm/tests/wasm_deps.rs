//! Verifies that `flick-vm` and `flick-vm-wasm` built for
//! `wasm32-unknown-unknown` pull in zero OS-only runtime dependencies
//! (`tokio`, `hyper`, `wasmtime`, `libffi`, `rustix`, `mio`).

#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process::Command;

const FORBIDDEN_WASM_DEPS: &[&str] = &["tokio", "hyper", "wasmtime", "libffi", "rustix", "mio"];

fn collect_wasm32_runtime_deps(package: &str, extra_args: &[&str]) -> BTreeSet<String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
    let output = Command::new(cargo)
        .args(["tree", "--manifest-path"])
        .arg(&manifest)
        .args([
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "-e",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .args(extra_args)
        .output()
        .expect("run cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed for {package} ({extra_args:?}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn wasm32_dependency_graph_contains_no_os_runtime_crates() {
    for (package, label, extra) in [
        ("flick-vm", "default features", &[][..]),
        (
            "flick-vm",
            "--no-default-features",
            &["--no-default-features"][..],
        ),
        ("flick-vm-wasm", "default features", &[][..]),
    ] {
        let deps = collect_wasm32_runtime_deps(package, extra);
        assert!(
            deps.contains(package),
            "expected {package} in resolved dependency graph ({label})"
        );
        for &forbidden in FORBIDDEN_WASM_DEPS {
            assert!(
                !deps.contains(forbidden),
                "forbidden OS crate `{forbidden}` found in {package} wasm32 dependency graph ({label}): {deps:?}"
            );
        }
    }
}
