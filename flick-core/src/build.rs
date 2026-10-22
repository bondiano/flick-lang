//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps, the built-in modules they use, and for an app the
//! runner that starts `main`.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::codegen::{self, Options, sourcemap};
use crate::project::{Compiled, normalize};
use crate::resolve::{self, Module};
use crate::source::{FileId, SourceMap};
use crate::{ir, syntax, types};

/// Directory of the built-in modules inside the output.
pub const BUILTIN_DIR: &str = "_flick";

/// A file of the output, by its path relative to the output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    pub contents: String,
}

pub struct Settings<'a> {
    /// The package the project's modules belong to.
    pub package: Option<&'a str>,
    /// The entry module of an app.
    pub entry: Option<&'a str>,
    pub typescript: bool,
    /// How a source map refers to a source file.
    pub source_path: &'a dyn Fn(FileId) -> String,
    /// The directory of a module's source, which a relative foreign module
    /// is resolved against; `None` for a module outside the build.
    pub source_dir: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// The absolute output directory.
    pub out_dir: &'a Path,
}

/// The output of `modules`.
pub fn emit(modules: &[Compiled], sources: &SourceMap, settings: &Settings) -> Vec<Artifact> {
    let ext = if settings.typescript { "ts" } else { "js" };
    let is_builtin = |m: &str| types::builtin_sources().iter().any(|(n, _)| *n == m);
    let path = |m: &str| {
        let file = format!("{}.{ext}", m.replace('.', "/"));
        if is_builtin(m) {
            format!("{BUILTIN_DIR}/{file}")
        } else {
            file
        }
    };
    let own = |m: &str| modules.iter().any(|c| c.name == m);
    let package = |m: &str| {
        own(m)
            .then(|| settings.package.map(str::to_owned))
            .flatten()
    };
    let foreign = |module: &str, spec: &str| {
        let relative = spec.starts_with("./") || spec.starts_with("../");
        match (relative, (settings.source_dir)(module)) {
            (true, Some(dir)) => {
                let target = normalize(&dir.join(spec));
                let from = settings.out_dir.join(path(module));
                specifier(from.parent().unwrap_or(settings.out_dir), &target)
            }
            _ => spec.to_owned(),
        }
    };
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: settings.typescript,
    };
    let mut out = vec![Artifact {
        path: "package.json".to_owned(),
        contents: "{ \"type\": \"module\" }\n".to_owned(),
    }];
    for module in builtin_modules() {
        let output = codegen::js(module, &options);
        out.push(Artifact {
            path: path(&module.name),
            contents: output.code,
        });
    }
    for module in modules {
        let output = codegen::js(&module.ir, &options);
        let file = path(&module.name);
        let base = file.rsplit('/').next().unwrap_or(&file).to_owned();
        let map = sourcemap::source_map(&base, &output, sources, settings.source_path);
        out.push(Artifact {
            path: file.clone(),
            contents: format!("{}//# sourceMappingURL={base}.map\n", output.code),
        });
        out.push(Artifact {
            path: format!("{file}.map"),
            contents: map,
        });
    }
    if let Some(entry) = settings.entry {
        out.push(Artifact {
            path: format!("main.{ext}"),
            contents: format!(
                "import {{ runMain }} from \"@flick-lang/runtime\";\nimport {{ main }} from \"./{}\";\n\nawait runMain(main);\n",
                path(entry)
            ),
        });
    }
    out
}

/// The relative import specifier of `target` from the directory `from`.
fn specifier(from: &Path, target: &Path) -> String {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

/// The built-in modules, lowered.
pub fn builtin_modules() -> &'static [ir::Module] {
    static MODULES: OnceLock<Vec<ir::Module>> = OnceLock::new();
    MODULES.get_or_init(|| {
        let interfaces = types::builtins();
        types::builtin_sources()
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                let mut sources = SourceMap::default();
                let file = sources.add(format!("<builtin>/{name}.fl"), *text);
                let parse = syntax::parse(text, file);
                let module_syntax = resolve::module_syntax(&parse.tree, text, file);
                let module = Module {
                    name,
                    file,
                    text,
                    tree: &parse.tree,
                    syntax: &module_syntax,
                };
                let (mut interface, _) = resolve::resolve(&module, &|_| None);
                let local = |n: &str| types::builtin_sources().iter().any(|(m, _)| *m == n);
                let options = types::Options {
                    local: &local,
                    limits: types::Limits::default(),
                };
                let (_, typing) = types::check_typed(
                    &module,
                    &|_| None,
                    &interfaces[..i],
                    options,
                    &mut interface,
                );
                ir::lower(&module, &typing).expect("built-in modules compile")
            })
            .collect()
    })
}
