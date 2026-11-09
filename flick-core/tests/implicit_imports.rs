//! What the JavaScript of an implicit package's module imports of another such
//! module, an instance's dictionary or a lang item among them, its source says
//! with an `import`, so a package's dependencies can be read from its sources.
//! Which names a module qualifies is checked beside the built-in modules.

#![cfg(feature = "typescript")]

use std::io;
use std::path::{Path, PathBuf};

use flick_core::build::{self, BUILTIN_DIR, Extras, Settings};
use flick_core::codegen::EffectBridge;
use flick_core::project::config::Backend;
use flick_core::project::{self, Host, normalize};
use flick_core::source::{FileId, SourceMap};
use flick_core::{resolve, syntax, types};

const MAIN: &str = "module Main exposing (answer)\nanswer : Int\nlet answer = 42\n";

struct Program;

impl Host for Program {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Main.fl") {
            Ok(MAIN.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn source_path(_: FileId) -> String {
    "file:///Main.fl".to_owned()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the build settings require this shape"
)]
fn source_dir(_: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

/// The modules the source `text` of a built-in module imports.
fn imports(text: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let file = sources.add("module.fl", text);
    let parse = syntax::parse(text, file);
    (resolve::module_syntax(&parse.tree, text, file)
        .imports
        .iter())
    .map(|i| i.module.clone())
    .collect()
}

/// The built-in modules the JavaScript `code` at the path `file` imports.
fn emitted(file: &str, code: &str) -> Vec<String> {
    let dir = Path::new(BUILTIN_DIR)
        .join(file)
        .parent()
        .expect("a directory")
        .to_path_buf();
    code.lines()
        .filter(|line| line.starts_with("import "))
        .filter_map(|line| line.rsplit_once(" from \"")?.1.split_once('"'))
        .filter(|(spec, _)| spec.starts_with('.'))
        .filter_map(|(spec, _)| {
            let target = normalize(&dir.join(spec)).with_extension("");
            let target = target.strip_prefix(BUILTIN_DIR).ok()?.to_owned();
            Some(target.to_string_lossy().replace('/', "."))
        })
        .collect()
}

#[test]
fn the_javascript_of_an_implicit_package_imports_what_its_source_imports() {
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) =
        project::compile(&Program, &mut sources, None, &[PathBuf::from("/Main.fl")]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    // declarations ask for every built-in module
    let settings = Settings {
        package: None,
        entries: &[],
        typescript: false,
        release: false,
        extras: Extras {
            dts: true,
            ..Extras::default()
        },
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend: Backend::Effect,
        effect_bridge: EffectBridge::Absent,
        ui: None,
        runners: &[],
        core_modules: &[],
        core_only: false,
    };
    let (artifacts, diagnostics) = build::emit(&compilation, &sources, &settings);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let mut problems = Vec::new();
    let mut seen = 0;
    for (name, text) in types::builtin_sources() {
        let file = format!("{}.js", name.replace('.', "/"));
        let path = format!("{BUILTIN_DIR}/{file}");
        let code = &(artifacts.iter())
            .find(|a| a.path == path)
            .unwrap_or_else(|| panic!("`{path}` is written"))
            .contents;
        seen += 1;
        let imported = imports(text);
        for target in emitted(&file, code) {
            if types::implicit_package_of(&target).is_some() && !imported.contains(&target) {
                problems.push(format!("{name} -> {target}"));
            }
        }
    }
    assert!(seen > 100, "only {seen} modules");
    assert!(problems.is_empty(), "no `import`:\n{}", problems.join("\n"));
}
