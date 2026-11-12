//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps and `.iface` files, the built-in modules they use, and
//! for an app the runner that starts `main`.

mod abi;
mod bridge_select;
#[cfg(not(target_arch = "wasm32"))]
mod capabilities;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod capability_tests;
#[cfg(feature = "native-project")]
pub mod go;

pub use crate::effects;
pub(crate) use crate::effects::bindings;
pub use abi::interface;

use std::collections::{BTreeMap, BTreeSet};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tracing::debug;

use crate::clock::{micros, timed};

#[cfg(feature = "native-project")]
use crate::codegen::sourcemap;
use crate::codegen::ts::{Facade, Projection, Typing, declaration_file, facade_files};
use crate::codegen::{self, EffectBridge, Known, Labels, Options};
use crate::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "native-project")]
use crate::iface;
use crate::iface::{Interface, Namespace};
use crate::project::config::{Backend, MAIN_ENTRY};
use crate::project::{Compilation, Compiled, MainKind, main_kind, normalize};
use crate::source::{FileId, SourceMap, Span};
use crate::{ir, opt, types};

pub use crate::codegen::BUILTIN_DIR;

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
    /// Whether the project has the optional Effect adapter for RT fallbacks.
    pub effect_bridge: EffectBridge,
    /// The module that renders and mounts views in place of React's; a
    /// relative one is relative to the project's root.
    pub ui: Option<&'a str>,
    /// The module whose `run` starts an entry, by the entry's name; an entry
    /// without one starts the default way.
    pub runners: &'a [(String, String)],
}

/// The opt-in outputs of a build.
#[derive(Debug, Clone, Copy, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag asks for one thing of a build"
)]
pub struct Extras {
    /// Write `A/B.d.ts`, the public TypeScript interface of each module.
    pub dts: bool,
    /// Write `A/B.promise.js`: Promise façades of runtime functions.
    pub promise_facade: bool,
    /// Build for Flick's own runtime, whatever the project's `build.effect`.
    pub rt: bool,
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
    #[cfg(not(feature = "native-project"))]
    let _ = sources;
    let mut diagnostics = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    if settings.backend == Backend::Rt && !settings.effect_bridge.available() {
        capabilities::check(compilation, settings, sources, &mut diagnostics);
        if !diagnostics.is_empty() {
            return (Vec::new(), diagnostics);
        }
    }
    let modules = &compilation.modules;
    let builtin_names: BTreeSet<&str> = types::builtin_sources().iter().map(|(n, _)| *n).collect();
    let path = |m: &str| codegen::module_path(&builtin_names, settings.typescript, m);
    let packages: BTreeMap<String, String> = (modules.iter())
        .filter_map(|c| Some((c.name.clone(), c.ir.package.clone()?)))
        .collect();
    let package = |m: &str| packages.get(m).cloned();
    let published_interfaces = published_interfaces(compilation, settings);
    let interface_of = |m: &str| published_interfaces.get(m).cloned();
    let foreign = |module: &str, spec: &str| {
        // a library's own file ships beside its module, so its spelling holds
        if compilation.shipped_foreign(module, spec).is_some() {
            spec.to_owned()
        } else {
            foreign_specifier(settings, &path, module, spec)
        }
    };
    let by_class = settings.backend == Backend::Rt;
    let any_interface = |m: &str| {
        interface_of(m).or_else(|| {
            types::builtins_of(by_class)
                .iter()
                .find(|i| i.module == m)
                .cloned()
        })
    };
    let (workers, direct) = exports(settings.release, modules, &any_interface, by_class);
    let resolved = resolved(compilation, settings, by_class, &foreign);
    let lang = ir::lang_items();
    let known = Known::of(&lang);
    let typing = Typing {
        interface: &any_interface,
        package: &package,
        foreign: &foreign,
        path: &path,
        rt: by_class,
    };
    let options = Options {
        builtins: &builtin_names,
        packages: &packages,
        resolved: &resolved,
        typescript: settings.typescript.then_some(&typing),
        release: settings.release,
        workers: &workers,
        direct: &direct,
        labels: settings.extras.labels,
        rt: settings.backend == Backend::Rt,
        effect_bridge: bridge_select::select(compilation, settings.effect_bridge),
        lang: &known,
    };
    let mut out = Vec::new();
    #[cfg(feature = "native-project")]
    out.push(package_json(settings));
    let layout = codegen::RuntimeLayout::new(compilation.modules.iter().map(|m| &m.ir));
    let ctx = Context {
        layout: &layout,
        modules: &compilation.modules,
        settings,
        #[cfg(feature = "native-project")]
        sources,
        options: &options,
        path: &path,
        package: &package,
        foreign: &foreign,
        published_interfaces: &published_interfaces,
        any_interface: &any_interface,
    };
    // a built-in runner is written like any module the program imports
    let mut imports: Vec<String> = settings.runners.iter().map(|(_, r)| r.clone()).collect();
    imports.extend(own_modules(&ctx, compilation, &mut out, &mut diagnostics));
    builtins(&ctx, imports, &mut out);
    entry_runners(settings, modules, &path, &mut out, &mut diagnostics);
    (out, diagnostics)
}

