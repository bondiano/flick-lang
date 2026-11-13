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

#[cfg(test)]
mod tests {
    use flick_ir::{Exported, Namespace, Reexport};

    use crate::ir::tests::lower_text;

    #[test]
    fn a_module_lists_its_own_values_and_what_it_exports_of_others() {
        let text = "module App exposing (Enum, count, plain, doubled, scaled, wait)\n\n\
                    import Enum exposing (Enum)\n\n\
                    count : Atom.Writable Float\nlet count = Atom.make 0\n\n\
                    plain : Float -> Float -> Float\nlet plain a b = a + b\n\n\
                    doubled : Unit ->{Atom.Read} Float\nlet doubled () = Atom.value count * 2\n\n\
                    scaled : Float -> Unit ->{Atom.Read} Float\n\
                    let scaled by () = Atom.value count * by\n\n\
                    wait : Float ->{Async;} Float\nlet wait n = n\n\n\
                    hidden : Unit ->{Atom.Read} Float\nlet hidden () = Atom.value count\n";
        let exports = lower_text("App", text).expect("lowers").exports;
        let mut values = exports.values;
        values.sort_by(|a, b| a.name.cmp(&b.name));
        let value = |name: &str, sync| Exported {
            name: name.to_owned(),
            sync,
        };
        // only a function whose last stage is Sync counts its arrows; what
        // the module keeps to itself is not listed
        assert_eq!(
            values,
            [
                value("count", None),
                value("doubled", Some(1)),
                value("plain", None),
                value("scaled", Some(2)),
                value("wait", None),
            ]
        );
        assert_eq!(
            exports.reexports,
            [Reexport {
                module: "Enum".to_owned(),
                namespace: Namespace::Type,
                name: "Enum".to_owned(),
            }]
        );
    }
}
