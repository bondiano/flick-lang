//! Lowered built-ins and the exact text whose offsets their spans use.

use std::sync::{Arc, OnceLock};

use tracing::debug;

use crate::clock::{micros, timed};
use crate::iface::Interface;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::target_preflight::BuiltinPreflight;
use crate::source::FileId;
use crate::{ir, types};

pub struct BuiltinSource {
    pub name: String,
    /// Includes generated declarations used during lowering.
    pub text: String,
}

/// One source table for the separately checked built-in modules. Its file
/// identities are local to this bundle, not to any project source map.
pub struct BuiltinModules {
    pub modules: Vec<ir::Module>,
    pub sources: Vec<BuiltinSource>,
    /// The `@native` alternatives of each module, as many as `modules`: the
    /// modules lower the clauses the default environment selects, and a
    /// build selects again ([`BuiltinModules::module_for`]).
    pub alternatives: Vec<Vec<ir::NativeAlternative>>,
}

impl BuiltinModules {
    /// Module `i` as `env` selects its `@native` clauses; as the default
    /// environment does without one.
    pub fn module_for(&self, i: usize, env: Option<&types::NativeOverrideEnv>) -> ir::Module {
        let mut module = self.modules[i].clone();
        if let (Some(env), Some(alternatives)) = (env, self.alternatives.get(i)) {
            ir::select_native(&mut module, alternatives, env);
        }
        module
    }
}

pub fn builtin_modules() -> &'static [ir::Module] {
    builtin_modules_of(false)
}

pub fn builtin_modules_of(stages_by_class: bool) -> &'static [ir::Module] {
    &builtin_bundle_of(stages_by_class).modules
}

static MODULES: [OnceLock<BuiltinModules>; 2] = [OnceLock::new(), OnceLock::new()];

pub fn builtin_bundle_of(stages_by_class: bool) -> &'static BuiltinModules {
    MODULES[usize::from(stages_by_class)].get_or_init(|| {
        let store = BUILTIN_STORE.get();
        if let Some(kept) = store.and_then(|store| (store.load)(stages_by_class)) {
            return kept;
        }
        let (modules, declarations) = lower_builtins(stages_by_class);
        if !stages_by_class {
            let _ = DECLARATIONS.set(declarations);
        }
        if let Some(store) = store {
            (store.save)(stages_by_class, &modules);
        }
        modules
    })
}

/// What environment preflight reads of the built-in modules. A loaded bundle
/// is read as it is; otherwise the summary a host kept spares loading the
/// whole lowered standard library, which an editor otherwise holds for good.
#[cfg(not(target_arch = "wasm32"))]
pub fn builtin_preflight_of(stages_by_class: bool) -> &'static BuiltinPreflight {
    static KEPT: [OnceLock<BuiltinPreflight>; 2] = [OnceLock::new(), OnceLock::new()];
    KEPT[usize::from(stages_by_class)].get_or_init(|| {
        let store = BUILTIN_STORE.get();
        if MODULES[usize::from(stages_by_class)].get().is_none()
            && let Some(kept) = store.and_then(|store| (store.load_preflight)(stages_by_class))
        {
            return kept;
        }
        let made = BuiltinPreflight::of(builtin_modules_of(stages_by_class));
        if let Some(store) = store {
            (store.save_preflight)(stages_by_class, &made);
        }
        made
    })
}

/// A host cache preserves the complete bundle for the same compiler,
/// sources and checking mode, and what preflight reads of it. A legacy
/// IR-only cache must be regenerated.
pub struct BuiltinStore {
    pub load: fn(bool) -> Option<BuiltinModules>,
    pub save: fn(bool, &BuiltinModules),
    #[cfg(not(target_arch = "wasm32"))]
    pub load_preflight: fn(bool) -> Option<BuiltinPreflight>,
    #[cfg(not(target_arch = "wasm32"))]
    pub save_preflight: fn(bool, &BuiltinPreflight),
}

static BUILTIN_STORE: OnceLock<BuiltinStore> = OnceLock::new();

pub fn keep_builtin_modules(store: BuiltinStore) -> bool {
    BUILTIN_STORE.set(store).is_ok()
}

static DECLARATIONS: OnceLock<Vec<Arc<Interface>>> = OnceLock::new();

/// The laws of the built-in classes, lowered. Only a proof reads them, so
/// they are checked again when one first does, apart from the bundle a host
/// keeps.
pub fn builtin_laws(stages_by_class: bool) -> &'static [ir::Proven] {
    static LAWS: [OnceLock<Vec<ir::Proven>>; 2] = [OnceLock::new(), OnceLock::new()];
    LAWS[usize::from(stages_by_class)].get_or_init(|| {
        let mut laws = Vec::new();
        types::check_each(
            types::builtin_sources(),
            stages_by_class,
            |module, typing| {
                if !typing.laws.is_empty() {
                    laws.extend(ir::laws_of(module, typing));
                }
            },
        );
        laws
    })
}

pub fn builtin_declarations() -> &'static [Arc<Interface>] {
    builtin_modules_of(false);
    DECLARATIONS.get_or_init(|| lower_builtins(false).1)
}

fn lower_builtins(stages_by_class: bool) -> (BuiltinModules, Vec<Arc<Interface>>) {
    let mut bundle = BuiltinModules {
        modules: vec![],
        sources: vec![],
        alternatives: vec![],
    };
    let mut declarations = Vec::new();
    let ((), elapsed) = timed(|| {
        let interfaces = types::check_each(
            types::builtin_sources(),
            stages_by_class,
            |module, typing| {
                let file = FileId::new(
                    u32::try_from(bundle.sources.len()).expect("built-in source count fits u32"),
                );
                #[cfg(not(target_arch = "wasm32"))]
                let (mut lowered, local_schemes) =
                    ir::lower_with_local_schemes(module, typing).expect("built-in modules compile");
                #[cfg(target_arch = "wasm32")]
                let mut lowered = ir::lower(module, typing).expect("built-in modules compile");
                let mut relocate = |span: &mut flick_ir::Span| {
                    assert_eq!(
                        span.file, module.file,
                        "a built-in check has one source file"
                    );
                    span.file = file;
                };
                lowered.each_span_mut(&mut relocate);
                // the browser compiler builds for the default environment alone
                #[cfg(not(target_arch = "wasm32"))]
                let mut alternatives =
                    ir::native_alternatives(module, typing).expect("built-in modules compile");
                #[cfg(target_arch = "wasm32")]
                let mut alternatives: Vec<ir::NativeAlternative> = Vec::new();
                for alternative in &mut alternatives {
                    alternative.each_span_mut(&mut relocate);
                }
                bundle.alternatives.push(alternatives);
                bundle.sources.push(BuiltinSource {
                    name: format!("<builtin>/{}.fl", module.name),
                    text: module.text.to_owned(),
                });
                bundle.modules.push(lowered);
                #[cfg(not(target_arch = "wasm32"))]
                let mut decl = typing.declarations.clone();
                #[cfg(not(target_arch = "wasm32"))]
                if !local_schemes.is_empty() {
                    Arc::make_mut(&mut decl).local_schemes = local_schemes;
                }
                #[cfg(target_arch = "wasm32")]
                let decl = typing.declarations.clone();
                declarations.push(decl);
            },
        );
        for (module, interface) in bundle.modules.iter_mut().zip(&interfaces) {
            assert_eq!(module.name, interface.module, "a module and its interface");
            ir::fill_exports(module, interface);
        }
    });
    debug!(
        modules = bundle.modules.len(),
        ?elapsed,
        micros = micros(elapsed),
        "built-in modules lowered"
    );
    (bundle, declarations)
}
