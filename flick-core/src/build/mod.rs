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
mod interfaces;
mod javascript;

pub use crate::effects;
pub(crate) use crate::effects::bindings;
pub use abi::interface;
pub use interfaces::published_interfaces;
pub use javascript::emit;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tracing::debug;

use crate::clock::{micros, timed};
use crate::codegen::{EffectBridge, Labels};
use crate::iface::Interface;
use crate::project::Compiled;
use crate::project::config::{Backend, MAIN_ENTRY};
use crate::source::FileId;
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
