//! Checks whether exported interface entries are closed (contain no
//! solver-local unification variables, skolems, error markers, or contextual
//! row atoms). Mirrors what [`serde_json::to_value`] accepts without pulling
//! JSON serialization for types and interfaces into the browser wasm build.

use super::{Export, Interface};

pub(super) fn open_field(interface: &Interface) -> Option<String> {
    if let Some(export) = interface
        .exports
        .iter()
        .find(|export| !export_is_closed(export))
    {
        return Some(format!("export `{}`", export.entity.name));
    }
    if let Some(instance) = interface
        .instances
        .iter()
        .find(|instance| !instance_is_closed(instance))
    {
        return Some(format!("instance `{}`", instance.name));
    }
    if interface
        .parameterized
        .as_ref()
        .is_some_and(|signature| !signature_is_closed(signature))
    {
        return Some("parameterized signature".into());
    }
    if let Some(support) = interface
        .support_types
        .iter()
        .find(|support| !def_is_closed(&support.definition))
    {
        return Some(format!("support type `{}`", support.entity.name));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(local) = interface
        .local_schemes
        .iter()
        .find(|entry| !type_is_closed(&entry.scheme))
    {
        return Some(format!("local scheme `{}`", local.local));
    }
    interface.modules.iter().find_map(|(name, nested)| {
        open_field(nested).map(|field| format!("module `{name}`, {field}"))
    })
}
use crate::types::{Form, Instance, Kind, ModuleRequirement, ModuleSignature, Row, Type, TypeDef};

pub(crate) fn export_is_closed(export: &Export) -> bool {
    let closed = export.scheme.as_ref().is_none_or(type_is_closed)
        && export.definition.as_ref().is_none_or(def_is_closed);
    #[cfg(test)]
    debug_assert_eq!(
        closed,
        serde_json::to_value(export).is_ok(),
        "export_is_closed diverged from serde_json::to_value"
    );
    closed
}

pub(crate) fn instance_is_closed(instance: &Instance) -> bool {
    let closed = instance.vars.iter().all(|b| kind_is_closed(&b.kind))
        && instance.context.iter().all(type_is_closed)
        && type_is_closed(&instance.head);
    #[cfg(test)]
    debug_assert_eq!(
        closed,
        serde_json::to_value(instance).is_ok(),
        "instance_is_closed diverged from serde_json::to_value"
    );
    closed
}

fn type_is_closed(ty: &Type) -> bool {
    match ty {
        Type::Con { .. } | Type::Var(_) | Type::Symbol(_) => true,
        Type::Meta(_) | Type::Skolem(_) | Type::Error => false,
        Type::App { head, arg } => type_is_closed(head) && type_is_closed(arg),
        Type::Fun {
            param,
            effects,
            result,
        } => {
            type_is_closed(param)
                && effects
                    .as_deref()
                    .is_none_or(|e| type_is_closed(&e.services) && type_is_closed(&e.errors))
                && type_is_closed(result)
        }
        Type::Tuple(items) => items.iter().all(type_is_closed),
        Type::Record(row) | Type::Row(row) => row_is_closed(row),
        Type::Union(left, right) => type_is_closed(left) && type_is_closed(right),
        Type::Without { row, gone } => {
            type_is_closed(row) && gone.iter().all(|elem| !elem.has_meta_view())
        }
        Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
            type_is_closed(ty) && type_is_closed(captures)
        }
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            vars.iter().all(|b| kind_is_closed(&b.kind))
                && constraints.iter().all(type_is_closed)
                && type_is_closed(body)
        }
        Type::Exists { contract, body, .. } => type_is_closed(contract) && type_is_closed(body),
    }
}

fn row_is_closed(row: &Row) -> bool {
    row.entries
        .iter()
        .all(|(elem, ty)| !elem.has_meta_view() && type_is_closed(ty))
        && row.tail.as_deref().is_none_or(type_is_closed)
}

fn kind_is_closed(kind: &Kind) -> bool {
    match kind {
        Kind::Type
        | Kind::Module
        | Kind::ServiceRow
        | Kind::ErrorRow
        | Kind::CaptureRow
        | Kind::Symbol
        | Kind::Region
        | Kind::Constraint
        | Kind::Service
        | Kind::Named { .. } => true,
        Kind::Row(inner) => kind_is_closed(inner),
        Kind::Arrow(left, right) => kind_is_closed(left) && kind_is_closed(right),
        Kind::Meta(_) => false,
    }
}

