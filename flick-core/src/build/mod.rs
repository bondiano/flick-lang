//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps and `.iface` files, the built-in modules they use, and
//! for an app the runner that starts `main`.

mod abi;
pub mod effects;
pub mod go;

pub use abi::interface;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::codegen::ts::{Facade, Projection, declaration_file, facade_files};
use crate::codegen::{self, Labels, Options, sourcemap};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface;
use crate::iface::{Interface, Namespace};
use crate::project::config::{Backend, MAIN_ENTRY};
use crate::project::{Compilation, Compiled, MainKind, main_kind, normalize};
use crate::resolve::{self, Module};
use crate::source::{FileId, SourceMap, Span};
use crate::{ir, opt, syntax, types};

/// Directory of the built-in modules inside the output.
pub const BUILTIN_DIR: &str = "_flick";

/// The launcher of the entry `main`, the one `entry` names: a hyphen
/// cannot occur in a module name.
pub const fn runner_path(typescript: bool) -> &'static str {
    if typescript {
        "flick-main.ts"
    } else {
        "flick-main.js"
    }
}

/// The launcher of the entry `name`: `flick-<name>.js`.
pub fn runner_file(name: &str, typescript: bool) -> String {
    format!("flick-{name}.{}", if typescript { "ts" } else { "js" })
}

/// The compiled decoders the runner of the entry `name` installs: those of
/// `main` keep the name a single entry always had.
pub fn schema_aot_file(name: &str) -> String {
    if name == MAIN_ENTRY {
        SCHEMA_AOT.to_owned()
    } else {
        format!("schemas.{name}.aot.js")
    }
}

/// The runtime modules `[build] ui` replaces.
const UI_RENDERER: &[&str] = &["@flick-lang/runtime/ui", "@flick-lang/runtime/ui/react"];

/// A file of the output, by its path relative to the output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    pub contents: String,
}

pub struct Settings<'a> {
    /// The package the project's modules belong to.
    pub package: Option<&'a str>,
    /// The entry points of an app: a runner for each name, starting the
    /// `main` of the module.
    pub entries: &'a [(String, String)],
    pub typescript: bool,
    /// The release profile: the optimizer's passes before code generation.
    pub release: bool,
    /// What else the build writes.
    pub extras: Extras,
    /// How a source map refers to a source file; an absolute path becomes
    /// relative to the map.
    pub source_path: &'a dyn Fn(FileId) -> String,
    /// The directory of a module's source, which a relative foreign module
    /// is resolved against; `None` for a module outside the build.
    pub source_dir: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// The absolute output directory.
    pub out_dir: &'a Path,
    /// The project's root: interfaces name their source relative to it.
    pub root: Option<&'a Path>,
    /// What runs effects: without Effect, compiled code imports the
    /// runtime's `core`.
    pub backend: Backend,
    /// The module that renders and mounts views in place of React's; a
    /// relative one is relative to the project's root.
    pub ui: Option<&'a str>,
    /// The module whose `run` starts an entry, by the entry's name; an entry
    /// without one starts the default way.
    pub runners: &'a [(String, String)],
    /// The modules of a library on Effect that it also built without Effect,
    /// under [`effects::CORE_DIR`].
    pub core_modules: &'a [String],
    /// Build, without Effect, only the modules that need no Effect, and no
    /// interfaces: the part of a library under [`effects::CORE_DIR`].
    pub core_only: bool,
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
    /// What a release build labels for debugging tools.
    pub labels: Labels,
}

