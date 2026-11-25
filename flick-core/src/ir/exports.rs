//! The exports of a module as its IR gives them to the backend: its own
//! values, with the stages of a Sync one and the summary of a projection,
//! and what it exports of others.

#[cfg(not(target_arch = "wasm32"))]
use std::mem;

use flick_ir::{
    Exported, Exports, Item, ItemKind, Module, Namespace as IrNamespace, Projection, Reexport,
};

use crate::iface::{Decl, Interface, Namespace};
use crate::types::{Type, Typing};

/// The exports `interface` lists for `module`, whose code tells which of
/// its values are projections.
pub fn exports(module: &Module, interface: &Interface) -> Exports {
    let mut out = Exports::default();
    for export in &interface.exports {
        let entity = &export.entity;
        if entity.module == module.name {
            if entity.namespace == Namespace::Value {
                let sync = export.scheme.as_ref().and_then(sync_stages);
                let name = entity.name.clone();
                let projection = projection(module, &name);
                out.values.push(Exported {
                    name,
                    sync,
                    projection,
                });
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

/// Populates `module.exports.values` and `module.exports.reexports` from
/// `interface` while preserving any `@export "name"` entries already lowered
/// into `module.exports.host`.
#[inline]
pub fn fill_exports(module: &mut Module, interface: &Interface) {
    #[cfg(not(target_arch = "wasm32"))]
    let mut out = exports(module, interface);
    #[cfg(target_arch = "wasm32")]
    let out = exports(module, interface);
    #[cfg(not(target_arch = "wasm32"))]
    {
        out.host = mem::take(&mut module.exports.host);
    }
    module.exports = out;
}

/// Builds the initial `Exports` table for a lowered module, keeping only
/// `@export "name"` entries whose binding exists in `items`.
pub(super) fn host_exports(typing: &Typing, items: &[Item]) -> Exports {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let host = typing
            .host_exports
            .iter()
            .filter(|export| {
                items.iter().any(|item| match &item.kind {
                    ItemKind::Value { name, .. } => name == &export.name,
                    ItemKind::Foreign(f) => f.name == export.name,
                    _ => false,
                })
            })
            .cloned()
            .collect();
        Exports {
            host,
            ..Exports::default()
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (typing, items);
        Exports::default()
    }
}

/// The summary of the value `name` of `module` when it is a projection
/// that takes no dictionaries, which other modules read in place of a call.
fn projection(module: &Module, name: &str) -> Option<Projection> {
    module.items.iter().find_map(|item| match &item.kind {
        ItemKind::Value {
            name: n,
            type_params,
            dicts,
            body,
            ..
        } if n == name && dicts.is_empty() => Projection::of(type_params, body),
        _ => None,
    })
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
            projection: None,
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

    /// An exported projection, the accessor or the explicit `fun` alike,
    /// carries its summary; a function that computes, or a projection the
    /// module keeps to itself, does not.
    #[test]
    fn an_exported_projection_carries_its_summary() {
        let text = "module App exposing (User, city, also, label)\n\n\
                    type alias User = { name : String; address : { city : String } }\n\n\
                    city : User -> String\nlet city = .address.city\n\n\
                    also : User -> String\nlet also = fun u -> u.address.city\n\n\
                    label : User -> String\nlet label u = u.name ++ \"!\"\n\n\
                    hidden : User -> String\nlet hidden = .name\n";
        let exports = lower_text("App", text).expect("lowers").exports;
        let summary = |name: &str| {
            let value = exports.values.iter().find(|e| e.name == name);
            value.expect("exported").projection.clone()
        };
        for name in ["city", "also"] {
            let summary = summary(name).expect("a projection");
            assert_eq!(summary.version, flick_ir::PROJECTION_VERSION);
            assert_eq!(summary.path, ["address", "city"], "{name}");
            assert!(
                summary.param.is_some() && summary.result.is_some(),
                "{summary:?}"
            );
        }
        assert_eq!(summary("label"), None);
        assert!(exports.values.iter().all(|e| e.name != "hidden"));
    }
}