fn def_is_closed(def: &TypeDef) -> bool {
    kind_is_closed(&def.kind) && form_is_closed(&def.form)
}

fn form_is_closed(form: &Form) -> bool {
    match form {
        Form::Nominal { .. } | Form::Adt { .. } | Form::Error | Form::Kind | Form::KindMember => {
            true
        }
        Form::Alias { body, .. } => type_is_closed(body),
        Form::ModuleContract { requirements } => requirements.iter().all(requirement_is_closed),
        Form::ModuleSignature { signature } => signature_is_closed(signature),
        Form::Service { operations, .. } => operations.iter().all(|op| type_is_closed(&op.ty)),
        Form::Class {
            supers,
            default_sigs,
            ..
        } => {
            supers.iter().all(type_is_closed)
                && default_sigs.iter().all(|(_, ty)| type_is_closed(ty))
        }
    }
}

fn signature_is_closed(signature: &ModuleSignature) -> bool {
    match signature {
        ModuleSignature::Structure { requirements } => {
            requirements.iter().all(requirement_is_closed)
        }
        ModuleSignature::Parameterized {
            parameter, result, ..
        } => signature_is_closed(parameter) && signature_is_closed(result),
    }
}

fn requirement_is_closed(req: &ModuleRequirement) -> bool {
    match req {
        ModuleRequirement::Kind { definition, .. } => def_is_closed(definition),
        ModuleRequirement::Module { signature, .. } => signature_is_closed(signature),
        ModuleRequirement::Type {
            params, manifest, ..
        } => {
            params.iter().all(|b| kind_is_closed(&b.kind))
                && manifest.as_ref().is_none_or(type_is_closed)
        }
        ModuleRequirement::Value { ty, .. } => type_is_closed(ty),
        ModuleRequirement::Nominal {
            definition,
            methods,
            ..
        } => {
            definition.as_deref().is_none_or(def_is_closed)
                && methods.iter().all(|(_, ty)| type_is_closed(ty))
        }
        ModuleRequirement::Dictionary { head, .. } => type_is_closed(head),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use crate::iface::LocalScheme;
    use crate::iface::{self, Decl, Entity, Namespace, SupportType};
    use crate::types::{Atom, Binder, Effects, Elem, Operation, Scope};

    fn base_export() -> Export {
        Export {
            entity: Entity {
                module: "M".into(),
                namespace: Namespace::Value,
                name: "x".into(),
            },
            decl: Decl::Let,
            parent: None,
            scheme: None,
            definition: None,
            blocked: None,
            at: None,
            via: Vec::new(),
        }
    }

    #[test]
    fn publication_rejects_an_open_export_without_panicking() {
        let mut export = base_export();
        export.scheme = Some(Type::Tuple(vec![Type::Error]));
        let interface = Interface {
            module: "M".into(),
            exports: vec![export],
            ..Interface::default()
        };
        rejects(&interface, "export `x`");
    }

    fn rejects(interface: &Interface, field: &str) {
        for result in [iface::write(interface), iface::fingerprint(interface)] {
            let error = result.expect_err("the interface contains a checker-local type");
            assert!(error.contains(field), "{error}");
        }
    }

    #[test]
    fn publication_rejects_open_nested_kinds_signatures_support_types_and_instance_contexts() {
        let mut interface = Interface {
            module: "M".into(),
            ..Interface::default()
        };
        let mut export = base_export();
        export.scheme = Some(Type::Forall {
            vars: vec![Binder {
                name: "a".into(),
                kind: Kind::meta(3),
            }],
            constraints: Vec::new(),
            body: Box::new(Type::Var("a".into())),
        });
        interface.modules.insert(
            "Nested".into(),
            Interface {
                module: "M.Nested".into(),
                exports: vec![export],
                ..Interface::default()
            },
        );
        rejects(&interface, "module `Nested`, export `x`");
        interface.modules.clear();

        let empty = || {
            Box::new(ModuleSignature::Structure {
                requirements: Vec::new(),
            })
        };
        interface.parameterized = Some(ModuleSignature::Parameterized {
            group_size: 1,
            binder: "P".into(),
            parameter: empty(),
            result: Box::new(ModuleSignature::Structure {
                requirements: vec![ModuleRequirement::Value {
                    name: "x".into(),
                    ty: Type::Error,
                }],
            }),
        });
        rejects(&interface, "parameterized signature");
        interface.parameterized = None;

        interface.support_types.push(SupportType {
            entity: base_export().entity,
            definition: TypeDef {
                kind: Kind::meta(3),
                form: Form::NOMINAL,
                variance: Vec::new(),
                native: None,
            },
            owner_path: vec!["Nested".into()],
        });
        rejects(&interface, "support type `x`");
        interface.support_types.clear();

        interface.instances.push(Instance {
            implicit: false,
            module: "M".into(),
            name: "X".into(),
            vars: Vec::new(),
            context: vec![Type::Skolem(1)],
            head: Type::con("M", "X"),
        });
        rejects(&interface, "instance `X`");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fingerprint_still_omits_root_local_schemes() {
        let mut interface = Interface {
            module: "M".into(),
            ..Interface::default()
        };
        let fingerprint = iface::fingerprint(&interface).expect("closed");
        interface.local_schemes.push(LocalScheme {
            item: "f".into(),
            local: "x".into(),
            scheme: Type::Error,
        });
        assert_eq!(iface::fingerprint(&interface), Ok(fingerprint));
        assert!(
            iface::write(&interface)
                .expect_err("open local")
                .contains("local scheme `x`")
        );
    }

    #[test]
    fn export_closedness_matches_serde_json_on_closed_and_open_variants() {
        let closed_types = [
            Type::con("Core", "Int"),
            Type::Var("a".into()),
            Type::Symbol("tag".into()),
            Type::Fun {
                param: Box::new(Type::con("Core", "Int")),
                effects: Some(Box::new(Effects::new(Type::empty_row(), Type::empty_row()))),
                result: Box::new(Type::con("Core", "Bool")),
            },
            Type::Exists {
                scope: Scope::default(),
                binder: "K".into(),
                contract: Box::new(Type::con("M", "Sig")),
                body: Box::new(Type::con("Core", "Int")),
            },
            Type::Record(Row::closed(vec![
                (Elem::named("a"), Type::con("Core", "Int")),
                (Elem::Error(Atom::meta(7)), Type::con("Core", "Unit")),
            ])),
        ];
        for ty in closed_types {
            let mut export = base_export();
            export.scheme = Some(ty);
            assert!(export_is_closed(&export));
            assert!(serde_json::to_value(&export).is_ok());
        }

        let open_types = [
            Type::meta(1),
            Type::Skolem(2),
            Type::Error,
            Type::Forall {
                vars: vec![Binder {
                    name: "a".into(),
                    kind: Kind::meta(3),
                }],
                constraints: Vec::new(),
                body: Box::new(Type::Var("a".into())),
            },
        ];
        for ty in open_types {
            let mut export = base_export();
            export.scheme = Some(ty);
            assert!(!export_is_closed(&export));
            assert!(serde_json::to_value(&export).is_err());
        }

        let mut open_service = base_export();
        open_service.decl = Decl::Service;
        open_service.definition = Some(TypeDef {
            kind: Kind::Service,
            form: Form::Service {
                params: Vec::new(),
                operations: vec![Operation {
                    name: "op".into(),
                    ty: Type::Error,
                    class: flick_ir::OpClass::Tail,
                }],
                effect: false,
            },
            variance: Vec::new(),
            native: None,
        });
        assert!(!export_is_closed(&open_service));
        assert!(serde_json::to_value(&open_service).is_err());
    }

    #[test]
    fn instance_closedness_matches_serde_json() {
        let closed = Instance {
            implicit: false,
            module: "M".into(),
            name: "Show Int".into(),
            vars: vec![Binder {
                name: "a".into(),
                kind: Kind::Type,
            }],
            context: vec![Type::con("Show", "Show")],
            head: Type::con("Show", "Show"),
        };
        assert!(instance_is_closed(&closed));
        assert!(serde_json::to_value(&closed).is_ok());

        let open_var = Instance {
            vars: vec![Binder {
                name: "a".into(),
                kind: Kind::meta(1),
            }],
            ..closed.clone()
        };
        assert!(!instance_is_closed(&open_var));
        assert!(serde_json::to_value(&open_var).is_err());

        let open_ctx = Instance {
            context: vec![Type::Skolem(1)],
            ..closed
        };
        assert!(!instance_is_closed(&open_ctx));
        assert!(serde_json::to_value(&open_ctx).is_err());
    }
}
