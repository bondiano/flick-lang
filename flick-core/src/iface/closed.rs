//! Checks whether exported interface entries are closed (contain no
//! solver-local unification variables, skolems, error markers, or contextual
//! row atoms). Mirrors what [`serde_json::to_value`] accepts without pulling
//! JSON serialization for types and interfaces into the browser wasm build.

use super::Export;
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
    use crate::iface::{Decl, Entity, Namespace};
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
