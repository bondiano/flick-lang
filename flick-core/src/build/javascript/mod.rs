//! Native JavaScript build orchestration and its shared frontend inputs.

mod module;
pub use module::shared_files;
#[cfg(feature = "native-project")]
pub use module::source_path;
mod runner;
pub use runner::{EntryFacts, entry_facts};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::codegen::ts::Typing;
use crate::codegen::{self, Known, Options, Resolved};
use crate::diagnostics::Diagnostic;
use crate::iface::Interface;
use crate::project::config::Backend;
use crate::project::{Compilation, Compiled, normalize};
use crate::source::SourceMap;
use crate::{ir, types};

#[cfg(not(target_arch = "wasm32"))]
use super::capabilities;
#[cfg(feature = "native-project")]
use super::runner_file;
use super::{
    Artifact, Settings, bindings, bridge_select, builtin_modules_of, effects, published_interfaces,
    specifier,
};
use module::{compiled_module, declarations, dts_path};
use runner::entry_runners;

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
    preflight(compilation, sources, settings, &mut diagnostics);
    if !diagnostics.is_empty() {
        return (Vec::new(), diagnostics);
    }
    let modules = &compilation.modules;
    let builtin_names: Vec<&str> = types::builtin_sources().iter().map(|(n, _)| *n).collect();
    let path = |m: &str| codegen::module_path(&builtin_names, settings.typescript, m);
    let mut packages = BTreeMap::new();
    for c in modules {
        if let Some(package) =
            c.ir.package
                .as_deref()
                .or(c.package.as_deref())
                .or(settings.package)
        {
            packages.insert(c.name.clone(), package.to_owned());
        }
    }
    let package = |m: &str| packages.get(m).cloned();
    let published_interfaces =
        published_interfaces(compilation, settings.package, settings.root, None);
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
    // each module gets its own as the build writes it (see `typed_options`)
    let untyped = BTreeMap::new();
    let options = Options {
        builtins: &builtin_names,
        paths: None,
        packages: &packages,
        resolved: &resolved,
        typescript: settings.typescript.then_some(&untyped),
        release: settings.release,
        workers: &workers,
        direct: &direct,
        labels: settings.extras.labels,
        rt: settings.backend == Backend::Rt,
        effect_bridge: bridge_select::select(compilation, settings.effect_bridge, settings.runners),
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
        typing: settings.typescript.then_some(&typing),
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

/// Common runtime capability checks before native or component generation.
pub fn preflight(
    compilation: &Compilation,
    sources: &SourceMap,
    settings: &Settings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    if settings.backend == Backend::Rt && !settings.effect_bridge.available() {
        capabilities::check(compilation, settings, sources, diagnostics);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (compilation, sources, settings, diagnostics);
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
) -> Vec<Resolved> {
    let own = compilation.modules.iter().map(|c| &c.ir);
    let mut out: Vec<Resolved> = Vec::new();
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
            let known = |r: &Resolved| r.module == module.name && r.spec == spec;
            if (ui || relative || compilation.shipped_foreign(&module.name, spec).is_some())
                && !out.iter().any(known)
            {
                out.push(Resolved {
                    module: module.name.clone(),
                    spec: spec.to_owned(),
                    specifier: foreign(&module.name, spec),
                });
            }
        }
    }
    out
}

/// Host-resolved foreign specifiers as plain data. A component never needs
/// a filesystem lookup or a callback to the compiler to reproduce them.
pub fn foreign_inputs(compilation: &Compilation, settings: &Settings) -> Vec<Resolved> {
    let builtins: Vec<_> = types::builtin_sources()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let path = |m: &str| codegen::module_path(&builtins, settings.typescript, m);
    let foreign = |module: &str, spec: &str| {
        if compilation.shipped_foreign(module, spec).is_some() {
            spec.to_owned()
        } else {
            foreign_specifier(settings, &path, module, spec)
        }
    };
    let own = compilation.modules.iter().map(|m| &m.ir);
    let mut out: Vec<Resolved> = Vec::new();
    for module in own.chain(builtin_modules_of(settings.backend == Backend::Rt)) {
        for item in &module.items {
            if let ir::ItemKind::Foreign(f) = &item.kind
                && let Some(spec) = f.binding.from.as_deref()
                && !out
                    .iter()
                    .any(|r| r.module == module.name && r.spec == spec)
            {
                out.push(Resolved {
                    module: module.name.clone(),
                    spec: spec.to_owned(),
                    specifier: foreign(&module.name, spec),
                });
            }
        }
    }
    out
}

pub fn foreign_specifier(
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
        let output = typed_options(ctx, module, |options| codegen::js(module, options));
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

/// What the compilation of every module shares.
struct Context<'a> {
    layout: &'a codegen::RuntimeLayout,
    modules: &'a [Compiled],
    settings: &'a Settings<'a>,
    #[cfg(feature = "native-project")]
    sources: &'a SourceMap,
    options: &'a Options<'a>,
    /// In TypeScript, what projects the types of a module's exports.
    typing: Option<&'a Typing<'a>>,
    path: &'a dyn Fn(&str) -> String,
    package: &'a dyn Fn(&str) -> Option<String>,
    foreign: &'a dyn Fn(&str, &str) -> String,
    published_interfaces: &'a BTreeMap<&'a str, Interface>,
    any_interface: &'a dyn Fn(&str) -> Option<Interface>,
}

/// `js` with the options of `module`'s code: in TypeScript they hold the
/// types of its exports, projected here, for the modules the build writes
/// and no others.
fn typed_options<R>(ctx: &Context, module: &ir::Module, js: impl FnOnce(&Options) -> R) -> R {
    let typed = ctx.typing.map(|typing| typing.prepare(module));
    js(&Options {
        typescript: typed.as_ref().or(ctx.options.typescript),
        ..*ctx.options
    })
}
