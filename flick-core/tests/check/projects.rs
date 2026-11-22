//! Project-level fixture harness for `tests/check/projects/`.

use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_core::diagnostics::Diagnostic;
use flick_core::project::{self, Foreign, ForeignPackage, Host, Project};
use flick_core::source::SourceMap;
use flick_core::types;
use flick_test_support::{files as fixture_files, par_map};

use super::{annotations, diff, opted_out};

pub(super) struct FixtureHost {
    pub(super) root: PathBuf,
}

impl Host for FixtureHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// Simulates the JS backend's package report for `tests/check/projects/`
    /// fixtures that include a `package.json` (real CLI runs use the WASM
    /// backend; `flick-core` tests have no backend loaded).
    fn foreign(&self, project: &Project) -> Foreign {
        let Ok(bytes) = fs::read(project.root.join("package.json")) else {
            return Foreign::default();
        };
        let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Foreign::default();
        };
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let mut declared = Vec::new();
        for section in ["dependencies", "devDependencies"] {
            if let Some(table) = manifest.get(section).and_then(serde_json::Value::as_object) {
                for dep in table.keys() {
                    if !declared.contains(dep) {
                        declared.push(dep.clone());
                    }
                }
            }
        }
        let packages = declared
            .iter()
            .map(|dep| {
                let dir = project.root.join("node_modules").join(dep);
                let installed = fs::read(dir.join("package.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
                let pkg_version = installed
                    .as_ref()
                    .and_then(|m| m.get("version").and_then(serde_json::Value::as_str))
                    .map(str::to_owned);
                let source_dir = installed
                    .as_ref()
                    .and_then(|m| {
                        m.get("flick")
                            .and_then(|f| f.get("source-dir"))
                            .and_then(serde_json::Value::as_str)
                    })
                    .map(|src| PathBuf::from("node_modules").join(dep).join(src));
                ForeignPackage {
                    name: dep.clone(),
                    version: pkg_version,
                    source_dir,
                }
            })
            .collect();
        Foreign {
            name,
            version,
            declared,
            packages,
            diagnostics: Vec::new(),
        }
    }
}

/// Checks one fixture project the way `flick check` without paths does.
fn check_project(dir: &Path) -> Vec<String> {
    let host = FixtureHost {
        root: dir.to_path_buf(),
    };
    let mut sources = SourceMap::default();
    let diagnostics = match project::discover(&host, &mut sources, dir, Some(Path::new("."))) {
        Ok(Some(project)) => {
            let mut roots = Vec::new();
            roots.extend(fixture_files(&project.source_root(), "fl"));
            roots.extend(fixture_files(&project.test_root(), "fl"));
            project::check(&host, &mut sources, Some(&project), &roots)
        }
        Ok(None) => vec![],
        Err(diagnostic) => vec![diagnostic],
    };
    let diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|d| !opted_out(&sources, d))
        .collect();
    let mut files = fixture_files(dir, "fl");
    files.push(dir.join("flick.toml"));
    files.sort();
    let mut report = Vec::new();
    let mut expected_any = false;
    let mut accounted = vec![false; diagnostics.len()];
    for path in files {
        let shown = host.display(&path);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let prefix = if path.extension().is_some_and(|e| e == "toml") {
            "#"
        } else {
            "//"
        };
        let expected = match annotations(&text, prefix) {
            Ok(expected) => expected,
            Err(e) => {
                report.push(format!("{shown}: bad annotation: {e}"));
                continue;
            }
        };
        expected_any |= !expected.is_empty();
        let mut actual = Vec::new();
        for (i, d) in diagnostics.iter().enumerate() {
            // spanless diagnostics are annotated in `flick.toml`
            let file = d.span.map(|s| sources.get(s.file).path.clone());
            if file.as_deref().unwrap_or("flick.toml") == shown {
                accounted[i] = true;
                actual.push(d);
            }
        }
        let problems = diff(&sources, &actual, expected);
        if !problems.is_empty() {
            report.push(format!("{shown}:\n{}", problems.join("\n")));
        }
    }
    for (d, _) in diagnostics.iter().zip(&accounted).filter(|(_, a)| !**a) {
        report.push(format!(
            "  unexpected in an unannotated file: {} {}",
            d.code, d.message
        ));
    }
    if !expected_any && report.is_empty() && !diagnostics.is_empty() {
        report.push("  project has diagnostics but no annotations".into());
    }
    report
}

#[test]
fn project_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/projects");
    let mut cases: Vec<PathBuf> = fs::read_dir(&root)
        .expect("tests/check/projects exists")
        .map(|e| e.expect("readable").path())
        .filter(|p| p.is_dir())
        .collect();
    cases.sort();
    assert!(!cases.is_empty());
    let _ = types::builtins();
    let report: Vec<String> = par_map(&cases, |case| {
        let problems = check_project(&project::normalize(case));
        let name = case.file_name().expect("named").to_string_lossy();
        (!problems.is_empty()).then(|| format!("{name}:\n{}", problems.join("\n")))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(
        report.is_empty(),
        "project fixture mismatches:\n{}",
        report.join("\n")
    );
}