/// The output of a compilation, and what it could not write: exports
/// without a TypeScript form under `--emit-dts`.
pub fn emit(
    compilation: &Compilation,
    sources: &SourceMap,
    settings: &Settings,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    if !settings.core_only {
        effects::check_packages(
            &compilation.package_modules,
            settings.backend,
            &mut diagnostics,
        );
    }
    let modules = &compilation.modules;
    let provider = |m: &str| {
        compilation
            .package_modules
            .iter()
            .find(|module| module.module == m)
    };
    let ext = if settings.typescript { "ts" } else { "js" };
    let is_builtin = |m: &str| types::builtin_sources().iter().any(|(n, _)| *n == m);
    let path = |m: &str| {
        let file = format!("{}.{ext}", m.replace('.', "/"));
        match provider(m) {
            // a package's module sits next to its interface in `iface-dir`
            Some(module) => format!(
                "npm:{}.js",
                effects::package_path(&module.js_specifier, m, &module.interface, settings.backend)
            ),
            None if is_builtin(m) => format!("{BUILTIN_DIR}/{file}"),
            None => file,
        }
    };
    let own = |m: &str| modules.iter().any(|c| c.name == m);
    let package = |m: &str| match provider(m) {
        Some(module) => Some(module.package.clone()),
        None => own(m)
            .then(|| settings.package.map(str::to_owned))
            .flatten(),
    };
    let published_interfaces = published_interfaces(compilation, settings);
    let interface_of = |m: &str| {
        published_interfaces
            .get(m)
            .cloned()
            .or_else(|| provider(m).map(|module| module.interface.clone()))
    };
    let foreign = |module: &str, spec: &str| {
        // `[build] ui` stands for the renderer and the mount of `Ui` and `Action`
        let (spec, dir) = match settings.ui {
            Some(ui) if UI_RENDERER.contains(&spec) => (ui, settings.root.map(Path::to_path_buf)),
            _ => (spec, (settings.source_dir)(module)),
        };
        let relative = spec.starts_with("./") || spec.starts_with("../");
        if settings.backend == Backend::None && !relative {
            // a module without a version on `core` is reported by `effects::check`
            return effects::specifier(spec).unwrap_or_else(|| spec.to_owned());
        }
        match (relative, dir) {
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
    let (workers, direct) = exports(settings.release, modules, &any_interface);
    let workers = |m: &str, name: &str| workers.get(&(m.to_owned(), name.to_owned())).copied();
    let direct = |m: &str, i: &str, method: &str| {
        direct.contains(&(m.to_owned(), i.to_owned(), method.to_owned()))
    };
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: settings.typescript,
        interface: &any_interface,
        release: settings.release,
        workers: &workers,
        direct: &direct,
        labels: settings.extras.labels,
    };
    let mut out = if settings.core_only {
        Vec::new()
    } else {
        vec![package_json(settings)]
    };
    let ctx = Context {
        modules: &compilation.modules,
        settings,
        sources,
        options: &options,
        path: &path,
        package: &package,
        published_interfaces: &published_interfaces,
        any_interface: &any_interface,
    };
    // a built-in runner is written like any module the program imports
    let mut imports: Vec<String> = settings.runners.iter().map(|(_, r)| r.clone()).collect();
    imports.extend(own_modules(&ctx, compilation, &mut out, &mut diagnostics));
    builtins(&ctx, imports, &mut out, &mut diagnostics);
    entry_runners(settings, modules, &path, &mut out, &mut diagnostics);
    (out, diagnostics)
}

fn published_interfaces<'a>(
    compilation: &'a Compilation,
    settings: &Settings,
) -> BTreeMap<&'a str, Interface> {
    let modules = &compilation.modules;
    let checked_interfaces = |name: &str| {
        modules
            .iter()
            .find(|module| module.name == name)
            .map(|module| module.interface.clone())
            .or_else(|| {
                compilation
                    .package_modules
                    .iter()
                    .find(|module| module.module == name)
                    .map(|module| module.interface.clone())
            })
    };
    let mut published_interfaces: BTreeMap<&str, Interface> = modules
        .iter()
        .map(|module| {
            let mut published = interface(
                &module.ir,
                &module.interface,
                settings.package,
                &[],
                &checked_interfaces,
            );
            published.source = settings
                .root
                .and_then(|root| module.path.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            if let Some(abi) = &mut published.abi {
                abi.core = if settings.backend == Backend::None {
                    Some(String::new())
                } else {
                    settings
                        .core_modules
                        .contains(&module.name)
                        .then(|| effects::CORE_DIR.to_owned())
                };
            }
            (module.name.as_str(), published)
        })
        .collect();
    // Hash only finalized ABI, including backend availability, exactly as published.
    let fingerprints: BTreeMap<&str, String> = published_interfaces
        .iter()
        .map(|(name, interface)| (*name, iface::fingerprint(interface)))
        .chain(compilation.package_modules.iter().map(|module| {
            (
                module.module.as_str(),
                iface::fingerprint(&module.interface),
            )
        }))
        .collect();
    for module in modules {
        let published = published_interfaces
            .get_mut(module.name.as_str())
            .expect("every compiled module has a published interface");
        published.dependencies = module
            .imports
            .iter()
            .filter_map(|name| Some((name.clone(), fingerprints.get(name.as_str())?.clone())))
            .collect();
    }
    published_interfaces
}

