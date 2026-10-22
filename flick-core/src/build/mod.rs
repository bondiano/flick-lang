//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps and `.iface` files, the built-in modules they use, and
//! for an app the runner that starts `main`.

mod abi;

pub use abi::interface;

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::codegen::{self, Options, sourcemap};
use crate::iface;
use crate::project::{Compilation, normalize};
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

/// The output of a compilation.
pub fn emit(compilation: &Compilation, sources: &SourceMap, settings: &Settings) -> Vec<Artifact> {
    let modules = &compilation.modules;
    let provider = |m: &str| {
        compilation
            .packages
            .iter()
            .find(|(module, ..)| module == m)
            .map(|(_, package, interface, js)| (package.clone(), interface.clone(), js.clone()))
    };
    let ext = if settings.typescript { "ts" } else { "js" };
    let is_builtin = |m: &str| types::builtin_sources().iter().any(|(n, _)| *n == m);
    let path = |m: &str| {
        let file = format!("{}.{ext}", m.replace('.', "/"));
        match provider(m) {
            // a package's module sits next to its interface in `iface-dir`
            Some((_, _, js)) => format!("npm:{js}.js"),
            None if is_builtin(m) => format!("{BUILTIN_DIR}/{file}"),
            None => file,
        }
    };
    let own = |m: &str| modules.iter().any(|c| c.name == m);
    let package = |m: &str| match provider(m) {
        Some((package, ..)) => Some(package),
        None => own(m)
            .then(|| settings.package.map(str::to_owned))
            .flatten(),
    };
    let interface_of = |m: &str| {
        modules
            .iter()
            .find(|c| c.name == m)
            .map(|c| c.interface.clone())
            .or_else(|| provider(m).map(|(_, i, _)| i))
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
    let any_interface = |m: &str| {
        interface_of(m).or_else(|| types::builtins().iter().find(|i| i.module == m).cloned())
    };
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: settings.typescript,
        interface: &any_interface,
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
        let built = interface(
            &module.ir,
            &module.interface,
            package(&module.name).as_deref(),
            &module.imports,
            &interface_of,
        );
        out.push(Artifact {
            path: format!("{}.iface", module.name.replace('.', "/")),
            contents: iface::write(&built),
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

#[cfg(test)]
mod tests {
    use crate::iface::{self, Interface};
    use crate::ir::tests::check_text;

    #[test]
    fn built_interface_has_the_abi_and_round_trips() {
        let (ir, interface) = check_text(
            "App.Shapes",
            "module App.Shapes exposing (Shape(..), area, scale, now)\n\
             type Shape = Circle Number | Rect Number Number\n  deriving Eq, Show\n\
             class Describe a =\n  describe : a -> String\n\
             instance Describe Shape =\n  let describe s = \"shape\"\n\
             area : Shape -> Number\nlet area s =\n  match s with\n  | Circle r -> r * r\n  | Rect w h -> w * h\n\
             scale : Number -> Shape -> Shape\nlet scale k s = Circle k\n\
             now : Unit ->{} Number\nforeign sync now from global \"Date.now\"\n",
        )
        .expect("checks");
        let dependency = Interface {
            module: "Geo".into(),
            ..Interface::default()
        };
        let lookup = |m: &str| (m == "Geo").then(|| dependency.clone());
        let built = super::interface(
            &ir,
            &interface,
            Some("@acme/shapes"),
            &["Geo".into()],
            &lookup,
        );
        let json = iface::write(&built);
        insta::assert_snapshot!(json);
        let back = iface::read(json.as_bytes(), "App.Shapes").expect("valid");
        assert_eq!(iface::write(&back), json);
        assert_eq!(iface::fingerprint(&back), iface::fingerprint(&built));
        assert_eq!(built.dependencies["Geo"], iface::fingerprint(&dependency));
    }

    #[test]
    fn summaries_see_through_calls_of_total_values() {
        let (ir, interface) = check_text(
            "Calc",
            "module Calc exposing (..)\n\
             double : Number -> Number\nlet double x = x + x\n\
             quad : Number -> Number\nlet quad x = double (double x)\n\
             adder : Number -> Number -> Number\nlet adder a b = a + b\n\
             add_one : Number -> Number\nlet add_one = adder 1\n\
             loop : Number -> Number\nlet rec loop x = loop x\n\
             via_loop : Number -> Number\nlet via_loop x = double (loop x)\n\
             ratio : Number -> Number\nlet ratio x = x / 2\n",
        )
        .expect("checks");
        let built = super::interface(&ir, &interface, None, &[], &|_| None);
        let partial: Vec<(String, bool)> = built
            .abi
            .expect("abi")
            .values
            .into_iter()
            .map(|s| (s.name, s.partial))
            .collect();
        let expected = [
            ("double", false),
            ("quad", false),
            ("adder", false),
            ("add_one", false),
            ("loop", true),
            ("via_loop", true),
            ("ratio", true),
        ];
        for (name, p) in expected {
            assert!(
                partial.contains(&(name.to_owned(), p)),
                "{name}: {partial:?}"
            );
        }
    }
}
