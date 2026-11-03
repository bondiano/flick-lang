use std::path::{Path, PathBuf};
use std::{env, fs};

use flick_core::diagnostics::Severity;
use flick_core::project::cache::Key;
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;
use flick_core::{resolve, syntax};

use crate::builtins;
use crate::command::Outcome;
use crate::sources::{self, FsHost};
use crate::{cache, toolchain};

/// The import root a module file outside the project's roots establishes:
/// the directory its module name's path starts in, where its siblings are.
pub fn sibling_root(path: &Path, name: &str) -> PathBuf {
    let tail = project::module_path(name);
    let root = path
        .to_str()
        .and_then(|p| p.strip_suffix(tail.to_str()?))
        .map_or_else(
            || path.parent().unwrap_or(Path::new("/")).to_path_buf(),
            PathBuf::from,
        );
    project::normalize(&root)
}

/// The module name `text` declares in its header, if any.
pub fn declared_name(text: &str) -> Option<String> {
    let mut sources = SourceMap::default();
    let file = sources.add("", text);
    let parse = syntax::parse(text, file);
    resolve::module_syntax(&parse.tree, text, file)
        .header
        .map(|header| header.name)
}

/// The conservative source roots and other inputs used by the native compiler.
pub fn describe(
    host: &FsHost,
    mut sources: SourceMap,
    project: Option<&Project>,
    selected: &[PathBuf],
) -> Outcome {
    let mut diagnostics = Vec::new();
    let mut roots = project.map_or_else(Vec::new, |p| vec![p.source_root(), p.test_root()]);
    let project_root_count = roots.len();
    let mut modules = Vec::new();
    for path in selected {
        let file = match sources::load(&mut sources, &path.to_string_lossy()) {
            Ok(file) => file,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let text = &sources.get(file).text;
        let parse = syntax::parse(text, file);
        let module = resolve::module_syntax(&parse.tree, text, file);
        diagnostics.extend(parse.diagnostics);
        let name = module
            .header
            .map(|header| header.name)
            .or_else(|| {
                roots
                    .iter()
                    .find_map(|root| project::module_name(path.strip_prefix(root).ok()?))
            })
            .unwrap_or_default();
        if !roots.iter().any(|root| path.starts_with(root)) {
            roots.push(sibling_root(path, &name));
        }
        modules.push(serde_json::json!({ "path": path, "name": name }));
    }
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Outcome {
            sources,
            diagnostics,
        };
    }
    let mut files = project.map_or_else(|| selected.to_vec(), |p| cache::input_files(p, selected));
    for root in roots.iter().skip(project_root_count) {
        let mut found = Vec::new();
        sources::source_files(&root.to_string_lossy(), &mut found);
        files.extend(found.into_iter().map(PathBuf::from));
    }
    roots.sort();
    roots.dedup();
    // A newly created nearer config must also invalidate an existing import.
    for dir in host.cwd.ancestors() {
        files.push(dir.join(project::CONFIG_FILE));
        if project.is_some_and(|p| p.root == dir) {
            break;
        }
    }
    files.sort();
    files.dedup();
    let mut key = Key::default();
    key.add("compiler", toolchain::COMPILER);
    key.add("std", builtins::std_hash());
    if let Ok(meta) = env::current_exe().and_then(fs::metadata) {
        key.add(
            "compiler-binary",
            format!("{} {:?}", meta.len(), meta.modified().ok()),
        );
    }
    if let Some(project) = project {
        for package in ["@flick-lang/runtime", "effect"] {
            key.add(
                format!("version {package}"),
                toolchain::resolved(&project.root, package).unwrap_or_default(),
            );
        }
    }
    for file in &files {
        key.add(file.to_string_lossy(), fs::read(file).unwrap_or_default());
    }
    println!(
        "{}",
        serde_json::json!({
            "key": key.finish(),
            "files": files,
            "roots": roots,
            "modules": modules,
        })
    );
    Outcome {
        sources,
        diagnostics,
    }
}
