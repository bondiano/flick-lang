//! The versions this compiler pins and how the installed tree compares:
//! another runtime, Effect adapter or Effect version stops a build before it writes
//! code against an ABI it does not know.

use std::path::Path;
use std::time::UNIX_EPOCH;
use std::{env, fs, io, process};

use flick_core::build;
use flick_core::codegen::go::PACKAGES;
use flick_core::diagnostics::{Diagnostic, Severity, codes};

pub const COMPILER: &str = env!("CARGO_PKG_VERSION");
pub const RUNTIME: &str = "0.1.0";
pub const EFFECT_TS: &str = "0.1.0";
pub const EFFECT: &str = "4.1.2";
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

/// The version of an installed package, from `root/node_modules`.
pub fn installed(root: &Path, package: &str) -> Option<String> {
    let path = root.join("node_modules").join(package).join("package.json");
    let text = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(json.get("version")?.as_str()?.to_owned())
}

/// The version of `package` that the Effect adapter resolves, then the project.
pub fn resolved(root: &Path, package: &str) -> Option<String> {
    let adapter_dir = root.join("node_modules/@flick-lang/effect-ts");
    if package == "effect" {
        installed(&adapter_dir, package).or_else(|| installed(root, package))
    } else {
        installed(root, package).or_else(|| installed(&adapter_dir, package))
    }
}

/// `P015` for an installed runtime or adapter of another version; `rt` needs
/// only the kernel package, while the Effect backend also pins its adapter.
pub fn check_runtime(root: &Path, rt: bool) -> Option<Diagnostic> {
    let packages = [("@flick-lang/runtime", RUNTIME)]
        .into_iter()
        .chain((!rt).then_some(("@flick-lang/effect-ts", EFFECT_TS)))
        .chain((!rt).then_some(("effect", EFFECT)));
    for (package, wanted) in packages {
        let Some(found) = resolved(root, package) else {
            continue;
        };
        if found != wanted {
            return Some(Diagnostic::new(
                codes::P015,
                Severity::Error,
                format!(
                    "the installed `{package}` is {found}, but flick {COMPILER} needs {wanted}: install `{package}@{wanted}`"
                ),
            ));
        }
    }
    None
}

/// Runs `go build` on the Go module of the entry `name` under `out_dir`,
/// writing its executable beside the module; `P029` when Go is missing or
/// the build fails.
pub fn go_build(out_dir: &Path, name: &str) -> Option<Diagnostic> {
    let dir = out_dir.join(build::go::module_dir(name));
    let mut command = process::Command::new("go");
    // without -trimpath the directory of a package keys Go's build cache: each
    // run generation and e2e case would compile the runtime anew and keep it
    command
        .args(["build", "-trimpath", "-o"])
        .arg(out_dir.join(build::go::executable(name)))
        .arg(".")
        .current_dir(&dir)
        // only optional packages (the PostgreSQL driver, …) require modules, which go downloads once
        .env("GOFLAGS", "-mod=mod")
        .env("GOTOOLCHAIN", "local");
    // the runtime needs no C: a static executable, built without a C compiler
    if env::var_os("CGO_ENABLED").is_none() {
        command.env("CGO_ENABLED", "0");
    }
    let output = command.output();
    let message = match output {
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            "the go target needs Go: `go` is not on PATH; install Go 1.23 or later".to_owned()
        }
        Err(err) => format!("cannot run `go build`: {err}"),
        Ok(out) if out.status.success() => return None,
        Ok(out) if missing_module(&dir, &out.stderr).is_some() => {
            let (package, module) = missing_module(&dir, &out.stderr).expect("matched");
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!(
                "`{package}` of the runtime needs {module}, which `go build` could not download: build once with network access, or fill Go's module cache with `go mod download` in `{}`\n{}",
                dir.display(),
                stderr.trim_end()
            )
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let lines: Vec<&str> = stderr.lines().collect();
            let tail = lines[lines.len().saturating_sub(20)..].join("\n");
            format!(
                "`go build` failed in `{}`, a bug of the compiler:\n{tail}",
                dir.display()
            )
        }
    };
    Some(Diagnostic::new(codes::P029, Severity::Error, message))
}

/// The optional package of the runtime in the Go module `dir` and the
/// module of it that `go build` names in `stderr`: one it could not fetch.
fn missing_module(dir: &Path, stderr: &[u8]) -> Option<(&'static str, &'static str)> {
    let stderr = String::from_utf8_lossy(stderr);
    PACKAGES
        .iter()
        .filter(|p| dir.join(p.name).is_dir())
        .flat_map(|p| p.modules.lines().map(move |m| (p.name, m)))
        .find(|(_, module)| stderr.contains(module))
}