/// The specifiers of the foreign modules of `compilation` and the built-in
/// modules that the build resolves, by module and specifier: one named
/// relative to its module, shipped beside it, or the renderer `[build] ui`
/// stands for. The JS backend resolves the others, a package's, itself.
fn resolved(
    compilation: &Compilation,
    settings: &Settings,
    by_class: bool,
    foreign: &dyn Fn(&str, &str) -> String,
) -> BTreeMap<(String, String), String> {
    let own = compilation.modules.iter().map(|c| &c.ir);
    let mut out = BTreeMap::new();
    for module in own.chain(builtin_modules_of(by_class)) {
        for item in &module.items {
            let ir::ItemKind::Foreign(f) = &item.kind else {
                continue;
            };
            let Some(spec) = f.binding.from.as_deref() else {
                continue;
            };
            let ui = settings.ui.is_some() && bindings::UI_RENDERER.contains(&spec);
            let relative = spec.starts_with("./") || spec.starts_with("../");
            if ui || relative || compilation.shipped_foreign(&module.name, spec).is_some() {
                let key = (module.name.clone(), spec.to_owned());
                out.entry(key)
                    .or_insert_with(|| foreign(&module.name, spec));
            }
        }
    }
    out
}

fn foreign_specifier(
    settings: &Settings,
    path: &dyn Fn(&str) -> String,
    module: &str,
    spec: &str,
) -> String {
    // `[build] ui` stands for the renderer and the mount of `Ui` and `Action`.
    let (spec, dir) = match settings.ui {
        Some(ui) if bindings::UI_RENDERER.contains(&spec) => {
            (ui, settings.root.map(Path::to_path_buf))
        }
        _ => (spec, (settings.source_dir)(module)),
    };
    let relative = spec.starts_with("./") || spec.starts_with("../");
    // a foreign module may have a version for Flick's own runtime beside it
    let beside = spec
        .strip_suffix(".mjs")
        .map(|stem| format!("{stem}.rt.mjs"))
        .filter(|_| settings.backend == Backend::Rt && relative)
        .filter(|rt| dir.as_ref().is_some_and(|d| d.join(rt).is_file()));
    let spec = beside.as_deref().unwrap_or(spec);
    if !relative {
        return effects::runtime_specifier(settings.backend == Backend::Rt, spec);
    }
    match (relative, dir) {
        (true, Some(dir)) => {
            let target = normalize(&dir.join(spec));
            let from = settings.out_dir.join(path(module));
            specifier(from.parent().unwrap_or(settings.out_dir), &target)
        }
        _ => spec.to_owned(),
    }
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
    };
    let published_interfaces: BTreeMap<&str, Interface> = modules
        .iter()
        .map(|module| {
            let mut published = interface(
                &module.ir,
                &module.interface,
                module.package.as_deref().or(settings.package),
                &[],
                &checked_interfaces,
            );
            published.source = settings
                .root
                .and_then(|root| module.path.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            (module.name.as_str(), published)
        })
        .collect();
    #[cfg(feature = "native-project")]
    let mut published_interfaces = published_interfaces;
    #[cfg(feature = "native-project")]
    {
        // Hash only finalized ABI, including backend availability, exactly as published.
        let fingerprints: BTreeMap<&str, String> = published_interfaces
            .iter()
            .map(|(name, interface)| (*name, iface::fingerprint(interface)))
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
                        _ if settings.backend == Backend::Rt => Start::RunMainRt,
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
    let modules = compilation.modules.iter();
    modules
        .flat_map(|module| compiled_module(ctx, module, out, diagnostics))
        .collect()
}

#[cfg(feature = "native-project")]
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
    by_class: bool,
) -> Exports {
    let mut out = Exports::default();
    if !release {
        return out;
    }
    let own = modules.iter().map(|c| (&c.name, &c.ir.items));
    let builtin = builtin_modules_of(by_class)
        .iter()
        .map(|m| (&m.name, &m.items));
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
fn builtins(ctx: &Context, imports: Vec<String>, out: &mut Vec<Artifact>) {
    let dts = ctx.settings.extras.dts;
    let mut pending = imports;
    let lowered = builtin_modules_of(ctx.settings.backend == Backend::Rt);
    if dts {
        pending = lowered.iter().map(|module| module.name.clone()).collect();
    }
    let mut written = HashSet::new();
    while let Some(name) = pending.pop() {
        let Some(module) = lowered.iter().find(|m| m.name == name) else {
            continue;
        };
        if !written.insert(name) {
            continue;
        }
        let output = codegen::js(module, ctx.options);
        pending.extend(output.imports);
        out.push(Artifact {
            path: (ctx.path)(&module.name),
            contents: output.code,
        });
        if dts
            && let Some((text, _)) = declarations(
                &module.name,
                ctx.any_interface,
                ctx.package,
                ctx.path,
                ctx.foreign,
                ctx.settings.backend == Backend::Rt,
            )
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
fn generic_of(
    modules: &[Compiled],
    module: &str,
    name: &str,
    by_class: bool,
) -> Option<(Vec<String>, ir::Expr)> {
    if let Some(m) = modules.iter().find(|m| m.name == module) {
        return opt::names_only(&m.ir, name, &|n| m.interface.exports_value(n));
    }
    let ir = builtin_modules_of(by_class)
        .iter()
        .find(|m| m.name == module)?;
    let interface = types::builtins_of(by_class)
        .iter()
        .find(|i| i.module == module)?;
    opt::names_only(ir, name, &|n| interface.exports_value(n))
}

/// How many arguments `name` of `module` takes before its body runs, for
/// the optimizer: a module of the build, else a built-in one.
fn arity_of(modules: &[Compiled], module: &str, name: &str, by_class: bool) -> Option<usize> {
    let items = match modules.iter().find(|m| m.name == module) {
        Some(m) => &m.ir.items,
        None => {
            &builtin_modules_of(by_class)
                .iter()
                .find(|m| m.name == module)?
                .items
        }
    };
    (items.iter().filter_map(opt::arity)).find_map(|(n, arity)| (n == name).then_some(arity))
}

/// What the compilation of every module shares.
struct Context<'a> {
    layout: &'a codegen::RuntimeLayout,
    modules: &'a [Compiled],
    settings: &'a Settings<'a>,
    #[cfg(feature = "native-project")]
    sources: &'a SourceMap,
    options: &'a Options<'a>,
    path: &'a dyn Fn(&str) -> String,
    package: &'a dyn Fn(&str) -> Option<String>,
    foreign: &'a dyn Fn(&str, &str) -> String,
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
    let by_class = ctx.settings.backend == Backend::Rt;
    let optimized = ctx
        .settings
        .release
        .then(|| optimized(ctx.modules, module, by_class));
    // Flick's own runtime has `control` arms, and takes what it lacks when it is
    // called: a module binds more than a program uses
    if ctx.settings.backend != Backend::Rt {
        effects::control_on_js(&module.ir, diagnostics);
    }
    let mut imports = Vec::new();
    for (physical, output) in codegen::js_parts(
        optimized.as_ref().unwrap_or(&module.ir),
        ctx.options,
        ctx.layout,
    ) {
        imports.extend(write_js_output(ctx, &(ctx.path)(&physical), output, out));
    }
    let file = (ctx.path)(&module.name);
    let built = ctx
        .published_interfaces
        .get(module.name.as_str())
        .expect("every compiled module has a published interface");
    if ctx.settings.extras.dts
        && let Some((text, refused)) = declarations(
            &module.name,
            ctx.any_interface,
            ctx.package,
            ctx.path,
            ctx.foreign,
            ctx.settings.backend == Backend::Rt,
        )
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
            ctx.foreign,
            ctx.settings.backend == Backend::Rt,
        ));
    }
    #[cfg(feature = "native-project")]
    out.push(Artifact {
        path: format!("{}.iface", module.name.replace('.', "/")),
        contents: iface::write(built),
    });
    imports
}

