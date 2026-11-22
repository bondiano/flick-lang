//! What a backend's packages give a project: the backend names the files it
//! reads, the host hands them over with their canonical paths, reads the
//! further ones each answer needs, and pins the contents of the runtime
//! graphs the backend resolves. The host knows nothing of the ecosystem: it
//! matches paths, reads files and hashes directories.

mod scope;
#[cfg(test)]
pub(super) mod tests;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_backend::command::Dependency;
use flick_backend::{
    Backend, Diagnostic, OptionValue, PackageRoot, Packages, ProjectFile, RuntimeDependency,
    RuntimeGraph, sha256,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use scope::{ancestor_ceiling, is_allowed_need, record_supplied};
pub(crate) use scope::{
    canonical_slashed, is_safe_pattern, is_safe_relative, normalize_canonical_str,
};

/// How many times one report may ask for more files: a level of a runtime
/// graph takes one.
const ROUNDS: usize = 128;

/// Maximum files accumulated across all rounds of one report.
const MAX_FILES: usize = 16_384;

/// The backend's packages, with each runtime graph pinned.
pub(crate) struct Report {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) declared: Vec<String>,
    pub(crate) sources: Vec<PackageRoot>,
    pub(crate) runtime: Vec<RuntimeDependency>,
    pub(crate) inputs: Vec<String>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Asks `backend` for the packages of the project at `root`.
pub(crate) fn report(
    backend: &dyn Backend,
    root: &Path,
    dependencies: &[Dependency],
    options: &[OptionValue],
) -> Result<Report, String> {
    report_in(backend, root, root, dependencies, options)
}

/// Like [`report`], using `scope_root` to bound ancestor lookups (for nested
/// importers inside a build output directory).
pub(crate) fn report_in(
    backend: &dyn Backend,
    root: &Path,
    scope_root: &Path,
    dependencies: &[Dependency],
    options: &[OptionValue],
) -> Result<Report, String> {
    let patterns = backend.package_files();
    if patterns.is_empty() {
        return Ok(Report {
            name: None,
            version: None,
            declared: Vec::new(),
            sources: Vec::new(),
            runtime: Vec::new(),
            inputs: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    for pattern in &patterns {
        if !is_safe_pattern(pattern) {
            return Err(format!(
                "the backend named an invalid package file pattern `{pattern}`"
            ));
        }
    }
    let names: BTreeSet<&str> = patterns
        .iter()
        .filter_map(|p| p.rsplit('/').next())
        .collect();
    let top = top_dirs(&patterns);
    let real_root = PathBuf::from(canonical_slashed(root));
    let real_scope = PathBuf::from(canonical_slashed(scope_root));
    let scope_ceiling = {
        let candidate = ancestor_ceiling(&real_scope, &top);
        if real_root.starts_with(&candidate) {
            candidate
        } else {
            ancestor_ceiling(&real_root, &top)
        }
    };
    let mut call_options = options.to_vec();
    if !call_options.iter().any(|o| o.key == "boundary") {
        call_options.push(OptionValue {
            key: "boundary".into(),
            value: slashed(&scope_ceiling),
        });
    }
    let mut supplied_bases = BTreeSet::from([(real_root.clone(), scope_ceiling.clone())]);
    let mut supplied_node_dirs = BTreeSet::new();
    let mut found = Vec::new();
    for pattern in &patterns {
        matching(root, Path::new(""), pattern, true, &mut found);
    }
    let mut files: Vec<ProjectFile> = (found.into_iter())
        .take(MAX_FILES)
        .map(|path| read(&root.join(&path), slashed(&path)))
        .collect();
    for file in &files {
        record_supplied(
            file,
            &scope_ceiling,
            &top,
            &mut supplied_bases,
            &mut supplied_node_dirs,
        );
    }
    for _ in 0..ROUNDS {
        let answer = backend.packages(&files, dependencies, &call_options);
        if answer.needs.is_empty() {
            return finalize_report(
                answer,
                &real_root,
                &patterns,
                &names,
                &supplied_bases,
                &supplied_node_dirs,
            );
        }
        for need in answer.needs {
            if files.len() >= MAX_FILES {
                return Err(format!(
                    "the backend asked for more than {MAX_FILES} package files"
                ));
            }
            if files.iter().any(|f| f.path == need) {
                return Err(format!("the backend asked again for `{need}`"));
            }
            if !is_allowed_need(&need, &real_root, &patterns, &names, &supplied_bases) {
                return Err(format!(
                    "the backend asked for `{need}`, which is outside its package scope"
                ));
            }
            let file = read(&root.join(&need), need);
            record_supplied(
                &file,
                &scope_ceiling,
                &top,
                &mut supplied_bases,
                &mut supplied_node_dirs,
            );
            files.push(file);
        }
    }
    Err(format!("the backend asked for more files {ROUNDS} times"))
}

fn finalize_report(
    answer: Packages,
    real_root: &Path,
    patterns: &[String],
    names: &BTreeSet<&str>,
    supplied_bases: &BTreeSet<(PathBuf, PathBuf)>,
    supplied_node_dirs: &BTreeSet<String>,
) -> Result<Report, String> {
    for source in &answer.sources {
        if !source.root.is_empty() && !is_safe_relative(&source.root) {
            return Err(format!(
                "the backend reported a package root outside the project: `{}`",
                source.root
            ));
        }
    }
    for input in &answer.inputs {
        if !is_safe_relative(input)
            && !is_allowed_need(input, real_root, patterns, names, supplied_bases)
        {
            return Err(format!(
                "the backend reported a build input outside its package scope: `{input}`"
            ));
        }
    }
    let runtime = answer
        .runtime
        .iter()
        .map(|g| pin(g, supplied_node_dirs))
        .collect::<Result<_, _>>()?;
    Ok(Report {
        name: answer.name,
        version: answer.version,
        declared: answer.declared,
        sources: answer.sources,
        runtime,
        inputs: answer.inputs,
        diagnostics: answer.diagnostics,
    })
}

/// Whether any file under `root` matches one of `patterns`.
pub(crate) fn any_match(root: &Path, patterns: &[String]) -> bool {
    let mut files = Vec::new();
    patterns.iter().filter(|p| is_safe_pattern(p)).any(|p| {
        matching(root, Path::new(""), p, false, &mut files);
        !files.is_empty()
    })
}

/// The files under `root` that `patterns` name, relative to it, and those a
/// pattern without `*` names though absent: creating one matters too.
pub(crate) fn named(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for pattern in patterns.iter().filter(|p| is_safe_pattern(p)) {
        if pattern.contains('*') {
            matching(root, Path::new(""), pattern, false, &mut files);
        } else {
            files.push(PathBuf::from(pattern));
        }
    }
    files
}

/// Unique top-level directory names referenced by `patterns` (e.g. `node_modules`).
pub(crate) fn top_dirs(patterns: &[String]) -> Vec<String> {
    let mut dirs = BTreeSet::new();
    for pattern in patterns.iter().filter(|p| is_safe_pattern(p)) {
        if let Some((first, _)) = pattern.split_once('/')
            && !first.is_empty()
            && !first.contains('*')
        {
            dirs.insert(first.to_owned());
        }
    }
    dirs.into_iter().collect()
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The file at `path`, asked for as `asked`; an absent one has no path.
fn read(path: &Path, asked: String) -> ProjectFile {
    match (fs::read(path), fs::canonicalize(path)) {
        (Ok(bytes), Ok(real)) => ProjectFile {
            path: asked,
            real: normalize_canonical_str(&real.to_string_lossy()),
            bytes,
        },
        _ => ProjectFile {
            path: asked,
            real: String::new(),
            bytes: Vec::new(),
        },
    }
}

/// The files under `root` that `pattern` names, relative to it, `*` standing
/// for any one name, part of a name with a prefix or suffix, as `@*`. When
/// `include_missing_in_dir` is set, a leaf entry under an existing matched
/// subdirectory is also reported as absent so a backend can tell a broken or
/// empty package directory from one that is not installed.
fn matching(
    root: &Path,
    at: &Path,
    pattern: &str,
    include_missing_in_dir: bool,
    out: &mut Vec<PathBuf>,
) {
    let (part, rest) = pattern
        .split_once('/')
        .map_or((pattern, None), |(p, r)| (p, Some(r)));
    let mut names: Vec<String> = match part.split_once('*') {
        None => vec![part.to_owned()],
        Some((prefix, suffix)) => (fs::read_dir(root.join(at)).into_iter().flatten().flatten())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                n.len() >= prefix.len() + suffix.len()
                    && n.starts_with(prefix)
                    && n.ends_with(suffix)
            })
            .collect(),
    };
    names.sort();
    for name in names {
        let path = at.join(&name);
        if let Some(rest) = rest {
            matching(root, &path, rest, include_missing_in_dir, out);
        } else if root.join(&path).is_file()
            || (include_missing_in_dir
                && !at.as_os_str().is_empty()
                && fs::symlink_metadata(root.join(at)).is_ok())
        {
            out.push(path);
        }
    }
}

#[derive(Serialize)]
struct Node {
    name: String,
    version: String,
    content: String,
    dependencies: Vec<(String, usize)>,
}

/// The pin of a runtime graph: its packages' contents and edges, never
/// their paths. Each node's `dir` must be the canonical parent of a manifest
/// the host supplied in this report.
fn pin(
    graph: &RuntimeGraph,
    supplied_node_dirs: &BTreeSet<String>,
) -> Result<RuntimeDependency, String> {
    for skip in &graph.skip {
        if skip.is_empty() || skip.contains(['/', '\\', '\0']) || skip == "." || skip == ".." {
            return Err(format!("the backend named an invalid skip entry `{skip}`"));
        }
    }
    let mut nodes = Vec::new();
    for node in &graph.nodes {
        if !supplied_node_dirs.contains(&node.dir) {
            return Err(format!(
                "the backend named a runtime directory the host did not supply: `{}`",
                node.dir
            ));
        }
        nodes.push(Node {
            name: node.name.clone(),
            version: node.version.clone(),
            content: content(Path::new(&node.dir), &graph.skip)?,
            dependencies: (node.dependencies.iter())
                .map(|(name, index)| (name.clone(), *index as usize))
                .collect(),
        });
    }
    let version = (nodes.first())
        .map(|n| n.version.clone())
        .ok_or("an empty runtime graph")?;
    let mut bytes = Vec::new();
    ciborium::into_writer(&nodes, &mut bytes).map_err(|e| e.to_string())?;
    Ok(RuntimeDependency {
        package: graph.package.clone(),
        version,
        sha256: sha256(&bytes),
    })
}

/// The hash of the files of the package at `dir`, outside the directories
/// `skip` names and `.git`, by their relative names and sizes.
pub(crate) fn content(dir: &Path, skip: &[String]) -> Result<String, String> {
    let mut files = Vec::new();
    collect(dir, Path::new(""), skip, &mut files)
        .map_err(|e| format!("cannot pin the installed runtime: {e}"))?;
    files.sort();
    let mut hash = Sha256::new();
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let path = dir.join(&relative);
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        hash.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(size.to_le_bytes());
        io::copy(
            &mut fs::File::open(path).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn collect(
    package: &Path,
    relative: &Path,
    skip: &[String],
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(package.join(relative))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if name == ".git" || skip.iter().any(|s| name == s.as_str()) {
            continue;
        }
        let path = relative.join(name);
        if kind.is_dir() {
            collect(package, &path, skip, files)?;
        } else if kind.is_file() {
            files.push(path);
        } else {
            return Err(io::Error::other("runtime contains a nonregular file"));
        }
    }
    Ok(())
}
