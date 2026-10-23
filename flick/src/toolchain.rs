//! The versions this compiler pins and how the installed tree compares:
//! another `@flick-lang/runtime` or `effect` stops a build before it writes
//! code against an ABI it does not know.

use std::fs;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};

pub const COMPILER: &str = env!("CARGO_PKG_VERSION");
pub const RUNTIME: &str = "0.1.0";
pub const EFFECT: &str = "4.0.5";
pub const PNPM: &str = "12.14.0";
pub const NODE: &str = "24.18.0";

/// The version of an installed package, from `root/node_modules`.
pub fn installed(root: &Path, package: &str) -> Option<String> {
    let path = root.join("node_modules").join(package).join("package.json");
    let text = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(json.get("version")?.as_str()?.to_owned())
}

/// `P015` for an installed runtime or Effect of another version; nothing
/// when they are absent, which only running needs.
pub fn check_runtime(root: &Path) -> Option<Diagnostic> {
    let runtime_dir = root.join("node_modules/@flick-lang/runtime");
    for (package, wanted, base) in [
        ("@flick-lang/runtime", RUNTIME, root.to_path_buf()),
        ("effect", EFFECT, runtime_dir.clone()),
    ] {
        let Some(found) = installed(&base, package).or_else(|| installed(root, package)) else {
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
