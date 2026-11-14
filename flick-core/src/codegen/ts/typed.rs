//! What `--emit ts` adds to the JavaScript of a module, projected from the
//! interfaces outside the JS backend: the declarations of its exported
//! types, the types of its exported values and the imports they need.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use super::Projection;
use crate::codegen::{names, relative};
use crate::iface::{Interface, Namespace};
use crate::ir::Module;
use flick_backend_js::codegen::prefix_key;

pub use flick_backend_js::codegen::ts::Typed;

/// Where the projection finds what it projects, as the build has it.
pub struct Typing<'a> {
    /// The interface of a module.
    pub interface: &'a dyn Fn(&str) -> Option<Interface>,
    /// The package of a module, for constructor ids and brands.
    pub package: &'a dyn Fn(&str) -> Option<String>,
    /// The import specifier of a foreign module `spec` named in `module`.
    pub foreign: &'a dyn Fn(&str, &str) -> String,
    /// The output path of a module.
    pub path: &'a dyn Fn(&str) -> String,
    /// The projection of values compiled for Flick's own runtime.
    pub rt: bool,
}

impl Typing<'_> {
    /// The TypeScript of each file `module` is written as, by its physical
    /// module: what the build gives the JS backend with the module.
    pub fn prepare(&self, module: &Module) -> BTreeMap<String, Typed> {
        let mut prepared = BTreeMap::new();
        prepared.insert(module.name.clone(), self.typed(&module.name, &module.name));
        for prefix in &module.initialization.prefixes {
            let physical = prefix_key(&module.name, prefix.before);
            prepared.insert(physical.clone(), self.typed(&module.name, &physical));
        }
        prepared
    }

    /// The TypeScript of `module`, written as the file of `physical`: the
    /// module itself or a part of it of its own.
    pub fn typed(&self, module: &str, physical: &str) -> Typed {
        let projection = Projection::for_runtime(physical, self.interface, self.package, self.rt)
            .shipping(self.foreign);
        let (mut declarations, mut annotations) = (Vec::new(), HashMap::new());
        if let Some(mut interface) = (self.interface)(module) {
            if physical != module {
                for export in &mut interface.exports {
                    if export.entity.module == module && export.entity.namespace != Namespace::Type
                    {
                        export.entity.module = physical.into();
                    }
                }
            }
            declarations = projection.declarations(&interface).0;
            annotations = annotated(&projection, &interface);
        }
        let here = (self.path)(physical);
        let modules = RefCell::new(Vec::new());
        let path = |m: &str| {
            modules.borrow_mut().push(m.to_owned());
            relative(&here, &(self.path)(m))
        };
        let imports = projection.imports(&path);
        Typed {
            declarations,
            annotations,
            imports,
            modules: modules.into_inner(),
        }
    }
}

/// The TypeScript type of each exported value and constructor that has
/// one.
fn annotated(projection: &Projection, interface: &Interface) -> HashMap<String, String> {
    interface
        .exports
        .iter()
        .filter(|e| {
            e.entity.module == projection.module
                && matches!(
                    e.entity.namespace,
                    Namespace::Value | Namespace::Constructor
                )
        })
        .filter_map(|e| {
            let ty = projection.value(e.scheme.as_ref()?).ok()?;
            Some((names::ident(&e.entity.name), ty))
        })
        .collect()
}