/// The runner of each entry, or why `main` cannot be started.
fn entry_runners(
    settings: &Settings,
    modules: &[Compiled],
    path: &dyn Fn(&str) -> String,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, entry) in settings.entries {
        let started_by = settings
            .runners
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r.as_str());
        let module = modules.iter().find(|m| &m.name == entry);
        let main = module.and_then(|m| m.interface.find(Namespace::Value, "main"));
        // a re-exported `main` is imported from the module that defines it, whose output has it
        let defining = main.map_or(entry.as_str(), |e| e.entity.module.as_str());
        // a project checked its entry already, against its runner if it has one;
        // a file built alone is checked here
        let kind = main
            .filter(|_| started_by.is_none())
            .and_then(|e| Some(main_kind(e.scheme.as_ref()?)));
        match kind {
            Some(Err(message)) => {
                let diagnostic = Diagnostic::new(codes::P026, Severity::Error, message);
                let file = modules.iter().find(|m| m.name == defining).map(|m| m.file);
                let at = file.zip(main.and_then(|e| e.at));
                diagnostics.push(match at {
                    Some((f, [start, end])) => diagnostic.with_span(Span::new(f, start, end)),
                    None => diagnostic,
                });
            }
            kind => {
                let file = runner_file(name, settings.typescript);
                let aot = settings.extras.schema_aot.then(|| schema_aot_file(name));
                let start = match started_by {
                    Some(module) => Start::Run(codegen::relative(&file, &path(module))),
                    None => match kind.and_then(Result::ok) {
                        Some(MainKind::Pure) => Start::Call,
                        _ => Start::RunMain,
                    },
                };
                out.push(runner(
                    &file,
                    &codegen::relative(&file, &path(defining)),
                    &start,
                    aot.as_deref(),
                ));
            }
        }
    }
}

/// The output of the program's own modules; what they import.
fn own_modules(
    ctx: &Context,
    compilation: &Compilation,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    if ctx.settings.core_only {
        return core_modules(ctx, compilation, out);
    }
    let modules = compilation.modules.iter();
    modules
        .flat_map(|module| compiled_module(ctx, module, out, diagnostics))
        .collect()
}

/// The build without Effect of a library's modules that need no Effect, nor
/// any module that needs it; what they import.
fn core_modules(ctx: &Context, compilation: &Compilation, out: &mut Vec<Artifact>) -> Vec<String> {
    let compiled: Vec<(&str, Vec<Artifact>, Vec<String>, bool)> = compilation
        .modules
        .iter()
        .map(|module| {
            let (mut artifacts, mut needs) = (Vec::new(), Vec::new());
            let imports = compiled_module(ctx, module, &mut artifacts, &mut needs);
            (module.name.as_str(), artifacts, imports, needs.is_empty())
        })
        .collect();
    let package = |m: &str| {
        compilation
            .package_modules
            .iter()
            .find(|module| module.module == m)
    };
    let mut core: HashSet<&str> = compiled.iter().filter(|c| c.3).map(|c| c.0).collect();
    // a module is out with what it imports
    loop {
        let out_now: Vec<&str> = compiled
            .iter()
            .filter(|(name, _, imports, _)| {
                core.contains(name)
                    && !imports.iter().all(|i| match package(i) {
                        Some(module) => effects::importable(&module.interface, Backend::None),
                        None if compilation.modules.iter().any(|m| &m.name == i) => {
                            core.contains(i.as_str())
                        }
                        None => effects::in_core(i),
                    })
            })
            .map(|c| c.0)
            .collect();
        if out_now.is_empty() {
            break;
        }
        for name in out_now {
            core.remove(name);
        }
    }
    compiled
        .into_iter()
        .filter(|c| core.contains(c.0))
        .flat_map(|(_, artifacts, imports, _)| {
            out.extend(artifacts);
            imports
        })
        .collect()
}

/// The modules of a library a build with `core_only` wrote.
pub fn core_built(
    artifacts: &[Artifact],
    compilation: &Compilation,
    typescript: bool,
) -> Vec<String> {
    let ext = if typescript { "ts" } else { "js" };
    compilation
        .modules
        .iter()
        .filter(|m| {
            let file = format!("{}.{ext}", m.name.replace('.', "/"));
            artifacts.iter().any(|a| a.path == file)
        })
        .map(|m| m.name.clone())
        .collect()
}

/// `package.json` of the output. Top-level initialization is pure
/// (spec 11): only the runner of an app runs anything when loaded.
fn package_json(settings: &Settings) -> Artifact {
    let side_effects = if settings.entries.is_empty() {
        "false".to_owned()
    } else {
        let runners: Vec<String> = settings
            .entries
            .iter()
            .map(|(name, _)| format!("\"./{}\"", runner_file(name, settings.typescript)))
            .collect();
        format!("[{}]", runners.join(", "))
    };
    Artifact {
        path: "package.json".to_owned(),
        contents: format!("{{ \"type\": \"module\", \"sideEffects\": {side_effects} }}\n"),
    }
}

