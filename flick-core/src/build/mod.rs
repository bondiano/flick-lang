//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps and `.iface` files, the built-in modules they use, and
//! for an app the runner that starts `main`.

mod abi;

pub use abi::interface;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::codegen::ts::{Facade, Projection, declaration_file, facade_files};
use crate::codegen::{self, Options, sourcemap};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface;
use crate::iface::{Interface, Namespace};
use crate::project::{Compilation, Compiled, normalize};
use crate::resolve::{self, Module};
use crate::source::{FileId, SourceMap};
use crate::{ir, opt, syntax, types};

/// Directory of the built-in modules inside the output.
pub const BUILTIN_DIR: &str = "_flick";

/// The app launcher: a hyphen cannot occur in a module name.
pub const fn runner_path(typescript: bool) -> &'static str {
    if typescript {
        "flick-main.ts"
    } else {
        "flick-main.js"
    }
}

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
    /// The release profile: the optimizer's passes before code generation.
    pub release: bool,
    /// What else the build writes.
    pub extras: Extras,
    /// How a source map refers to a source file.
    pub source_path: &'a dyn Fn(FileId) -> String,
    /// The directory of a module's source, which a relative foreign module
    /// is resolved against; `None` for a module outside the build.
    pub source_dir: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// The absolute output directory.
    pub out_dir: &'a Path,
    /// The project's root: interfaces name their source relative to it.
    pub root: Option<&'a Path>,
}

/// The opt-in outputs of a build.
#[derive(Debug, Clone, Copy, Default)]
pub struct Extras {
    /// Write `A/B.d.ts`, the public TypeScript interface of each module.
    pub dts: bool,
    /// Write `A/B.promise.js`: Promise façades of runtime functions.
    pub promise_facade: bool,
    /// The runner of an app compiles pure codecs ahead of use.
    pub schema_aot: bool,
}

/// The output of a compilation, and what it could not write: exports
/// without a TypeScript form under `--emit-dts`.
pub fn emit(
    compilation: &Compilation,
    sources: &SourceMap,
    settings: &Settings,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
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
    let exported_workers = exported_workers(settings.release, modules, &any_interface);
    let workers = |m: &str, name: &str| {
        exported_workers
            .get(&(m.to_owned(), name.to_owned()))
            .copied()
    };
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: settings.typescript,
        interface: &any_interface,
        release: settings.release,
        workers: &workers,
    };
    let mut out = vec![package_json(settings)];
    let decls = |name: &str| declarations(name, &any_interface, &package, &path);
    for module in builtin_modules() {
        out.push(Artifact {
            path: path(&module.name),
            contents: codegen::js(module, &options).code,
        });
        if settings.extras.dts
            && let Some((text, _)) = decls(&module.name)
        {
            out.push(Artifact {
                path: dts_path(&path(&module.name)),
                contents: text,
            });
        }
    }
    let ctx = Context {
        settings,
        sources,
        options: &options,
        path: &path,
        package: &package,
        interface_of: &interface_of,
        any_interface: &any_interface,
    };
    for module in modules {
        compiled_module(&ctx, module, &mut out, &mut diagnostics);
    }
    if let Some(entry) = settings.entry {
        out.push(runner(
            runner_path(settings.typescript),
            &path(entry),
            settings.extras.schema_aot,
        ));
    }
    (out, diagnostics)
}

/// `package.json` of the output. Top-level initialization is pure
/// (spec 11): only the runner of an app runs anything when loaded.
fn package_json(settings: &Settings) -> Artifact {
    let side_effects = match settings.entry {
        Some(_) => format!("[\"./{}\"]", runner_path(settings.typescript)),
        None => "false".to_owned(),
    };
    Artifact {
        path: "package.json".to_owned(),
        contents: format!("{{ \"type\": \"module\", \"sideEffects\": {side_effects} }}\n"),
    }
}

/// In release, the worker `name$n` each module of the build and each
/// built-in module exports for a value, from its reference lowering as its `.iface`
/// summaries say: a full application in another module calls it.
fn exported_workers(
    release: bool,
    modules: &[Compiled],
    any_interface: &dyn Fn(&str) -> Option<Interface>,
) -> HashMap<(String, String), usize> {
    if !release {
        return HashMap::new();
    }
    let own = modules.iter().map(|c| (&c.name, &c.ir.items));
    let builtin = builtin_modules().iter().map(|m| (&m.name, &m.items));
    let mut out = HashMap::new();
    for (module, items) in own.chain(builtin) {
        let Some(interface) = any_interface(module) else {
            continue;
        };
        for (name, arity) in items.iter().filter_map(codegen::exported_worker) {
            if interface.exports_value(name) {
                out.insert((module.clone(), name.to_owned()), arity);
            }
        }
    }
    out
}

/// What the compilation of every module shares.
struct Context<'a> {
    settings: &'a Settings<'a>,
    sources: &'a SourceMap,
    options: &'a Options<'a>,
    path: &'a dyn Fn(&str) -> String,
    package: &'a dyn Fn(&str) -> Option<String>,
    interface_of: &'a dyn Fn(&str) -> Option<Interface>,
    any_interface: &'a dyn Fn(&str) -> Option<Interface>,
}

