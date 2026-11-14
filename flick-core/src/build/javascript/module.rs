//! Modules, source maps and shared TypeScript outputs of the native build.

#[cfg(feature = "native-project")]
use std::path::Path;

use crate::codegen;
#[cfg(feature = "native-project")]
use crate::codegen::sourcemap;
use crate::codegen::ts::{Facade, Projection, Typing, declaration_file, facade_files};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::effects;
#[cfg(feature = "native-project")]
use crate::iface;
use crate::iface::{Interface, Namespace};
use crate::project::Compiled;
use crate::project::config::Backend;
#[cfg(feature = "native-project")]
use crate::project::normalize;
#[cfg(feature = "native-project")]
use crate::source::FileId;

#[cfg(feature = "native-project")]
use super::super::specifier;
use super::super::{Artifact, optimized};
use super::{Context, typed_options};

/// The JS, source map, interface and, when asked, `.d.ts` and Promise
/// façade of one module of the build.
pub(super) fn compiled_module(
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
    let ir = optimized.as_ref().unwrap_or(&module.ir);
    let parts = typed_options(ctx, ir, |options| {
        codegen::js_parts(ir, options, ctx.layout)
    });
    for (physical, output) in parts {
        imports.extend(write_js_output(ctx, &(ctx.path)(&physical), output, out));
    }
    let built = ctx
        .published_interfaces
        .get(module.name.as_str())
        .expect("every compiled module has a published interface");
    let typing = Typing {
        interface: ctx.any_interface,
        package: ctx.package,
        foreign: ctx.foreign,
        path: ctx.path,
        rt: ctx.settings.backend == Backend::Rt,
    };
    let (shared, issues) = shared_files(
        &module.name,
        built,
        &typing,
        ctx.settings.extras.dts,
        ctx.settings.extras.promise_facade,
    );
    out.extend(shared);
    diagnostics.extend(issues);
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

/// The `.d.ts` of module `name`, and the exports it refused with why.
pub(super) fn declarations(
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
pub(super) fn dts_path(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    format!("{stem}.d.ts")
}

/// The Promise façade of `module`: each exported runtime function that
/// needs no service and whose earlier stages only make closures.
fn facade(
    module: &str,
    built: &Interface,
    interfaces: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    path: &dyn Fn(&str) -> String,
    foreign: &dyn Fn(&str, &str) -> String,
    rt: bool,
) -> Vec<Artifact> {
    // the façade module names the module's own types through an import
    let facade_module = format!("{module}.promise");
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
        .filter(|e| e.entity.namespace == Namespace::Value && e.entity.module == module)
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
    let file = path(module);
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

/// Declarations and Promise facades share the compiler's projection rules.
/// These host outputs need only plain interfaces and ABI summaries; they
/// do not require a checker call from the backend component.
pub fn shared_files(
    module: &str,
    built: &Interface,
    typing: &Typing<'_>,
    dts: bool,
    promise_facade: bool,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    if dts
        && let Some((text, refused)) = declarations(
            module,
            typing.interface,
            typing.package,
            typing.path,
            typing.foreign,
            typing.rt,
        )
    {
        out.push(Artifact {
            path: dts_path(&(typing.path)(module)),
            contents: text,
        });
        for (name, reason) in refused {
            diagnostics.push(Diagnostic::new(
                codes::F004,
                Severity::Error,
                format!("`{module}.{name}` has no TypeScript form: {reason}"),
            ));
        }
    }
    if promise_facade {
        out.extend(facade(
            module,
            built,
            typing.interface,
            typing.package,
            typing.path,
            typing.foreign,
            typing.rt,
        ));
    }
    (out, diagnostics)
}
