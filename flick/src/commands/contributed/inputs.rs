//! The paths a contributed command names, relative to the project's root:
//! its path arguments, what its descriptor says it reads and writes, and
//! the files the host hands it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use flick_backend::command::{Argument, Descriptor, File, Invocation, Parameter, ValueKind};
use flick_core::project;

/// The bytes of input files one invocation may get.
const BUDGET: u64 = 64 << 20;

/// `arguments` with each value of a `path` parameter relative to `root`,
/// read against the working directory `cwd`.
pub(super) fn relative(
    descriptor: &Descriptor,
    arguments: Vec<Argument>,
    root: &Path,
    cwd: &Path,
) -> Result<Vec<Argument>, String> {
    let paths: Vec<&Parameter> = (descriptor.positionals.iter())
        .chain(&descriptor.options)
        .filter(|p| p.kind == ValueKind::Path)
        .collect();
    let mut out = Vec::new();
    for mut argument in arguments {
        if paths.iter().any(|p| p.name == argument.name) {
            for value in &mut argument.values {
                let full = project::normalize(&cwd.join(&*value));
                let inside = full.strip_prefix(root).map_err(|_| {
                    format!("`{value}` of `--{}` is outside the project", argument.name)
                })?;
                *value = slashed(inside);
            }
        }
        out.push(argument);
    }
    Ok(out)
}

/// `path` with `/` between its parts.
pub(super) fn slashed(path: &Path) -> String {
    let parts: Vec<String> = (path.components())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

/// The paths `patterns` names: each a path, `$name` for the values of the
/// path argument `name`, `$name/..` for the directories they are in, or
/// `$sources` for the source root.
pub(super) fn expand(patterns: &[String], invocation: &Invocation) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for pattern in patterns {
        match pattern.strip_prefix('$') {
            Some("sources") => paths.push(invocation.sources.clone()),
            Some(name) => {
                // `$name/..`: the directory each value of `name` is in
                let (name, parent) = name
                    .strip_suffix("/..")
                    .map_or((name, false), |n| (n, true));
                let values = (invocation.arguments.iter())
                    .filter(|a| a.name == name)
                    .flat_map(|a| a.values.iter());
                paths.extend(values.map(|v| match v.rsplit_once('/') {
                    Some((dir, _)) if parent => dir.to_owned(),
                    None if parent => String::new(),
                    _ => v.clone(),
                }));
            }
            None => paths.push(pattern.clone()),
        }
    }
    if let Some(path) = paths.iter().find(|p| !normal(p)) {
        return Err(format!("the declared path `{path}` leaves the project"));
    }
    Ok(paths)
}

/// Whether `path` stays inside the directory it is relative to.
pub(super) fn normal(path: &str) -> bool {
    (Path::new(path).components()).all(|c| matches!(c, Component::Normal(_)))
}

/// Whether `path` is one of `within` or under one; the empty path is the
/// whole project.
pub(super) fn under(path: &str, within: &[String]) -> bool {
    within.iter().any(|w| {
        w.is_empty()
            || path == w
            || path
                .strip_prefix(w.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })
}

/// The regular files at `paths` under `root`, a directory standing for
/// those under it outside `node_modules` and hidden directories. A path
/// that does not exist gives nothing.
pub(super) fn read(root: &Path, paths: &[String]) -> Result<Vec<File>, String> {
    let mut files = BTreeMap::new();
    let mut size = 0;
    for path in paths {
        collect(root, path, &mut files, &mut size)?;
    }
    Ok(files.into_values().collect())
}

fn collect(
    root: &Path,
    path: &str,
    files: &mut BTreeMap<String, File>,
    size: &mut u64,
) -> Result<(), String> {
    let full = root.join(path);
    let Ok(meta) = fs::symlink_metadata(&full) else {
        return Ok(());
    };
    if meta.is_file() {
        *size += meta.len();
        if *size > BUDGET {
            return Err(format!("its inputs exceed {} MiB", BUDGET >> 20));
        }
        let bytes = fs::read(&full).map_err(|e| format!("cannot read `{path}`: {e}"))?;
        files.insert(
            path.to_owned(),
            File {
                path: path.to_owned(),
                bytes,
                executable: false,
            },
        );
    } else if meta.is_dir() {
        let entries = fs::read_dir(&full).map_err(|e| format!("cannot read `{path}`: {e}"))?;
        let mut names: Vec<String> = (entries.flatten())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "node_modules" && !n.starts_with('.'))
            .collect();
        names.sort();
        for name in names {
            let child = if path.is_empty() {
                name
            } else {
                format!("{path}/{name}")
            };
            collect(root, &child, files, size)?;
        }
    }
    Ok(())
}