/// The JS, source map, interface and, when asked, `.d.ts` and Promise
/// façade of one module of the build.
fn compiled_module(
    ctx: &Context,
    module: &Compiled,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // the interface and its summaries come from the reference lowering
    let optimized = ctx.settings.release.then(|| {
        let mut ir = module.ir.clone();
        let exported = |name: &str| module.interface.exports_value(name);
        opt::optimize(&mut ir, &exported);
        ir
    });
    let output = codegen::js(optimized.as_ref().unwrap_or(&module.ir), ctx.options);
    let file = (ctx.path)(&module.name);
    let base = file.rsplit('/').next().unwrap_or(&file).to_owned();
    let map = sourcemap::source_map(&base, &output, ctx.sources, ctx.settings.source_path);
    out.push(Artifact {
        path: file.clone(),
        contents: format!("{}//# sourceMappingURL={base}.map\n", output.code),
    });
    out.push(Artifact {
        path: format!("{file}.map"),
        contents: map,
    });
    let mut built = interface(
        &module.ir,
        &module.interface,
        (ctx.package)(&module.name).as_deref(),
        &module.imports,
        ctx.interface_of,
    );
    built.source = ctx
        .settings
        .root
        .and_then(|root| module.path.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    if ctx.settings.extras.dts
        && let Some((text, refused)) =
            declarations(&module.name, ctx.any_interface, ctx.package, ctx.path)
    {
        out.push(Artifact {
            path: dts_path(&file),
            contents: text,
        });
        for (name, reason) in refused {
            diagnostics.push(Diagnostic::new(
                codes::F004,
                Severity::Error,
                format!("`{}.{name}` has no TypeScript form: {reason}", module.name),
            ));
        }
    }
    if ctx.settings.extras.promise_facade {
        out.extend(facade(
            module,
            &built,
            ctx.any_interface,
            ctx.package,
            ctx.path,
        ));
    }
    out.push(Artifact {
        path: format!("{}.iface", module.name.replace('.', "/")),
        contents: iface::write(&built),
    });
}

/// The runner of an app: starts `main` of the entry module at `entry`. With
/// `schema_aot` it collects the codecs the modules make while loading and
/// installs their compiled decoders, which `flick build` writes next to it.
fn runner(file: &str, entry: &str, schema_aot: bool) -> Artifact {
    let contents = if schema_aot {
        format!(
            "import {{ runMain, enableSchemaAot, installSchemaAot }} from \"@flick-lang/runtime\";\n\nenableSchemaAot();\nconst {{ main }} = await import(\"./{entry}\");\ninstallSchemaAot(await import(\"./{SCHEMA_AOT}\"));\nawait runMain(main);\n"
        )
    } else {
        format!(
            "import {{ runMain }} from \"@flick-lang/runtime\";\nimport {{ main }} from \"./{entry}\";\n\nawait runMain(main);\n"
        )
    };
    Artifact {
        path: file.to_owned(),
        contents,
    }
}

/// The module of compiled decoders an app's runner installs.
pub const SCHEMA_AOT: &str = "schemas.aot.js";

/// The `.d.ts` of module `name`, and the exports it refused with why.
fn declarations(
    name: &str,
    interfaces: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    path: &dyn Fn(&str) -> String,
) -> Option<(String, Vec<(String, String)>)> {
    let projection = Projection::new(name, interfaces, package);
    let interface = interfaces(name)?;
    let here = path(name);
    let import = |m: &str| codegen::relative(&here, &path(m));
    Some(declaration_file(&projection, &interface, &import))
}

/// `A/B.d.ts` beside `A/B.js` (or `A/B.ts`).
fn dts_path(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    format!("{stem}.d.ts")
}

/// The Promise façade of `module`: each exported runtime function that
/// needs no service and whose earlier stages only make closures.
fn facade(
    module: &Compiled,
    built: &Interface,
    interfaces: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    path: &dyn Fn(&str) -> String,
) -> Vec<Artifact> {
    // the façade module names the module's own types through an import
    let facade_module = format!("{}.promise", module.name);
    let projection = Projection::new(&facade_module, interfaces, package);
    let summaries = built
        .abi
        .as_ref()
        .map(|a| a.values.clone())
        .unwrap_or_default();
    let facades: Vec<Facade> = built
        .exports
        .iter()
        .filter(|e| e.entity.namespace == Namespace::Value && e.entity.module == module.name)
        .filter_map(|e| {
            let closures = summaries
                .iter()
                .find(|s| s.name == e.entity.name)?
                .stages
                .len();
            let (arity, ty) = projection.facade(e.scheme.as_ref()?, closures).ok()?;
            Some(Facade {
                name: e.entity.name.clone(),
                arity,
                ty,
            })
        })
        .collect();
    if facades.is_empty() {
        return Vec::new();
    }
    let file = path(&module.name);
    let stem = file
        .rsplit_once('.')
        .map_or(file.as_str(), |(s, _)| s)
        .to_owned();
    let base = file.rsplit('/').next().unwrap_or(&file).to_owned();
    let here = format!("{stem}.promise.js");
    let import = |m: &str| codegen::relative(&here, &path(m));
    let (js, dts) = facade_files(&projection, &format!("./{base}"), &facades, &import);
    vec![
        Artifact {
            path: here,
            contents: js,
        },
        Artifact {
            path: format!("{stem}.promise.d.ts"),
            contents: dts,
        },
    ]
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
                let imported = |m: &str| interfaces[..i].iter().find(|x| x.module == m).cloned();
                let (mut interface, _) = resolve::resolve(&module, &imported);
                let local = |n: &str| types::builtin_sources().iter().any(|(m, _)| *m == n);
                let options = types::Options {
                    local: &local,
                    limits: types::Limits::default(),
                    sql: Default::default(),
                };
                let (_, typing) = types::check_typed(
                    &module,
                    &imported,
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