fn write_js_output(
    ctx: &Context,
    file: &str,
    output: codegen::Output,
    out: &mut Vec<Artifact>,
) -> Vec<String> {
    #[cfg(not(feature = "native-project"))]
    let _ = ctx;
    #[cfg(feature = "native-project")]
    {
        let base = file.rsplit('/').next().unwrap_or(file);
        // an absolute source becomes relative to the map, so no build path ships
        let map_dir = normalize(&ctx.settings.out_dir.join(file));
        let map_dir = map_dir.parent().unwrap_or(ctx.settings.out_dir);
        let source_path = |file: FileId| {
            let source = (ctx.settings.source_path)(file);
            if Path::new(&source).is_absolute() {
                specifier(map_dir, &normalize(Path::new(&source)))
            } else {
                source
            }
        };
        let map = sourcemap::source_map(base, &output, ctx.sources, &source_path);
        out.push(Artifact {
            path: file.to_owned(),
            contents: format!("{}//# sourceMappingURL={base}.map\n", output.code),
        });
        out.push(Artifact {
            path: format!("{file}.map"),
            contents: map,
        });
    }
    #[cfg(not(feature = "native-project"))]
    {
        let path = file.to_owned();
        out.push(Artifact {
            path,
            contents: output.code,
        });
    }
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
            Some(("runMain", bindings::ADAPTER_PREFIX)),
            "await runMain(main);",
        ),
        Start::RunMainRt => (
            Some(("runMain", bindings::NATIVE_ENTRY)),
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
            "import {{ enableSchemaAot, installSchemaAot }} from \"{}\";\n{import}\nenableSchemaAot();\nconst {{ main }} = await import(\"{entry}\");\ninstallSchemaAot(await import(\"./{aot}\"));\n{run}\n",
            bindings::ADAPTER_PREFIX
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
    /// [`Start::RunMain`] on Flick's own runtime.
    RunMainRt,
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
    foreign: &dyn Fn(&str, &str) -> String,
    rt: bool,
) -> Option<(String, Vec<(String, String)>)> {
    let projection = Projection::for_runtime(name, interfaces, package, rt).shipping(foreign);
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
    foreign: &dyn Fn(&str, &str) -> String,
    rt: bool,
) -> Vec<Artifact> {
    // the façade module names the module's own types through an import
    let facade_module = format!("{}.promise", module.name);
    let projection =
        Projection::for_runtime(&facade_module, interfaces, package, rt).shipping(foreign);
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

/// The IR of `module` of the build `modules` as a release build optimizes
/// it, for every target alike; `by_class` is the mode the build is checked in.
pub fn optimized(modules: &[Compiled], module: &Compiled, by_class: bool) -> ir::Module {
    let mut ir = module.ir.clone();
    let exported = |name: &str| {
        module.interface.exports_value(name)
            || module
                .ir
                .initialization
                .prefixes
                .iter()
                .any(|p| p.bindings.iter().any(|n| n == name))
    };
    let generics = |m: &str, name: &str| generic_of(modules, m, name, by_class);
    let arity = |m: &str, name: &str| arity_of(modules, m, name, by_class);
    opt::optimize(&mut ir, &exported, &generics, &arity);
    ir
}

/// The built-in modules, lowered.
pub fn builtin_modules() -> &'static [ir::Module] {
    builtin_modules_of(false)
}

