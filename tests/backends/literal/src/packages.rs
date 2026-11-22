//! Package discovery, runtime resolution, and dependency validation for the
//! `test/literal` fixture backend.

use flick_backend::command;
use flick_backend::{
    Diagnostic, OptionValue, PackageRoot, Packages, ProjectFile, RuntimeGraph, RuntimeNode,
    Severity, Span,
};

pub(super) fn package_files() -> Vec<String> {
    vec![
        "package.json".into(),
        "node_modules/@*/*/package.json".into(),
        "literal-deps.json".into(),
        "literal_packages/*/manifest.json".into(),
    ]
}

pub(super) fn packages(
    files: &[ProjectFile],
    dependencies: &[command::Dependency],
    options: &[OptionValue],
) -> Packages {
    let option = |key: &str| {
        options
            .iter()
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    if option("inspect").is_some() || option("check-runtime").is_some() {
        return Packages {
            name: None,
            version: None,
            declared: vec![],
            sources: vec![],
            runtime: vec![],
            inputs: vec![],
            diagnostics: vec![],
            needs: vec![],
        };
    }
    let skip_runtime = option("skip-runtime") == Some("true");
    let (runtime, needs) = if skip_runtime {
        (Vec::new(), Vec::new())
    } else {
        resolve_runtime(
            files,
            option("root").unwrap_or_default(),
            option("boundary"),
        )
    };

    // Third-party literal ecosystem manifest (`literal-deps.json` + `literal_packages/*/manifest.json`)
    let literal_manifest: serde_json::Value = files
        .iter()
        .find(|f| f.path == "literal-deps.json" && !f.real.is_empty())
        .and_then(|f| serde_json::from_slice(&f.bytes).ok())
        .unwrap_or(serde_json::Value::Null);
    let declared_map = literal_manifest["dependencies"].as_object();
    let declared: Vec<String> = declared_map
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let installed_list: Vec<String> = literal_manifest["installed"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let (sources, inputs) = collect_literal_sources(files);
    let verify_installed = option("verify") == Some("installed");
    let diagnostics = check_literal_dependencies(
        dependencies,
        declared_map,
        &installed_list,
        &sources,
        verify_installed,
    );

    Packages {
        name: literal_manifest["name"].as_str().map(str::to_owned),
        version: literal_manifest["version"].as_str().map(str::to_owned),
        declared,
        sources,
        runtime,
        inputs,
        diagnostics,
        needs,
    }
}

fn literal_parent_dir(dir: &str) -> Option<&str> {
    let trimmed = if dir == "/" {
        "/"
    } else {
        dir.trim_end_matches('/')
    };
    let bytes = trimmed.as_bytes();
    let is_drive = (bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || (bytes.len() == 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/');
    let is_unc = trimmed.strip_prefix("//").is_some_and(|rest| {
        let mut parts = rest.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(s), Some(sh), None) if !s.is_empty() && !sh.is_empty())
    });
    if trimmed.is_empty() || trimmed == "/" || is_drive || is_unc {
        return None;
    }
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        Some("/")
    } else {
        Some(parent)
    }
}

fn resolve_runtime(
    files: &[ProjectFile],
    root: &str,
    boundary: Option<&str>,
) -> (Vec<RuntimeGraph>, Vec<String>) {
    let mut needs = Vec::new();
    let mut found = None;
    let boundary = boundary.map(|b| {
        if b == "/" {
            "/"
        } else {
            b.trim_end_matches('/')
        }
    });
    let mut dir = if root == "/" {
        "/"
    } else {
        root.trim_end_matches('/')
    };
    while !dir.is_empty() {
        if dir.rsplit('/').next() != Some("node_modules") {
            let path = format!(
                "{}/node_modules/@flick-lang/runtime/package.json",
                dir.trim_end_matches('/')
            );
            match files.iter().find(|f| f.path == path) {
                Some(file) if !file.real.is_empty() => {
                    found = Some(file).filter(|_| needs.is_empty());
                    break;
                }
                Some(_) => {}
                None => needs.push(path),
            }
        }
        if boundary.is_some_and(|b| dir == b) {
            break;
        }
        let Some(parent) = literal_parent_dir(dir) else {
            break;
        };
        dir = parent;
    }
    let runtime = found
        .and_then(|f| {
            let manifest: serde_json::Value = serde_json::from_slice(&f.bytes).ok()?;
            Some(RuntimeGraph {
                package: "@flick-lang/runtime".into(),
                nodes: vec![RuntimeNode {
                    name: manifest["name"].as_str()?.into(),
                    version: manifest["version"].as_str()?.into(),
                    dir: f.real.rsplit_once('/')?.0.into(),
                    dependencies: vec![],
                }],
                skip: vec!["node_modules".into()],
            })
        })
        .into_iter()
        .collect();
    (runtime, needs)
}

fn collect_literal_sources(files: &[ProjectFile]) -> (Vec<PackageRoot>, Vec<String>) {
    let mut sources = Vec::new();
    let mut inputs = vec!["literal-deps.json".into()];
    for file in files {
        if let Some(rest) = file.path.strip_prefix("literal_packages/")
            && let Some((pkg_dir, _)) = rest.split_once("/manifest.json")
            && !file.real.is_empty()
        {
            inputs.push(file.path.clone());
            if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&file.bytes)
                && let Some(source_dir) = data["source-dir"].as_str()
            {
                let pkg_name = data["name"].as_str().unwrap_or(pkg_dir).to_owned();
                let pkg_ver = data["version"].as_str().unwrap_or("0.1.0").to_owned();
                sources.push(PackageRoot {
                    package: pkg_name,
                    version: pkg_ver,
                    root: format!("literal_packages/{pkg_dir}/{source_dir}"),
                });
            }
        }
    }
    (sources, inputs)
}

fn check_literal_dependencies(
    dependencies: &[command::Dependency],
    declared_map: Option<&serde_json::Map<String, serde_json::Value>>,
    installed_list: &[String],
    sources: &[PackageRoot],
    verify_installed: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
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
        if d.name.is_empty() || d.name.contains(['/', '\\']) || d.name.contains("..") {
            diagnostics.push(error(
                "P032",
                format!(
                    "`[target.literal.dependencies]`: `{}` is not a valid literal package name",
                    d.name
                ),
            ));
            continue;
        }
        if d.version.trim().is_empty() || d.version.starts_with("bad:") {
            diagnostics.push(error(
                "P032",
                format!(
                    "`[target.literal.dependencies]`: `{} = \"{}\"` is not a valid literal package version",
                    d.name, d.version
                ),
            ));
            continue;
        }
        let written = declared_map.and_then(|m| m.get(&d.name));
        if let Some(ver) = written.and_then(serde_json::Value::as_str)
            && ver != d.version
        {
            let mut diag = error(
                "P040",
                format!(
                    "`literal-deps.json` names `{}` as `{ver}`, and {asker} needs `{}`",
                    d.name, d.version
                ),
            );
            diag.notes
                .push("make them agree on one version in `literal-deps.json`".into());
            diagnostics.push(diag);
            continue;
        }
        if verify_installed {
            let is_installed = installed_list.iter().any(|n| n == &d.name)
                || sources.iter().any(|s| s.package == d.name);
            if written.is_none() || !is_installed {
                diagnostics.push(error(
                    "P034",
                    format!(
                        "{asker} needs the literal package `{}`, which is not installed: run `flick install`",
                        d.name
                    ),
                ));
            }
        }
    }
    diagnostics
}
