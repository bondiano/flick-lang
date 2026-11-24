//! Go module package reporting and `[target.go.dependencies]` validation
//! for `flick/go`.

use std::collections::BTreeMap;
use std::str;

use flick_backend::command::Dependency;
use flick_backend::{Diagnostic, OptionValue, Packages, ProjectFile, Severity, Span};

pub(super) fn files() -> Vec<String> {
    vec!["go.mod".into(), "go.sum".into()]
}

pub(super) fn packages(
    files: &[ProjectFile],
    dependencies: &[Dependency],
    options: &[OptionValue],
) -> Packages {
    let option = |key: &str| {
        options
            .iter()
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    if matches!(option("inspect"), Some("doctor-packages" | "doctor"))
        || option("check-runtime").is_some()
    {
        return empty_packages();
    }

    let go_mod_bytes = files
        .iter()
        .find(|f| f.path == "go.mod" && !f.real.is_empty())
        .map(|f| f.bytes.as_slice());

    let mut diagnostics = Vec::new();
    let (module_name, mod_requires) = match go_mod_bytes {
        Some(bytes) => match str::from_utf8(bytes) {
            Ok(text) => parse_go_mod(text),
            Err(err) => {
                diagnostics.push(Diagnostic {
                    code: "P005".into(),
                    severity: Severity::Error,
                    message: format!("invalid `go.mod`: {err}"),
                    span: None,
                    notes: Vec::new(),
                });
                (None, BTreeMap::new())
            }
        },
        None => (None, BTreeMap::new()),
    };

    for (i, d) in dependencies.iter().enumerate() {
        let error = |code: &str, message: String| Diagnostic {
            code: code.into(),
            severity: Severity::Error,
            message,
            span: u32::try_from(i).ok().map(|file| Span {
                file,
                start: 0,
                end: 0,
            }),
            notes: Vec::new(),
        };
        let asker =
            d.by.as_ref()
                .map_or_else(|| "the project".to_owned(), |by| format!("`{by}`"));
        if !is_go_module_path(&d.name) {
            diagnostics.push(error(
                "P032",
                format!(
                    "`[target.go.dependencies]`: `{}` is not a valid Go module path (expected a domain-qualified path such as `example.com/pkg`)",
                    d.name
                ),
            ));
            continue;
        }
        if !is_go_module_version(&d.version) {
            diagnostics.push(error(
                "P032",
                format!(
                    "`[target.go.dependencies]`: `{} = \"{}\"` is not a canonical Go module version (expected `vMAJOR.MINOR.PATCH`, such as `v1.2.3`)",
                    d.name, d.version
                ),
            ));
            continue;
        }
        if let Some(existing) = mod_requires.get(&d.name)
            && existing != &d.version
        {
            let mut conflict = error(
                "P040",
                format!(
                    "`go.mod` requires Go module `{}` at `{existing}`, and {asker} needs `{}`",
                    d.name, d.version
                ),
            );
            conflict.notes.push(
                "make them agree on one version, or use versions of the packages that do".into(),
            );
            diagnostics.push(conflict);
        }
    }

    let declared: Vec<String> = mod_requires.keys().cloned().collect();
    Packages {
        name: module_name,
        version: None,
        declared,
        sources: Vec::new(),
        runtime: Vec::new(),
        inputs: vec!["go.mod".into(), "go.sum".into()],
        diagnostics,
        needs: Vec::new(),
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

/// Parses `module <name>` and `require` lines from a `go.mod` file.
fn parse_go_mod(text: &str) -> (Option<String>, BTreeMap<String, String>) {
    let mut name = None;
    let mut requires = BTreeMap::new();
    let mut in_require = false;
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or(raw).trim();
        if line.is_empty() {
            continue;
        }
        if in_require {
            if line == ")" {
                in_require = false;
                continue;
            }
            if let Some((path, ver)) = split_require(line) {
                requires.insert(path.to_owned(), ver.to_owned());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("module ") {
            let trimmed = rest.trim().trim_matches('"');
            if !trimmed.is_empty() {
                name = Some(trimmed.to_owned());
            }
        } else if line == "require (" || line.starts_with("require (") {
            in_require = true;
        } else if let Some(rest) = line.strip_prefix("require ")
            && let Some((path, ver)) = split_require(rest)
        {
            requires.insert(path.to_owned(), ver.to_owned());
        }
    }
    (name, requires)
}

fn split_require(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let path = parts.next()?.trim_matches('"');
    let ver = parts.next()?;
    (!path.is_empty() && !ver.is_empty()).then_some((path, ver))
}

/// Validates a third-party Go module path (per Go `module.CheckPath` basics:
/// domain-qualified first element with `.`, safe slash-separated segments).
pub(super) fn is_go_module_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return false;
    }
    let mut segments = path.split('/');
    let Some(first) = segments.next() else {
        return false;
    };
    if !first.contains('.') || !valid_segment(first, false) {
        return false;
    }
    segments.all(|seg| valid_segment(seg, true))
}

fn valid_segment(seg: &str, allow_tilde: bool) -> bool {
    if seg.is_empty()
        || seg == "."
        || seg == ".."
        || seg.starts_with(['.', '-'])
        || seg.ends_with('.')
    {
        return false;
    }
    seg.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_') || (allow_tilde && b == b'~')
    })
}