/// The built-in modules, lowered from a check with `stages_by_class`: read
/// from the host's store, or checked and lowered once and given to it.
pub fn builtin_modules_of(stages_by_class: bool) -> &'static [ir::Module] {
    static MODULES: [OnceLock<Vec<ir::Module>>; 2] = [OnceLock::new(), OnceLock::new()];
    MODULES[usize::from(stages_by_class)].get_or_init(|| {
        let store = BUILTIN_STORE.get();
        if let Some(kept) = store.and_then(|store| (store.load)(stages_by_class)) {
            return kept;
        }
        let (modules, declarations) = lower_builtins(stages_by_class);
        if !stages_by_class {
            // the same pass serves the reference model
            let _ = DECLARATIONS.set(declarations);
        }
        if let Some(store) = store {
            (store.save)(stages_by_class, &modules);
        }
        modules
    })
}

/// Where a host keeps the lowered built-in modules between its processes:
/// lowering them takes longer than building a small project. What `load`
/// gives must be what `save` got from this compiler for the same sources of
/// the built-in modules and the same mode of `stages_by_class`. Asked for
/// when a build first reads a mode, which only the emitters decide: a host
/// that installed them up front would read both modes for every build.
pub struct BuiltinStore {
    pub load: fn(bool) -> Option<Vec<ir::Module>>,
    pub save: fn(bool, &[ir::Module]),
}

