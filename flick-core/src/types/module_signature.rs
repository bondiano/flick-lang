//! Static module signatures and their dependency-preserving operations.

use super::{
    Form, Kind, ModuleNominalKind, ModuleRequirement as R, ModuleSignature, Type, TypeDef,
    show_type,
};
use crate::api_diff::canonical_type;
use crate::iface::{Decl, Entity, Export, Interface, Namespace};

impl ModuleSignature {
    /// A broken body must not leak inference variables to another checker,
    /// but its declared inputs still constrain applications and cache keys.
    pub(crate) fn inputs_only(&self) -> Self {
        match self {
            Self::Functor { group_size, binder, parameter, result } => Self::Functor {
                group_size: *group_size,
                binder: binder.clone(),
                parameter: parameter.clone(),
                result: Box::new(result.inputs_only()),
            },
            Self::Structure { .. } => Self::Structure { requirements: Vec::new() },
        }
    }
    pub(crate) fn definition(&self) -> TypeDef {
        TypeDef {
            kind: Kind::Module,
            form: match self {
                Self::Structure { requirements } => Form::ModuleContract {
                    requirements: requirements.clone(),
                },
                Self::Functor { .. } => Form::ModuleSignature {
                    signature: Box::new(self.clone()),
                },
            },
        }
    }
    pub fn from_interface(interface: &Interface) -> Self {
        fn local(ty: &Type, module: &str, names: &[String]) -> Type {
            if let Type::Con {
                module: owner,
                name,
            } = ty
                && owner == module
                && names.contains(name)
            {
                return Type::Var(name.clone());
            }
            ty.map_children(&mut |ty| local(ty, module, names))
        }

        let mut requirements = Vec::new();
        for export in &interface.exports {
            match export.entity.namespace {
                Namespace::Value => {
                    if let Some(ty) = &export.scheme {
                        requirements.push(R::Value {
                            name: export.entity.name.clone(),
                            ty: ty.clone(),
                        });
                    }
                }
                Namespace::Type => {
                    if let Some(definition) = &export.definition {
                        match &definition.form {
                            Form::Class { .. } | Form::Service { .. } => {
                                requirements.push(R::Nominal {
                                    name: export.entity.name.clone(),
                                    category: if matches!(definition.form, Form::Class { .. }) {
                                        ModuleNominalKind::Class
                                    } else {
                                        ModuleNominalKind::Service
                                    },
                                    identity: Some(export.entity.clone()),
                                    definition: Some(Box::new(definition.clone())),
                                    methods: Vec::new(),
                                });
                            }
                            Form::ModuleContract { .. } | Form::ModuleSignature { .. } => {}
                            form => requirements.push(R::Type {
                                name: export.entity.name.clone(),
                                params: Vec::new(),
                                manifest: if let Form::Alias { body, .. } = form {
                                    Some(body.clone())
                                } else {
                                    None
                                },
                            }),
                        }
                    }
                }
                Namespace::Constructor => {}
            }
        }
        for (name, child) in &interface.modules {
            requirements.push(R::Module {
                name: name.clone(),
                signature: Box::new(
                    child
                        .functor
                        .clone()
                        .unwrap_or_else(|| Self::from_interface(child)),
                ),
            });
        }
        let names: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let mut signature = Self::Structure { requirements };

        signature.map_types(&mut |ty| local(ty, &interface.module, &names));
        signature
    }
    pub fn display(&self) -> String {
        match self {
            Self::Functor {
                binder,
                parameter,
                result,
                ..
            } => format!(
                "({} : {}) -> {}",
                binder.rsplit('#').next().unwrap_or(binder),
                parameter.display(),
                result.display()
            ),
            Self::Structure { requirements } => requirements
                .iter()
                .map(|r| match r {
                    R::Type { name, manifest, .. } => manifest.as_ref().map_or_else(
                        || format!("type {name}"),
                        |ty| format!("type {name} = {}", show_type(ty)),
                    ),
                    R::Value { name, ty } => format!("{name} : {}", show_type(ty)),
                    R::Module { name, signature } => {
                        format!("module {name} : {}", signature.display())
                    }
                    R::Dictionary { head, from } => {
                        format!("instance {} from {from}", show_type(head))
                    }
                    R::Nominal {
                        name,
                        category,
                        identity,
                        ..
                    } => format!(
                        "{} {name}{}",
                        if matches!(category, ModuleNominalKind::Class) {
                            "class"
                        } else {
                            "service"
                        },
                        identity
                            .as_ref()
                            .map_or(String::new(), |e| format!(" = {}.{}", e.module, e.name))
                    ),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// An abstract interface usable when checking a body independently of its
    /// implementations. Abstract members are stable names in this interface.
    #[allow(clippy::too_many_lines)]
    pub fn interface(&self, path: &str) -> Interface {
        let mut interface = Interface {
            module: path.into(),
            ..Interface::default()
        };
        let Self::Structure { requirements } = self else {
            interface.functor = Some(self.clone());
            return interface;
        };
        let mut subst: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type { name, manifest, .. } => Some((
                    name.clone(),
                    manifest.clone().unwrap_or_else(|| Type::con(path, name)),
                )),
                R::Nominal { name, identity, .. } => Some((
                    name.clone(),
                    identity
                        .as_ref()
                        .map_or_else(|| Type::con(path, name), |e| Type::con(&e.module, &e.name)),
                )),
                _ => None,
            })
            .collect();
        for requirement in requirements {
            if let R::Nominal {
                name,
                category: ModuleNominalKind::Class,
                identity,
                ..
            } = requirement
            {
                let (module, actual_name) = identity.as_ref().map_or((path, name.as_str()), |e| {
                    (e.module.as_str(), e.name.as_str())
                });
                subst.push((
                    format!("{name}$Dict"),
                    Type::con(module, &format!("{actual_name}$Dict")),
                ));
            }
        }
        for _ in 0..subst.len() {
            let next: Vec<_> = subst
                .iter()
                .map(|(name, ty)| (name.clone(), ty.substitute(&subst)))
                .collect();
            if next == subst {
                break;
            }
            subst = next;
        }
        for requirement in requirements {
            let (name, namespace, decl, definition, scheme) = match requirement {
                R::Type {
                    name,
                    params,
                    manifest,
                } => (
                    name,
                    Namespace::Type,
                    Decl::Abstract,
                    Some(TypeDef {
                        kind: Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Type),
                        form: manifest.as_ref().map_or(Form::NOMINAL, |ty| Form::Alias {
                            params: params.iter().map(|p| p.name.clone()).collect(),
                            body: ty.substitute(&subst),
                            opaque: false,
                        }),
                    }),
                    None,
                ),
                R::Value { name, ty } => (
                    name,
                    Namespace::Value,
                    Decl::Let,
                    None,
                    Some(ty.substitute(&subst)),
                ),
                R::Nominal {
                    name,
                    category,
                    identity,
                    definition,
                    methods,
                } => {
                    let entity = identity.clone().unwrap_or(Entity {
                        module: path.into(),
                        namespace: Namespace::Type,
                        name: name.clone(),
                    });
                    for (name, ty) in methods {
                        interface.exports.push(Export {
                            entity: Entity {
                                module: entity.module.clone(),
                                namespace: Namespace::Value,
                                name: name.clone(),
                            },
                            decl: Decl::Method,
                            scheme: Some(ty.substitute(&subst)),
                            definition: None,
                            parent: Some(entity.name.clone()),
                            blocked: None,
                            at: None,
                        });
                    }
                    interface.exports.push(Export {
                        entity,
                        decl: if matches!(category, ModuleNominalKind::Class) {
                            Decl::Class
                        } else {
                            Decl::Service
                        },
                        definition: definition.as_deref().cloned(),
                        scheme: None,
                        parent: None,
                        blocked: None,
                        at: None,
                    });
                    continue;
                }
                R::Module { name, signature } => {
                    interface
                        .modules
                        .insert(name.clone(), signature.interface(&format!("{path}.{name}")));
                    continue;
                }
                R::Dictionary { .. } => continue,
            };
            interface.exports.push(Export {
                entity: Entity {
                    module: path.into(),
                    namespace,
                    name: name.clone(),
                },
                decl,
                definition,
                scheme,
                parent: None,
                blocked: None,
                at: None,
            });
        }
        interface
    }

    pub fn project(&self, actual: &Interface) -> Interface {
        match self {
            Self::Structure { requirements } => super::module_contract::project(
                &TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: requirements.clone(),
                    },
                },
                actual,
            ),
            Self::Functor { .. } => Interface {
                module: actual.module.clone(),
                functor: Some(self.clone()),
                ..Interface::default()
            },
        }
    }

    /// A result annotation is an abstraction boundary. Parameter projection
    /// uses `project` instead: it hides representations while keeping the
    /// argument's existing identities.
    pub fn seal(&self, actual: &Interface, path: &str) -> Interface {
        fn replace(ty: &Type, replacements: &[(Type, Type)]) -> Type {
            if let Some((_, new)) = replacements.iter().find(|(old, _)| old == ty) {
                return new.clone();
            }
            ty.map_children(&mut |ty| replace(ty, replacements))
        }

        let mut projected = self.project(actual);
        let Self::Structure { requirements } = self else {
            return projected;
        };
        let replacements: Vec<_> = requirements
            .iter()
            .filter_map(|r| {
                if let R::Type {
                    name,
                    manifest: None,
                    ..
                } = r
                {
                    let export = projected.find(Namespace::Type, name)?;
                    Some((
                        Type::con(&export.entity.module, &export.entity.name),
                        Type::con(path, name),
                    ))
                } else {
                    None
                }
            })
            .collect();

        for export in &mut projected.exports {
            let old = Type::con(&export.entity.module, &export.entity.name);
            if replacements.iter().any(|(ty, _)| ty == &old) {
                path.clone_into(&mut export.entity.module);
            }
            if let Some(ty) = &mut export.scheme {
                *ty = replace(ty, &replacements);
            }
            if let Some(TypeDef {
                form: Form::Alias { body, .. },
                ..
            }) = &mut export.definition
            {
                *body = replace(body, &replacements);
            }
        }
        for requirement in requirements {
            if let R::Module { name, signature } = requirement
                && let Some(child) = actual.modules.get(name)
            {
                projected.modules.insert(
                    name.clone(),
                    signature.seal(child, &format!("{path}.{name}")),
                );
            }
        }
        path.clone_into(&mut projected.module);
        projected
    }

    pub fn accepts(&self, actual: &Interface) -> bool {
        match self {
            Self::Structure { requirements } => {
                actual.functor.is_none()
                    && super::module_contract::check(
                        &TypeDef {
                            kind: Kind::Module,
                            form: Form::ModuleContract {
                                requirements: requirements.clone(),
                            },
                        },
                        actual,
                    )
                    .is_empty()
            }
            Self::Functor { .. } => actual
                .functor
                .as_ref()
                .is_some_and(|provided| provided.is_subtype_of(self)),
        }
    }

    pub fn is_subtype_of(&self, expected: &Self) -> bool {
        match (self, expected) {
            (
                Self::Functor {
                    group_size: ag,
                    binder: a,
                    parameter: ap,
                    result: ar,
                },
                Self::Functor {
                    group_size: bg,
                    binder: b,
                    parameter: bp,
                    result: br,
                },
            ) => {
                if ag != bg || !bp.is_subtype_of(ap) {
                    return false;
                }
                let mut ar = (**ar).clone();
                let mut br = (**br).clone();
                let matched = format!("$matched({a})({b})");
                ar.rename_parameter(a, &matched);
                br.rename_parameter(b, &matched);
                ar.is_subtype_of(&br)
            }
            (Self::Structure { .. }, Self::Structure { .. }) => {
                expected.accepts(&self.interface("$signature"))
            }
            _ => false,
        }
    }

    pub fn rename_parameter(&mut self, binder: &str, path: &str) {
        fn replace(ty: &Type, from: &str, path: &str) -> Type {
            if let Type::Con { module, name } = ty {
                if module == from {
                    return Type::con(path, name);
                }
                if let Some(suffix) = module.strip_prefix(&format!("{from}.")) {
                    return Type::con(&format!("{path}.{suffix}"), name);
                }
            }
            ty.map_children(&mut |ty| replace(ty, from, path))
        }

        let from = format!("$module:{binder}");

        match self {
            Self::Functor {
                binder: inner,
                parameter,
                result,
                ..
            } => {
                parameter.rename_parameter(binder, path);
                if inner != binder {
                    result.rename_parameter(binder, path);
                }
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.rename_parameter(binder, path);
                    } else {
                        let mut single = Self::Structure {
                            requirements: vec![requirement.clone()],
                        };
                        single.map_types(&mut |ty| replace(ty, &from, path));
                        let Self::Structure { mut requirements } = single else {
                            unreachable!()
                        };
                        *requirement = requirements.remove(0);
                    }
                }
            }
        }
    }

    pub(crate) fn canonicalize(&mut self, next: &mut usize) {
        match self {
            Self::Functor {
                binder,
                parameter,
                result,
                ..
            } => {
                parameter.canonicalize(next);
                let fresh = format!("'{}", *next);
                *next += 1;
                result.rename_parameter(binder, &format!("$module:{fresh}"));
                *binder = fresh;
                result.canonicalize(next);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.canonicalize(next);
                    }
                }
            }
        }
        self.map_types(&mut canonical_type);
    }

    pub fn refine(&mut self, path: &[String], ty: Type) -> Result<(), String> {
        let Self::Structure { requirements } = self else {
            return Err("`with type` requires a structure signature".into());
        };
        let Some((name, rest)) = path.split_first() else {
            return Err("empty type refinement".into());
        };
        for requirement in requirements {
            match requirement {
                R::Type {
                    name: member,
                    params,
                    manifest,
                } if member == name && rest.is_empty() => {
                    if !params.is_empty() {
                        return Err(format!("type `{name}` requires its declared parameters"));
                    }
                    if let Some(previous) = manifest
                        && canonical_type(previous) != canonical_type(&ty)
                    {
                        return Err(format!(
                            "type refinement contradicts the declared equality for `{name}`"
                        ));
                    }
                    *manifest = Some(ty);
                    return Ok(());
                }
                R::Module {
                    name: member,
                    signature,
                } if member == name && !rest.is_empty() => return signature.refine(rest, ty),
                _ => {}
            }
        }
        Err(format!("signature has no type `{}`", path.join(".")))
    }

    /// Visits all types, including types inside nested nominal requirements.
    pub fn map_types(&mut self, f: &mut impl FnMut(&Type) -> Type) {
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.map_types(f);
                result.map_types(f);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    match requirement {
                        R::Type {
                            manifest: Some(ty), ..
                        }
                        | R::Value { ty, .. } => *ty = f(ty),
                        R::Dictionary { head, .. } => *head = f(head),
                        R::Module { signature, .. } => signature.map_types(f),
                        R::Nominal {
                            identity,
                            definition,
                            methods,
                            ..
                        } => {
                            if let Some(entity) = identity {
                                if let Type::Con { module, name } = f(&Type::con(&entity.module, &entity.name)) {
                                    entity.module = module;
                                    entity.name = name;
                                }
                            }
                            for (_, ty) in methods {
                                *ty = f(ty);
                            }
                            if let Some(definition) = definition {
                                match &mut definition.form {
                                    Form::Class {
                                        supers,
                                        default_sigs,
                                        ..
                                    } => {
                                        for ty in supers
                                            .iter_mut()
                                            .chain(default_sigs.iter_mut().map(|(_, ty)| ty))
                                        {
                                            *ty = f(ty);
                                        }
                                    }
                                    Form::Service { operations, .. } => {
                                        for operation in operations {
                                            operation.ty = f(&operation.ty);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        R::Type { manifest: None, .. } => {}
                    }
                }
            }
        }
    }

    pub fn has_functor(&self) -> bool {
        match self {
            Self::Functor { .. } => true,
            Self::Structure { requirements } => requirements
                .iter()
                .any(|r| matches!(r, R::Module { signature, .. } if signature.has_functor())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iface::{read, write};

    fn structure(fields: &[&str]) -> ModuleSignature {
        ModuleSignature::Structure {
            requirements: fields
                .iter()
                .map(|name| R::Value {
                    name: (*name).into(),
                    ty: Type::con("Prelude", "Int"),
                })
                .collect(),
        }
    }

    fn factory(input: ModuleSignature, output: ModuleSignature) -> ModuleSignature {
        ModuleSignature::Functor {
            group_size: 1,
            binder: "K".into(),
            parameter: Box::new(input),
            result: Box::new(output),
        }
    }

    #[test]
    fn factory_argument_is_contravariant_and_result_is_covariant() {
        let expected = factory(structure(&["seed"]), structure(&["answer"]));
        let accepts_less_returns_more = factory(structure(&[]), structure(&["answer", "extra"]));
        let requires_more = factory(structure(&["seed", "extra"]), structure(&["answer"]));
        let returns_less = factory(structure(&["seed"]), structure(&[]));
        assert!(accepts_less_returns_more.is_subtype_of(&expected));
        assert!(!requires_more.is_subtype_of(&expected));
        assert!(!returns_less.is_subtype_of(&expected));
    }

    #[test]
    fn result_annotation_hides_representation_with_a_fresh_identity() {
        let implementation = ModuleSignature::Structure {
            requirements: vec![
                R::Type {
                    name: "Key".into(),
                    params: Vec::new(),
                    manifest: Some(Type::con("Prelude", "String")),
                },
                R::Value {
                    name: "value".into(),
                    ty: Type::Var("Key".into()),
                },
            ],
        }
        .interface("Argument");
        let signature = ModuleSignature::Structure {
            requirements: vec![
                R::Type {
                    name: "Key".into(),
                    params: Vec::new(),
                    manifest: None,
                },
                R::Value {
                    name: "value".into(),
                    ty: Type::Var("Key".into()),
                },
            ],
        };
        let sealed = signature.seal(&implementation, "Chosen");
        assert_eq!(
            sealed
                .find(Namespace::Type, "Key")
                .expect("type")
                .entity
                .module,
            "Chosen"
        );
        assert_eq!(
            sealed
                .find(Namespace::Value, "value")
                .expect("value")
                .scheme,
            Some(Type::con("Chosen", "Key"))
        );
    }

    #[test]
    fn refinements_cannot_invent_members_or_contradict_equalities() {
        let mut signature = ModuleSignature::Structure {
            requirements: vec![R::Type {
                name: "Key".into(),
                params: Vec::new(),
                manifest: None,
            }],
        };
        assert!(
            signature
                .refine(&["Missing".into()], Type::con("Prelude", "Int"))
                .is_err()
        );
        assert!(
            signature
                .refine(&["Key".into()], Type::con("Prelude", "Int"))
                .is_ok()
        );
        assert!(
            signature
                .refine(&["Key".into()], Type::con("Prelude", "String"))
                .is_err()
        );
    }

    #[test]
    fn parameter_renaming_respects_shadowing_inside_nested_modules() {
        let inner = factory(
            structure(&[]),
            ModuleSignature::Structure {
                requirements: vec![R::Type {
                    name: "Key".into(),
                    params: Vec::new(),
                    manifest: Some(Type::con("$module:K", "Key")),
                }],
            },
        );
        let mut outer = ModuleSignature::Structure {
            requirements: vec![R::Module {
                name: "Inner".into(),
                signature: Box::new(inner.clone()),
            }],
        };
        outer.rename_parameter("K", "Actual");
        assert_eq!(
            outer,
            ModuleSignature::Structure {
                requirements: vec![R::Module {
                    name: "Inner".into(),
                    signature: Box::new(inner)
                }]
            }
        );
    }

    #[test]
    fn parameter_renaming_preserves_dependent_nominal_identities() {
        let mut signature = ModuleSignature::Structure {
            requirements: vec![R::Nominal {
                name: "Clock".into(), category: ModuleNominalKind::Service,
                identity: Some(Entity { module: "$module:K.Child".into(), namespace: Namespace::Type, name: "Clock".into() }),
                definition: None, methods: Vec::new(),
            }],
        };
        signature.rename_parameter("K", "Actual");
        let ModuleSignature::Structure { requirements } = signature else { panic!("structure") };
        let R::Nominal { identity: Some(identity), .. } = &requirements[0] else { panic!("nominal") };
        assert_eq!(identity.module, "Actual.Child");
    }

    #[test]
    fn higher_order_signatures_survive_interface_serialization() {
        let signature = factory(
            factory(structure(&["seed"]), structure(&["answer"])),
            structure(&["answer"]),
        );
        let interface = signature.interface("App.Factory");
        let written = write(&interface);
        let read = read(written.as_bytes(), "App.Factory").expect("interface");
        assert_eq!(read.functor, Some(signature));
    }
}