/// Validates a canonical Go module semantic version (`vMAJOR.MINOR.PATCH[-pre][+build]`).
pub(super) fn is_go_module_version(version: &str) -> bool {
    let Some(rest) = version.strip_prefix('v') else {
        return false;
    };
    let (core_and_pre, build) = match rest.split_once('+') {
        Some((before, build)) => (before, Some(build)),
        None => (rest, None),
    };
    if let Some(build) = build
        && (build.is_empty()
            || !build.split('.').all(|seg| {
                !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
    {
        return false;
    }
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((before, pre)) => (before, Some(pre)),
        None => (core_and_pre, None),
    };
    if let Some(pre) = pre
        && (pre.is_empty()
            || !pre.split('.').all(|seg| {
                !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
    {
        return false;
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.into_iter().all(|num| {
        !num.is_empty()
            && num.bytes().all(|b| b.is_ascii_digit())
            && (num.len() == 1 || !num.starts_with('0'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_go_module_paths_and_versions() {
        assert!(is_go_module_path("example.com/greet"));
        assert!(is_go_module_path("github.com/google/uuid"));
        assert!(is_go_module_path("gopkg.in/yaml.v3"));
        assert!(is_go_module_path("example.com/foo/bar/v2"));
        assert!(!is_go_module_path(""));
        assert!(!is_go_module_path("math"));
        assert!(!is_go_module_path("../escape"));
        assert!(!is_go_module_path("example.com//bad"));
        assert!(!is_go_module_path("example.com/bad:port"));

        assert!(is_go_module_version("v1.0.0"));
        assert!(is_go_module_version("v0.0.0-20250101000000-0123456789ab"));
        assert!(is_go_module_version("v2.3.4+incompatible"));
        assert!(!is_go_module_version("1.0.0"));
        assert!(!is_go_module_version("^1.0.0"));
        assert!(!is_go_module_version("v1.0"));
        assert!(!is_go_module_version("v01.0.0"));
    }

    #[test]
    fn detects_go_mod_conflict_and_invalid_dependencies() {
        let go_mod = ProjectFile {
            path: "go.mod".into(),
            real: "/app/go.mod".into(),
            bytes: b"module app\n\ngo 1.23\n\nrequire (\n\texample.com/a v1.0.0\n)\n".to_vec(),
        };
        let deps = vec![
            Dependency {
                name: "example.com/a".into(),
                version: "v2.0.0".into(),
                by: Some("mylib".into()),
            },
            Dependency {
                name: "not-a-domain".into(),
                version: "v1.0.0".into(),
                by: None,
            },
            Dependency {
                name: "example.com/b".into(),
                version: "^1.0.0".into(),
                by: None,
            },
        ];
        let report = packages(&[go_mod], &deps, &[]);
        assert_eq!(report.diagnostics.len(), 3, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, "P040");
        assert_eq!(report.diagnostics[1].code, "P032");
        assert_eq!(report.diagnostics[2].code, "P032");
    }
}