static BUILTIN_STORE: OnceLock<BuiltinStore> = OnceLock::new();

/// Keeps the lowered built-in modules in `store` from now on; false when a
/// store is set already.
pub fn keep_builtin_modules(store: BuiltinStore) -> bool {
    BUILTIN_STORE.set(store).is_ok()
}

static DECLARATIONS: OnceLock<Vec<Arc<Interface>>> = OnceLock::new();

/// What the reference model reads of each built-in module, in the order of
/// [`builtin_modules`].
pub fn builtin_declarations() -> &'static [Arc<Interface>] {
    // lowering the modules fills this too, unless a store had them
    builtin_modules_of(false);
    DECLARATIONS.get_or_init(|| lower_builtins(false).1)
}

/// The built-in modules lowered, with what the reference model reads of each.
type Lowered = (Vec<ir::Module>, Vec<Arc<Interface>>);

fn lower_builtins(stages_by_class: bool) -> Lowered {
    let (mut lowered, mut declarations) = (Vec::new(), Vec::new());
    let ((), elapsed) = timed(|| {
        let interfaces = types::check_each(
            types::builtin_sources(),
            stages_by_class,
            |module, typing| {
                lowered.push(ir::lower(module, typing).expect("built-in modules compile"));
                declarations.push(typing.declarations.clone());
            },
        );
        for (module, interface) in lowered.iter_mut().zip(&interfaces) {
            assert_eq!(module.name, interface.module, "a module and its interface");
            module.exports = ir::exports(&module.name, interface);
        }
    });
    debug!(
        modules = lowered.len(),
        ?elapsed,
        micros = micros(elapsed),
        "built-in modules lowered"
    );
    (lowered, declarations)
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
             now : Unit ->{} Float\nforeign now from global \"Date.now\"\n",
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