/// Workers by module and value, and direct methods by module, instance
/// and method.
type Exports = (
    HashMap<(String, String), usize>,
    HashSet<(String, String, String)>,
);

/// In release, what each module of the build and each built-in module
/// exports for a full application or a canonical method call in another
/// module, from its reference lowering: the worker `name$n` of a value, as
/// its `.iface` summaries say, and the constants of instance methods.
fn exports(
    release: bool,
    modules: &[Compiled],
    any_interface: &dyn Fn(&str) -> Option<Interface>,
) -> Exports {
    let mut out = Exports::default();
    if !release {
        return out;
    }
    let own = modules.iter().map(|c| (&c.name, &c.ir.items));
    let builtin = builtin_modules().iter().map(|m| (&m.name, &m.items));
    for (module, items) in own.chain(builtin) {
        let Some(interface) = any_interface(module) else {
            continue;
        };
        for (name, arity) in items.iter().filter_map(codegen::exported_worker) {
            if interface.exports_value(name) {
                out.0.insert((module.clone(), name.to_owned()), arity);
            }
        }
        for (instance, method) in items.iter().flat_map(codegen::direct_methods) {
            out.1
                .insert((module.clone(), instance.to_owned(), method.to_owned()));
        }
    }
    out
}

/// The built-in modules the build's modules import, directly or through
/// one another. `.d.ts` files of other modules may name the types of any
/// built-in module, so with them every built-in module is written.
fn builtins(
    ctx: &Context,
    imports: Vec<String>,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let dts = ctx.settings.extras.dts;
    let mut pending: Vec<String> = if dts {
        builtin_modules().iter().map(|m| m.name.clone()).collect()
    } else {
        imports
    };
    let mut written = HashSet::new();
    while let Some(name) = pending.pop() {
        let Some(module) = builtin_modules().iter().find(|m| m.name == name) else {
            continue;
        };
        if !written.insert(name) {
            continue;
        }
        if ctx.settings.backend == Backend::None {
            effects::check_builtin(&module.name, diagnostics);
        }
        let output = codegen::js(module, ctx.options);
        pending.extend(output.imports);
        out.push(Artifact {
            path: (ctx.path)(&module.name),
            contents: output.code,
        });
        if dts
            && let Some((text, _)) =
                declarations(&module.name, ctx.any_interface, ctx.package, ctx.path)
        {
            out.push(Artifact {
                path: dts_path(&(ctx.path)(&module.name)),
                contents: text,
            });
        }
    }
}

/// The function `name` of `module`, a module of the build or a built-in one,
/// with its dictionary parameters and body for the optimizer to copy.
fn generic_of(modules: &[Compiled], module: &str, name: &str) -> Option<(Vec<String>, ir::Expr)> {
    if let Some(m) = modules.iter().find(|m| m.name == module) {
        return opt::names_only(&m.ir, name, &|n| m.interface.exports_value(n));
    }
    let ir = builtin_modules().iter().find(|m| m.name == module)?;
    let interface = types::builtins().iter().find(|i| i.module == module)?;
    opt::names_only(ir, name, &|n| interface.exports_value(n))
}

/// What the compilation of every module shares.
struct Context<'a> {
    modules: &'a [Compiled],
    settings: &'a Settings<'a>,
    sources: &'a SourceMap,
    options: &'a Options<'a>,
    path: &'a dyn Fn(&str) -> String,
    package: &'a dyn Fn(&str) -> Option<String>,
    published_interfaces: &'a BTreeMap<&'a str, Interface>,
    any_interface: &'a dyn Fn(&str) -> Option<Interface>,
}

