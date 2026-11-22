//! Runtime ABI and adapter contract verification plus `flick doctor` inspection
//! for the JavaScript backend.

use flick_backend::{Diagnostic, OptionValue, Severity};
use serde_json::Value;

use super::super::dependencies::PNPM;
use super::{Known, dir, installed_file, installed_version, resolve};

/// The runtime package ABI version this backend targets.
pub(super) const RUNTIME: &str = "0.1.0";
/// The package `effect` mode loads beside the runtime.
pub(super) const ADAPTER: &str = "@flick-lang/effect-ts";
/// Minimum Node.js version reported by `flick doctor`.
const NODE: &str = "24.18.0";
/// TypeScript release verified against `flick content-mapper`.
const TYPESCRIPT_GO: &str = "7.1.0-beta";
/// Workspace runtime manifest for resolving `"@flick-lang/runtime": "workspace:*"`.
const RUNTIME_MANIFEST: &str = include_str!("../../../../../runtime/package.json");

/// Exact production dependency pins of an installed `@flick-lang/effect-ts`.
pub(super) fn adapter_requirements(known: &Known<'_>) -> Result<Vec<(String, String)>, String> {
    let adapter_path = format!("node_modules/{ADAPTER}/package.json");
    let Some(file) = installed_file(known, ADAPTER) else {
        if known.files.contains_key(adapter_path.as_str()) {
            return Err(format!(
                "cannot read `{adapter_path}`: missing or unreadable package manifest"
            ));
        }
        return Ok(Vec::new());
    };
    let json: Value = serde_json::from_slice(&file.bytes)
        .map_err(|error| format!("invalid `{}`: {error}", file.path))?;
    if json.get("version").and_then(Value::as_str).is_none() {
        return Err(format!("`{ADAPTER}` has no package version"));
    }
    let deps = json
        .get("dependencies")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`{ADAPTER}` has no dependency contract"))?;
    if !deps.contains_key("effect") {
        return Err(format!("`{ADAPTER}` has no `effect` requirement"));
    }
    deps.iter()
        .map(|(name, value)| {
            if name == "@flick-lang/runtime" && value.as_str() == Some("workspace:*") {
                let runtime: Value =
                    serde_json::from_str(RUNTIME_MANIFEST).expect("runtime manifest");
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

/// `P015` for a runtime ABI mismatch or an unsatisfied installed adapter contract.
pub(super) fn check_runtime_diagnostics(
    known: &mut Known<'_>,
    rt: bool,
    compiler: &str,
    boundary: Option<&str>,
) -> Vec<Diagnostic> {
    let error = |message: String| Diagnostic {
        code: "P015".into(),
        severity: Severity::Error,
        message,
        span: None,
        notes: Vec::new(),
    };
    if let Some(found) = installed_version(known, "@flick-lang/runtime")
        && found != RUNTIME
    {
        return vec![error(format!(
            "the installed `@flick-lang/runtime` is {found}, but flick {compiler} needs {RUNTIME}: install `@flick-lang/runtime@{RUNTIME}`"
        ))];
    }
    if rt {
        return Vec::new();
    }
    let requirements = match adapter_requirements(known) {
        Ok(requirements) => requirements,
        Err(message) => return vec![error(message)],
    };
    let Some(adapter_file) = installed_file(known, ADAPTER) else {
        return Vec::new();
    };
    let adapter_dir = dir(&adapter_file.real).to_owned();
    let mut out = Vec::new();
    for (package, wanted) in requirements {
        if package == "@flick-lang/runtime" && wanted != RUNTIME {
            out.push(error(format!(
                "`{ADAPTER}` needs runtime {wanted}, but flick {compiler} supports runtime {RUNTIME}"
            )));
            return out;
        }
        if let Ok(found) = resolve(known, &adapter_dir, &package, false, boundary) {
            let version = found
                .and_then(|f| serde_json::from_slice::<Value>(&f.bytes).ok())
                .and_then(|v| v.get("version")?.as_str().map(str::to_owned));
            if version.as_deref() != Some(wanted.as_str()) {
                out.push(error(format!(
                    "`{ADAPTER}` needs `{package}@{wanted}`, but resolves {}: run `flick install`",
                    version.as_deref().unwrap_or("no installed package")
                )));
                return out;
            }
        }
    }
    out
}

fn push_doctor(lines: &mut Vec<Diagnostic>, ok: Option<bool>, what: &str, detail: &str) {
    let (code, severity) = match ok {
        Some(true) => ("doctor-ok", Severity::Note),
        Some(false) => ("doctor-fail", Severity::Error),
        None => ("doctor-note", Severity::Note),
    };
    lines.push(Diagnostic {
        code: code.into(),
        severity,
        message: format!("{what}\n{detail}"),
        span: None,
        notes: Vec::new(),
    });
}

/// Structured lines for `flick doctor`, encoded as diagnostics with codes
/// `doctor-ok`, `doctor-fail` and `doctor-note` and message `<what>\n<detail>`.
pub(super) fn doctor_diagnostics(
    known: &mut Known<'_>,
    project: &Value,
    options: &[OptionValue],
    boundary: Option<&str>,
) -> Vec<Diagnostic> {
    let option = |key: &str| {
        (options.iter())
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    let compiler = option("compiler").unwrap_or("0.1.0");
    let has_project = option("has-project") == Some("true");
    let node_ver = option("node-version").filter(|s| !s.is_empty());
    let pnpm_ver = option("pnpm-version").filter(|s| !s.is_empty());

    let mut lines = Vec::new();
    match node_ver {
        Some(v) => push_doctor(
            &mut lines,
            Some(at_least(v, NODE)),
            "Node.js",
            &format!("{v} (needs >= {NODE})"),
        ),
        None => push_doctor(
            &mut lines,
            Some(false),
            "Node.js",
            &format!("not found (needs >= {NODE})"),
        ),
    }

    let pinned = project["packageManager"]
        .as_str()
        .and_then(|spec| spec.strip_prefix("pnpm@"))
        .map_or(PNPM, |v| v.split('+').next().unwrap_or(v));
    match pnpm_ver {
        Some(v) => push_doctor(
            &mut lines,
            Some(v == pinned),
            "pnpm",
            &format!("{v} (the project pins {pinned})"),
        ),
        None => push_doctor(
            &mut lines,
            Some(false),
            "pnpm",
            &format!("not found (the project pins {pinned})"),
        ),
    }

    if has_project {
        doctor_project_lines(known, compiler, boundary, &mut lines);
    } else {
        push_doctor(
            &mut lines,
            None,
            "project",
            "none here: runtime and adapters not checked",
        );
        push_doctor(
            &mut lines,
            None,
            "Content Mapper",
            "typescript not installed: editors use `flick lsp` only",
        );
    }

    lines
}

fn doctor_project_lines(
    known: &mut Known<'_>,
    compiler: &str,
    boundary: Option<&str>,
    lines: &mut Vec<Diagnostic>,
) {
    for problem in check_runtime_diagnostics(known, false, compiler, boundary) {
        push_doctor(lines, Some(false), "package contract", &problem.message);
    }
    let mut requirements = vec![("@flick-lang/runtime".to_owned(), RUNTIME.to_owned())];
    match adapter_requirements(known) {
        Ok(deps) => requirements.extend(deps),
        Err(message) => push_doctor(lines, Some(false), ADAPTER, &message),
    }
    let adapter_dir = installed_file(known, ADAPTER).map(|f| dir(&f.real).to_owned());
    if installed_version(known, ADAPTER).is_none() {
        let detail = installed_version(known, "effect").unwrap_or_else(|| {
            "not installed; requirements come from the adapter package".to_owned()
        });
        push_doctor(lines, None, "effect", &detail);
    }
    let adapter_detail =
        installed_version(known, ADAPTER).unwrap_or_else(|| "not installed".to_owned());
    push_doctor(lines, None, ADAPTER, &adapter_detail);
    for (package, wanted) in requirements {
        let found = if package == ADAPTER || package == "@flick-lang/runtime" {
            installed_version(known, &package)
        } else if let Some(ref adir) = adapter_dir {
            match resolve(known, adir, &package, false, boundary) {
                Ok(Some(f)) => serde_json::from_slice::<Value>(&f.bytes)
                    .ok()
                    .and_then(|v| v.get("version")?.as_str().map(str::to_owned)),
                _ => None,
            }
        } else {
            None
        };
        match found {
            Some(v) => push_doctor(
                lines,
                Some(v == wanted),
                &package,
                &format!("{v} (needs {wanted})"),
            ),
            None => push_doctor(
                lines,
                None,
                &package,
                &format!("not installed; `flick install` (needs {wanted})"),
            ),
        }
    }
    for (package, what) in [
        ("pg", "PostgreSQL driver of `Database.pg`"),
        ("react", "React adapter of `Ui.Dom.mount`"),
    ] {
        let found = installed_version(known, package).or_else(|| {
            let adir = adapter_dir.as_deref()?;
            let nested = format!("{adir}/node_modules/{package}/package.json");
            match known.get(&nested) {
                Ok(Some(f)) => serde_json::from_slice::<Value>(&f.bytes)
                    .ok()
                    .and_then(|v| v.get("version")?.as_str().map(str::to_owned)),
                _ => None,
            }
        });
        let detail = found.map_or_else(
            || format!("not installed: the {what} is unavailable"),
            |v| format!("{v}, the {what}"),
        );
        push_doctor(lines, None, package, &detail);
    }
    let mapper = installed_version(known, "typescript");
    let detail = match mapper {
        Some(v) if v == TYPESCRIPT_GO => format!("typescript {v}, the mapper's release"),
        Some(v) => format!("typescript {v}: the mapper is verified against {TYPESCRIPT_GO}"),
        None => "typescript not installed: editors use `flick lsp` only".to_owned(),
    };
    push_doctor(lines, None, "Content Mapper", &detail);
}

fn at_least(found: &str, wanted: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(found) >= parse(wanted)
}
