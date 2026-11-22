//! A build: the checked and lowered modules of a project, compiled to ESM
//! with source maps and `.iface` files, the built-in modules they use, and
//! for an app the runner that starts `main`.

mod abi;
mod bridge_select;
pub use crate::project::builtins::{
    BuiltinModules, BuiltinSource, BuiltinStore, builtin_bundle_of, builtin_declarations,
    builtin_modules, builtin_modules_of, keep_builtin_modules,
};
#[cfg(all(test, not(target_arch = "wasm32")))]
mod capability_tests;
#[cfg(feature = "native-project")]
pub mod go;
mod interfaces;
mod javascript;
mod paths;
#[cfg(test)]
mod shadowed_tests;
#[cfg(not(target_arch = "wasm32"))]
pub mod target_preflight;

pub use crate::effects;
pub(crate) use crate::effects::bindings;
pub use abi::interface;
pub use bridge_select::select as javascript_effect_bridge;
pub use interfaces::published_interfaces;
pub use javascript::emit;
pub use javascript::foreign_inputs as javascript_foreign_inputs;
pub use javascript::foreign_specifier as javascript_foreign_specifier;
pub use javascript::shared_files as javascript_shared_files;
#[cfg(feature = "native-project")]
pub use javascript::source_path as javascript_source_path;
pub use javascript::{
    EntryFacts, entry_facts as javascript_entries, preflight as javascript_preflight,
};

use std::path::{Path, PathBuf};

use paths::specifier;

use crate::codegen::{EffectBridge, Known, Labels};
use crate::project::Compiled;
use crate::project::config::{BackendEnvironment, MAIN_ENTRY};
use crate::source::FileId;
use crate::{ir, opt, types};
use flick_backend_support::opt::eval::{PrimitiveEval, ReviewedPrimitives};

pub use crate::codegen::BUILTIN_DIR;

/// Which JS effect runtime the native JS emitter targets (used by internal
/// emitter tests and the browser playground).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Effect,
    /// Flick's own runtime, without Effect.
    Rt,
}

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
    /// Selected backend/platform for environment-specific preflight.
    pub environment: Option<&'a BackendEnvironment>,
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

/// The module of compiled decoders an app's runner installs.
pub const SCHEMA_AOT: &str = "schemas.aot.js";

/// Whether a JS build ships the file `path` of a library into its output: a
/// foreign JS module or its TypeScript declarations.
#[cfg(feature = "native-project")]
pub fn js_ships(path: &str) -> bool {
    [".mjs", ".cjs", ".d.mts", ".d.cts", ".d.ts"]
        .iter()
        .any(|end| path.ends_with(end))
}

/// The IR of `module` of the build `modules` as a release build optimizes
/// it, for every target alike; `by_class` is the mode the build is checked in.
pub fn optimized(modules: &[Compiled], module: &Compiled, by_class: bool) -> ir::Module {
    let table = ir::lang_items();
    let known = Known::of(&table);
    let is_builtin = |owner: &str| !modules.iter().any(|m| m.name == owner);
    let primitives = ReviewedPrimitives::for_js(
        builtin_modules_of(by_class),
        &is_builtin,
        &known,
        by_class,
        EffectBridge::Absent,
        &|_, _| None,
    );
    optimized_with_primitives(modules, module, by_class, &primitives)
}

pub(super) fn optimized_with_primitives(
    modules: &[Compiled],
    module: &Compiled,
    by_class: bool,
    primitives: &dyn PrimitiveEval,
) -> ir::Module {
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
    opt::optimize_with_primitives(&mut ir, &exported, &generics, &arity, primitives);
    ir
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