/// The JS, source map, interface and, when asked, `.d.ts` and Promise
/// façade of one module of the build.
fn compiled_module(
    ctx: &Context,
    module: &Compiled,
    out: &mut Vec<Artifact>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    // the interface and its summaries come from the reference lowering
    let optimized = ctx.settings.release.then(|| {
        let mut ir = module.ir.clone();
        let exported = |name: &str| module.interface.exports_value(name);
        let generics = |m: &str, name: &str| generic_of(ctx.modules, m, name);
        opt::optimize(&mut ir, &exported, &generics);
        ir
    });
    if ctx.settings.backend == Backend::None {
        effects::check(&module.ir, diagnostics);
    }
    let output = codegen::js(optimized.as_ref().unwrap_or(&module.ir), ctx.options);
    let file = (ctx.path)(&module.name);
    let base = file.rsplit('/').next().unwrap_or(&file).to_owned();
    // an absolute source becomes relative to the map, so no build path ships
    let map_dir = normalize(&ctx.settings.out_dir.join(&file));
    let map_dir = map_dir.parent().unwrap_or(ctx.settings.out_dir);
    let source_path = |f: FileId| {
        let source = (ctx.settings.source_path)(f);
        if Path::new(&source).is_absolute() {
            specifier(map_dir, &normalize(Path::new(&source)))
        } else {
            source
        }
    };
    let map = sourcemap::source_map(&base, &output, ctx.sources, &source_path);
    out.push(Artifact {
        path: file.clone(),
        contents: format!("{}//# sourceMappingURL={base}.map\n", output.code),
    });
    out.push(Artifact {
        path: format!("{file}.map"),
        contents: map,
    });
    if ctx.settings.core_only {
        return output.imports;
    }
    let built = ctx
        .published_interfaces
        .get(module.name.as_str())
        .expect("every compiled module has a published interface");
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
            built,
            ctx.any_interface,
            ctx.package,
            ctx.path,
        ));
    }
    out.push(Artifact {
        path: format!("{}.iface", module.name.replace('.', "/")),
        contents: iface::write(built),
    });
    output.imports
}

/// The runner of an entry: starts the `main` of the module at `entry`, a
/// specifier relative to the runner, a pure one by a plain call. With the
/// file of compiled decoders `schema_aot` it collects the codecs the modules
/// make while loading and installs those decoders, which `flick build`
/// writes next to it.
fn runner(file: &str, entry: &str, start: &Start, schema_aot: Option<&str>) -> Artifact {
    let (import, run) = match start {
        Start::Call => (None, "main(undefined);"),
        Start::RunMain => (
            Some(("runMain", "@flick-lang/runtime")),
            "await runMain(main);",
        ),
        // what `run` returns is what the host takes from the module, such as Cloudflare's `fetch`
        Start::Run(module) => (Some(("run", module.as_str())), "export default run(main);"),
    };
    let import = import.map_or(String::new(), |(name, module)| {
        format!("import {{ {name} }} from \"{module}\";\n")
    });
    let contents = if let Some(aot) = schema_aot {
        format!(
            "import {{ enableSchemaAot, installSchemaAot }} from \"@flick-lang/runtime\";\n{import}\nenableSchemaAot();\nconst {{ main }} = await import(\"{entry}\");\ninstallSchemaAot(await import(\"./{aot}\"));\n{run}\n"
        )
    } else {
        format!("{import}import {{ main }} from \"{entry}\";\n\n{run}\n")
    };
    // a runner is also a package's `bin`, which a shell starts with Node
    Artifact {
        path: file.to_owned(),
        contents: format!("#!/usr/bin/env node\n{contents}"),
    }
}

/// How the runner of an entry starts `main`.
enum Start {
    /// A pure `main`, called.
    Call,
    /// An effect, run by the runtime's Node runner.
    RunMain,
    /// By `run` of a runner module, at this path from the runner.
    Run(String),
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
                    no_let_assert: false,
                    thresholds: types::Thresholds::default(),
                    effects: true,
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
             type Shape = Circle Float | Rect Float Float\n  deriving Eq, Show\n\
             class Describe a =\n  describe : a -> String\n\
             instance Describe Shape =\n  let describe s = \"shape\"\n\
             area : Shape -> Float\nlet area s =\n  match s with\n  | Circle r -> r * r\n  | Rect w h -> w * h\n\
             scale : Float -> Shape -> Shape\nlet scale k s = Circle k\n\
             now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n",
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
             double : Float -> Float\nlet double x = x + x\n\
             quad : Float -> Float\nlet quad x = double (double x)\n\
             adder : Float -> Float -> Float\nlet adder a b = a + b\n\
             add_one : Float -> Float\nlet add_one = adder 1\n\
             loop : Float -> Float\nlet rec loop x = loop x\n\
             via_loop : Float -> Float\nlet via_loop x = double (loop x)\n\
             ratio : Float -> Float\nlet ratio x = x / 2\n",
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
            ("ratio", false),
        ];
        for (name, p) in expected {
            assert!(
                partial.contains(&(name.to_owned(), p)),
                "{name}: {partial:?}"
            );
        }
    }
}
