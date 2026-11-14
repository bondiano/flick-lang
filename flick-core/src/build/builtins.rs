//! Lowered built-ins and the exact text whose offsets their spans use.

use std::sync::{Arc, OnceLock};

use tracing::debug;

use crate::clock::{micros, timed};
use crate::iface::Interface;
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
}

pub fn builtin_modules() -> &'static [ir::Module] {
    builtin_modules_of(false)
}

pub fn builtin_modules_of(stages_by_class: bool) -> &'static [ir::Module] {
    &builtin_bundle_of(stages_by_class).modules
}

pub fn builtin_bundle_of(stages_by_class: bool) -> &'static BuiltinModules {
    static MODULES: [OnceLock<BuiltinModules>; 2] = [OnceLock::new(), OnceLock::new()];
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

/// A host cache preserves the complete bundle for the same compiler,
/// sources and checking mode. A legacy IR-only cache must be regenerated.
pub struct BuiltinStore {
    pub load: fn(bool) -> Option<BuiltinModules>,
    pub save: fn(bool, &BuiltinModules),
}

static BUILTIN_STORE: OnceLock<BuiltinStore> = OnceLock::new();

pub fn keep_builtin_modules(store: BuiltinStore) -> bool {
    BUILTIN_STORE.set(store).is_ok()
}

static DECLARATIONS: OnceLock<Vec<Arc<Interface>>> = OnceLock::new();

pub fn builtin_declarations() -> &'static [Arc<Interface>] {
    builtin_modules_of(false);
    DECLARATIONS.get_or_init(|| lower_builtins(false).1)
}

fn lower_builtins(stages_by_class: bool) -> (BuiltinModules, Vec<Arc<Interface>>) {
    let mut bundle = BuiltinModules {
        modules: vec![],
        sources: vec![],
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
                let mut lowered = ir::lower(module, typing).expect("built-in modules compile");
                lowered.each_span_mut(&mut |span| {
                    assert_eq!(
                        span.file, module.file,
                        "a built-in check has one source file"
                    );
                    span.file = file;
                });
                bundle.sources.push(BuiltinSource {
                    name: format!("<builtin>/{}.fl", module.name),
                    text: module.text.to_owned(),
                });
                bundle.modules.push(lowered);
                declarations.push(typing.declarations.clone());
            },
        );
        for (module, interface) in bundle.modules.iter_mut().zip(&interfaces) {
            assert_eq!(module.name, interface.module, "a module and its interface");
            module.exports = ir::exports(&module.name, interface);
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
