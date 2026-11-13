//! The exports of a module as its IR gives them to the backend: its own
//! values, with the stages of a Sync one, and what it exports of others.

use flick_ir::{Exported, Exports, Namespace as IrNamespace, Reexport};

use crate::iface::{Decl, Interface, Namespace};
use crate::types::Type;

/// The exports `interface` lists for `module`.
pub fn exports(module: &str, interface: &Interface) -> Exports {
    let mut out = Exports::default();
    for export in &interface.exports {
        let entity = &export.entity;
        if entity.module == module {
            if entity.namespace == Namespace::Value {
                let sync = export.scheme.as_ref().and_then(sync_stages);
                let name = entity.name.clone();
                out.values.push(Exported { name, sync });
            }
            continue;
        }
        let namespace = match entity.namespace {
            _ if matches!(export.decl, Decl::Method | Decl::Blocked) => continue,
            Namespace::Value => IrNamespace::Value,
            Namespace::Constructor => IrNamespace::Constructor,
            Namespace::Type => IrNamespace::Type,
        };
        out.reexports.push(Reexport {
            module: entity.module.clone(),
            namespace,
            name: entity.name.clone(),
        });
    }
    out
}

/// The supplied arrows of an exported value whose last stage is Sync.
fn sync_stages(scheme: &Type) -> Option<usize> {
    let mut ty = scheme;
    let mut stages = 0;
    loop {
        match ty {
            Type::Forall { body, .. } => ty = body,
            Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } => ty = inner,
            Type::Fun {
                effects, result, ..
            } => {
                stages += 1;
                match effects {
                    None => ty = result,
                    Some(effects) => return effects.sync.then_some(stages),
                }
            }
            _ => return None,
        }
    }
}
