//! Static module signatures and their dependency-preserving operations.

use std::mem;

use super::{
    Form, Kind, ModuleNominalKind, ModuleRequirement as R, ModuleSignature, Type, TypeDef,
    show_type,
};
use crate::api_diff::canonical_type;
use crate::iface::{Decl, Entity, Export, Interface, Namespace};

fn display_type(ty: &Type) -> String {
    fn scoped(ty: &Type) -> Type {
        if let Type::Con { module, name } = ty
            && let Some(alias) = module.strip_prefix("$display:")
        {
            return Type::con("", &format!("{alias}.{name}"));
        }
        ty.map_children(&mut scoped)
    }
    show_type(&scoped(ty))
}

/// Module substitution changes service/error identities and their derived
/// set keys together. Record field labels are source names and stay fixed.
fn map_module_children(ty: &Type, f: &mut dyn FnMut(&Type) -> Type) -> Type {
    let mut mapped = ty.map_children(f);
    if let (Type::Row(before), Type::Row(after)) = (ty, &mut mapped) {
        for ((old_key, old), (key, new)) in before.entries.iter().zip(&mut after.entries) {
            if let (
                Type::Con { module, name },
                Type::Con {
                    module: actual,
                    name: member,
                },
            ) = (old.spine().0, new.spine().0)
                && *old_key == format!("{module}.{name}")
            {
                *key = format!("{actual}.{member}");
            }
        }
        after.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    mapped
}

fn map_definition_types(definition: &mut TypeDef, f: &mut dyn FnMut(&Type) -> Type) {
    match &mut definition.form {
        Form::Alias { body, .. } => *body = f(body),
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
        Form::ModuleContract { requirements } => {
            let mut signature = ModuleSignature::Structure {
                requirements: mem::take(requirements),
            };
            signature.map_types(f);
            let ModuleSignature::Structure {
                requirements: mapped,
            } = signature
            else {
                unreachable!()
            };
            *requirements = mapped;
        }
        Form::ModuleSignature { signature } => signature.map_types(f),
        _ => {}
    }
}

fn map_interface_types(interface: &mut Interface, f: &mut dyn FnMut(&Type) -> Type) {
    for export in &mut interface.exports {
        if let Some(ty) = &mut export.scheme {
            *ty = f(ty);
        }
        if let Some(definition) = &mut export.definition {
            map_definition_types(definition, f);
        }
    }
    if let Some(signature) = &mut interface.functor {
        signature.map_types(f);
    }
    for child in interface.modules.values_mut() {
        map_interface_types(child, f);
    }
}

fn map_requirement_types(requirement: &mut R, f: &mut dyn FnMut(&Type) -> Type) {
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
            if let Some(entity) = identity
                && let Type::Con { module, name } = f(&Type::con(&entity.module, &entity.name))
            {
                entity.module = module;
                entity.name = name;
            }
            for (_, ty) in methods {
                *ty = f(ty);
            }
            if let Some(definition) = definition {
                map_definition_types(definition, f);
            }
        }
        R::Type { manifest: None, .. } => {}
    }
}

/// Equality checking may use transparent definitions in a known argument;
/// its abstraction-preserving parameter projection stays unchanged.
pub(crate) fn normalize_module_interface(interface: &mut Interface, witness: &Interface) {
    map_interface_types(interface, &mut |ty| {
        super::module_contract::expand_actual(ty, witness, 32)
    });
}

fn inferred_type_parameters(definition: &TypeDef) -> Vec<super::ty::Binder> {
    let names = match &definition.form {
        Form::Alias { params, .. } => params.as_slice(),
        _ => &[],
    };
    let mut parameters = Vec::new();
    let mut kind = &definition.kind;
    while let Kind::Arrow(parameter, result) = kind {
        let index = parameters.len();
        parameters.push(super::ty::Binder {
            name: names
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("a{index}")),
            kind: (**parameter).clone(),
        });
        kind = result;
    }
    parameters
}

/// Local declarations bind fresh result members. Services and errors use
/// different labels while these identities are represented by bound variables.
fn generalize_local_type(ty: &Type, module: &str, names: &[(String, bool)]) -> Type {
    let binding = |name: &str| {
        names
            .iter()
            .find(|(known, _)| known == name || name.strip_suffix("$Dict") == Some(known))
    };
    if let Type::Con {
        module: owner,
        name,
    } = ty
        && owner == module
        && binding(name).is_some()
    {
        return Type::Var(name.clone());
    }
    let mut mapped = map_module_children(ty, &mut |ty| generalize_local_type(ty, module, names));
    if let (Type::Row(before), Type::Row(after)) = (ty, &mut mapped) {
        for ((old_key, old), (key, new)) in before.entries.iter().zip(&mut after.entries) {
            if let (
                Type::Con {
                    module: owner,
                    name,
                },
                Type::Var(variable),
            ) = (old.spine().0, new.spine().0)
                && owner == module
                && *old_key == format!("{owner}.{name}")
                && let Some((_, service)) = binding(variable)
            {
                *key = if *service {
                    super::ty::service_label(new)
                } else {
                    super::ty::error_label(new)
                }
                .expect("a bound row atom");
            }
        }
        after.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    mapped
}

impl ModuleSignature {
    pub(crate) fn normalize_in(&mut self, witness: &Interface) {
        self.map_types(&mut |ty| super::module_contract::expand_actual(ty, witness, 32));
    }

    /// A broken body must not leak inference variables to another checker,
    /// but its declared inputs still constrain applications and cache keys.
    pub(crate) fn inputs_only(&self) -> Self {
        match self {
            Self::Functor {
                group_size,
                binder,
                parameter,
                result,
            } => Self::Functor {
                group_size: *group_size,
                binder: binder.clone(),
                parameter: parameter.clone(),
                result: Box::new(result.inputs_only()),
            },
            Self::Structure { .. } => Self::Structure {
                requirements: Vec::new(),
            },
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
        if let Some(signature) = &interface.functor {
            return signature.clone();
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
                                    identity: (export.entity.module != interface.module)
                                        .then(|| export.entity.clone()),
                                    definition: Some(Box::new(definition.clone())),
                                    methods: Vec::new(),
                                });
                            }
                            Form::ModuleContract { .. } | Form::ModuleSignature { .. } => {}
                            form => requirements.push(R::Type {
                                name: export.entity.name.clone(),
                                params: inferred_type_parameters(definition),
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
                binder: String::new(),
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
                R::Type { name, .. } => Some((name.clone(), false)),
                R::Nominal {
                    name,
                    identity: None,
                    category,
                    ..
                } => Some((name.clone(), *category == ModuleNominalKind::Service)),
                _ => None,
            })
            .collect();
        let mut signature = Self::Structure { requirements };

        signature.map_types(&mut |ty| generalize_local_type(ty, &interface.module, &names));
        signature
    }
    pub fn display(&self) -> String {
        let mut signature = self.clone();
        signature.display_names(&mut Vec::new());
        signature.display_body()
    }

    fn display_names(&mut self, used: &mut Vec<String>) {
        if let Self::Structure { requirements } = self {
            let members: Vec<_> = requirements
                .iter()
                .filter_map(|r| match r {
                    R::Module { binder, name, .. } if !binder.is_empty() => {
                        Some((binder.clone(), name.clone()))
                    }
                    _ => None,
                })
                .collect();
            for (binder, name) in members {
                self.rename_parameter(&binder, &format!("$display:{name}"));
            }
        }
        match self {
            Self::Functor {
                binder,
                parameter,
                result,
                ..
            } => {
                parameter.display_names(&mut used.clone());
                if !binder.is_empty() {
                    let base = binder.rsplit('#').next().unwrap_or(binder).to_owned();
                    let mut name = base.clone();
                    let mut suffix = 2;
                    while used.contains(&name) {
                        name = format!("{base}{suffix}");
                        suffix += 1;
                    }
                    result.rename_parameter(binder, &format!("$display:{name}"));
                    binder.clone_from(&name);
                    used.push(name);
                }
                result.display_names(used);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.display_names(&mut used.clone());
                    }
                }
            }
        }
    }

    fn display_body(&self) -> String {
        match self {
            Self::Functor { group_size, .. } => {
                let mut parameters = Vec::new();
                let mut tail = self;
                for _ in 0..(*group_size).max(1) {
                    let Self::Functor {
                        binder,
                        parameter,
                        result,
                        ..
                    } = tail
                    else {
                        break;
                    };
                    let input = parameter.display_body();
                    parameters.push(if binder.is_empty() {
                        if matches!(parameter.as_ref(), Self::Functor { .. }) {
                            format!("({input})")
                        } else {
                            input
                        }
                    } else {
                        format!("{binder} : {input}")
                    });
                    tail = result;
                }
                let input = parameters.join(", ");
                let bound = matches!(self, Self::Functor { binder, .. } if !binder.is_empty());
                if parameters.len() > 1 || bound {
                    format!("({input}) -> {}", tail.display_body())
                } else {
                    format!("{input} -> {}", tail.display_body())
                }
            }
            Self::Structure { requirements } if requirements.is_empty() => "{}".into(),
            Self::Structure { requirements } => requirements
                .iter()
                .map(|r| match r {
                    R::Type { name, manifest, .. } => manifest.as_ref().map_or_else(
                        || format!("type {name}"),
                        |ty| format!("type {name} = {}", display_type(ty)),
                    ),
                    R::Value { name, ty } => format!("{name} : {}", display_type(ty)),
                    R::Module {
                        name, signature, ..
                    } => {
                        format!("module {name} : {}", signature.display_body())
                    }
                    R::Dictionary { head, from } => {
                        format!("instance {} from {from}", display_type(head))
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
                        identity.as_ref().map_or(String::new(), |e| format!(
                            " = {}.{}",
                            e.module.strip_prefix("$display:").unwrap_or(&e.module),
                            e.name
                        ))
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
            interface.signature_explicit = true;
            return interface;
        };
        for requirement in requirements {
            if let R::Module {
                name, signature, ..
            } = requirement
            {
                interface
                    .modules
                    .insert(name.clone(), signature.interface(&format!("{path}.{name}")));
            }
        }
        let requirements = Self::instantiate_members(requirements, &interface);
        let mut subst: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type {
                    name,
                    params,
                    manifest,
                } => Some((
                    name.clone(),
                    if params.is_empty() {
                        manifest.clone().unwrap_or_else(|| Type::con(path, name))
                    } else {
                        Type::con(path, name)
                    },
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
        for requirement in &requirements {
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
        for requirement in &requirements {
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
                        definition: definition.as_deref().cloned().map(|mut def| {
                            map_definition_types(&mut def, &mut |ty| ty.substitute(&subst));
                            def
                        }),
                        scheme: None,
                        parent: None,
                        blocked: None,
                        at: None,
                    });
                    continue;
                }
                R::Module {
                    name, signature, ..
                } => {
                    let mut signature = (**signature).clone();
                    signature.substitute_outer_types(&subst);
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
                signature_explicit: true,
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
            map_module_children(ty, &mut |ty| replace(ty, replacements))
        }
        let Self::Structure { requirements } = self else {
            return self.project(actual);
        };
        let mut witness = actual.clone();
        for requirement in requirements {
            if let R::Module {
                name, signature, ..
            } = requirement
                && let Some(child) = actual.modules.get(name)
            {
                witness.modules.insert(
                    name.clone(),
                    signature.seal(child, &format!("{path}.{name}")),
                );
            }
        }
        let mut projected = self.project(&witness);
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
        }
        map_interface_types(&mut projected, &mut |ty| replace(ty, &replacements));
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

    /// Substitute an argument's exported identities, rather than inventing
    /// identities below the name of a module alias.
    pub(crate) fn instantiate_parameter(&mut self, binder: &str, actual: &Interface) {
        fn member(ty: &Type, marker: &str, actual: &Interface) -> Type {
            if let Type::Con { module, name } = ty {
                let suffix = if module == marker {
                    Some("")
                } else {
                    module
                        .strip_prefix(marker)
                        .and_then(|s| s.strip_prefix('.'))
                };
                if let Some(suffix) = suffix {
                    let child = suffix
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .try_fold(actual, |child, name| child.modules.get(name));
                    if let Some(child) = child {
                        if let Some(export) = child.find(Namespace::Type, name) {
                            return Type::con(&export.entity.module, &export.entity.name);
                        }
                        if let Some(class) = name.strip_suffix("$Dict")
                            && let Some(export) = child.find(Namespace::Type, class)
                        {
                            return Type::con(
                                &export.entity.module,
                                &format!("{}$Dict", export.entity.name),
                            );
                        }
                        return Type::con(&child.module, name);
                    }
                }
            }
            map_module_children(ty, &mut |ty| member(ty, marker, actual))
        }
        let marker = format!("$argument({binder})({})", actual.module);
        self.rename_parameter(binder, &marker);
        self.map_types(&mut |ty| member(ty, &marker, actual));
    }

    /// Resolve structure-bound paths through the supplied child identities.
    pub(crate) fn instantiate_members(requirements: &[R], actual: &Interface) -> Vec<R> {
        let mut signature = Self::Structure {
            requirements: requirements.to_vec(),
        };
        for requirement in requirements {
            if let R::Module { name, binder, .. } = requirement
                && !binder.is_empty()
                && let Some(child) = actual.modules.get(name)
            {
                signature.instantiate_parameter(binder, child);
            }
        }
        let Self::Structure { requirements } = signature else {
            unreachable!()
        };
        requirements
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
            map_module_children(ty, &mut |ty| replace(ty, from, path))
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
                        map_requirement_types(requirement, &mut |ty| replace(ty, &from, path));
                    }
                }
            }
        }
    }

    /// A nested signature can mention types bound by its containing
    /// structure. Its own structure members shadow those outer type names;
    /// the input of an arrow does not bind unqualified names in its result.
    pub(crate) fn substitute_outer_types(&mut self, subst: &[(String, Type)]) {
        if subst.is_empty() {
            return;
        }
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.substitute_outer_types(subst);
                result.substitute_outer_types(subst);
            }
            Self::Structure { requirements } => {
                let subst: Vec<_> = subst
                    .iter()
                    .filter(|(name, _)| {
                        let base = name.strip_suffix("$Dict").unwrap_or(name);
                        !requirements.iter().any(|r| {
                            matches!(r,
                        R::Type { name, .. } | R::Nominal { name, .. } if name == base)
                        })
                    })
                    .cloned()
                    .collect();
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.substitute_outer_types(&subst);
                    } else {
                        map_requirement_types(requirement, &mut |ty| ty.substitute(&subst));
                    }
                }
            }
        }
    }

    pub(crate) fn canonicalize(&mut self, next: &mut usize) {
        if let Self::Structure { requirements } = self {
            let binders: Vec<_> = requirements
                .iter()
                .filter_map(|r| match r {
                    R::Module { binder, .. } if !binder.is_empty() => Some(binder.clone()),
                    _ => None,
                })
                .collect();
            for binder in binders {
                let fresh = format!("'{}", *next);
                *next += 1;
                self.rename_parameter(&binder, &format!("$module:{fresh}"));
                if let Self::Structure { requirements } = self {
                    for requirement in requirements {
                        if let R::Module { binder: old, .. } = requirement
                            && old == &binder
                        {
                            old.clone_from(&fresh);
                        }
                    }
                }
            }
        }
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
                    ..
                } if member == name && !rest.is_empty() => return signature.refine(rest, ty),
                _ => {}
            }
        }
        Err(format!("signature has no type `{}`", path.join(".")))
    }

    /// Visits all types, including types inside nested nominal requirements.
    pub fn map_types(&mut self, f: &mut dyn FnMut(&Type) -> Type) {
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.map_types(f);
                result.map_types(f);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    map_requirement_types(requirement, f);
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

    fn dependent_structure(binder: &str) -> ModuleSignature {
        let child = ModuleSignature::Structure {
            requirements: vec![R::Type {
                name: "Item".into(),
                params: Vec::new(),
                manifest: None,
            }],
        };
        let item = Type::con(&format!("$module:{binder}"), "Item");
        ModuleSignature::Structure {
            requirements: vec![
                R::Module {
                    name: "M".into(),
                    binder: binder.into(),
                    signature: Box::new(child),
                },
                R::Value {
                    name: "value".into(),
                    ty: item.clone(),
                },
                R::Type {
                    name: "Alias".into(),
                    params: Vec::new(),
                    manifest: Some(item),
                },
            ],
        }
    }

    #[test]
    fn inferred_rows_distinguish_local_errors_services_and_reexported_identities() {
        for imported in [false, true] {
            let actual = nominal_row_source(if imported { "External" } else { "Factory" });
            let ModuleSignature::Structure { requirements } =
                ModuleSignature::from_interface(&actual)
            else {
                panic!("structure");
            };
            let R::Nominal { identity, .. } = &requirements[1] else {
                panic!("service");
            };
            assert_eq!(
                identity.as_ref().map(|e| e.module.as_str()),
                imported.then_some("External")
            );
            let R::Value {
                ty:
                    Type::Fun {
                        effects: Some(effects),
                        ..
                    },
                ..
            } = &requirements[2]
            else {
                panic!("operation");
            };
            let Type::Row(errors) = &effects.errors else {
                panic!("errors");
            };
            let Type::Row(services) = &effects.services else {
                panic!("services");
            };
            assert_eq!(
                errors.entries,
                vec![("~Failure".into(), Type::Var("Failure".into()))]
            );
            assert_eq!(
                services.entries,
                if imported {
                    vec![("External.Clock".into(), Type::con("External", "Clock"))]
                } else {
                    vec![("@Clock".into(), Type::Var("Clock".into()))]
                }
            );
        }
    }

    fn nominal_row_source(owner: &str) -> Interface {
        use super::super::ty::{Effects, Row};
        let mut actual = ModuleSignature::Structure {
            requirements: vec![
                R::Type {
                    name: "Failure".into(),
                    params: Vec::new(),
                    manifest: None,
                },
                R::Nominal {
                    name: "Clock".into(),
                    category: ModuleNominalKind::Service,
                    identity: None,
                    methods: Vec::new(),
                    definition: Some(Box::new(TypeDef {
                        kind: Kind::Service,
                        form: Form::Service {
                            params: Vec::new(),
                            operations: Vec::new(),
                            effect: false,
                        },
                    })),
                },
            ],
        }
        .interface("Factory");
        actual
            .exports
            .iter_mut()
            .find(|e| e.entity.name == "Clock")
            .expect("service")
            .entity
            .module = owner.into();
        actual.exports.push(Export {
            entity: Entity {
                module: "Factory".into(),
                namespace: Namespace::Value,
                name: "run".into(),
            },
            decl: Decl::Let,
            parent: None,
            definition: None,
            blocked: None,
            at: None,
            scheme: Some(Type::Fun {
                param: Box::new(Type::con("Prelude", "Unit")),
                effects: Some(Box::new(Effects::new(
                    Type::Row(Row::closed(vec![(
                        format!("{owner}.Clock"),
                        Type::con(owner, "Clock"),
                    )])),
                    Type::Row(Row::closed(vec![(
                        "Factory.Failure".into(),
                        Type::con("Factory", "Failure"),
                    )])),
                ))),
                result: Box::new(Type::con("Prelude", "Int")),
            }),
        });
        actual
    }

    #[test]
    fn structure_member_witnesses_survive_projection_and_nested_sealing() {
        let signature = dependent_structure("Contract@1#M");
        let actual = signature.interface("Source");
        let source_item = Type::con("Source.M", "Item");
        assert_eq!(
            actual
                .find(Namespace::Value, "value")
                .expect("witness value")
                .scheme,
            Some(source_item.clone())
        );
        assert!(signature.accepts(&actual));
        assert_eq!(
            signature
                .project(&actual)
                .find(Namespace::Value, "value")
                .expect("projected value")
                .scheme,
            Some(source_item)
        );
        for name in ["First", "Second"] {
            let sealed = signature.seal(&actual, name);
            let expected = Type::con(&format!("{name}.M"), "Item");
            assert_eq!(
                sealed
                    .find(Namespace::Value, "value")
                    .expect("sealed value")
                    .scheme,
                Some(expected.clone())
            );
            assert!(
                matches!(&sealed.find(Namespace::Type, "Alias").expect("sealed alias").definition.as_ref().expect("alias definition").form,
                Form::Alias { body, .. } if body == &expected)
            );
            assert!(signature.accepts(&sealed));
        }
    }

    #[test]
    fn structure_member_binders_are_lexical_and_canonical() {
        let mut first = dependent_structure("First@12#M");
        let mut second = dependent_structure("Second@300#M");
        assert!(first.is_subtype_of(&second));
        assert!(second.is_subtype_of(&first));
        assert!(first.display().contains("value : M.Item"));
        assert!(!first.display().contains("$module:"));
        first.canonicalize(&mut 0);
        second.canonicalize(&mut 0);
        assert_eq!(first, second);
    }

    #[test]
    fn nested_signatures_capture_outer_types_and_respect_inner_type_binders() {
        let item = ModuleSignature::Structure {
            requirements: vec![
                R::Type {
                    name: "Item".into(),
                    params: Vec::new(),
                    manifest: None,
                },
                R::Value {
                    name: "value".into(),
                    ty: Type::Var("Item".into()),
                },
            ],
        };
        let output = ModuleSignature::Structure {
            requirements: vec![R::Type {
                name: "Value".into(),
                params: Vec::new(),
                manifest: Some(Type::Var("Item".into())),
            }],
        };
        let signature = ModuleSignature::Structure {
            requirements: vec![
                R::Type {
                    name: "Item".into(),
                    params: Vec::new(),
                    manifest: None,
                },
                R::Module {
                    name: "Inner".into(),
                    binder: String::new(),
                    signature: Box::new(item.clone()),
                },
                R::Module {
                    name: "Build".into(),
                    binder: String::new(),
                    signature: Box::new(factory(item, output)),
                },
            ],
        };
        let actual = signature.interface("Source");
        assert!(signature.accepts(&actual));
        let projected = signature.project(&actual);
        let sealed = signature.seal(&actual, "Result");
        for (interface, root) in [(&projected, "Source"), (&sealed, "Result")] {
            assert_eq!(
                interface.modules["Inner"]
                    .find(Namespace::Value, "value")
                    .expect("inner value")
                    .scheme,
                Some(Type::con(&format!("{root}.Inner"), "Item"))
            );
            let ModuleSignature::Functor {
                parameter, result, ..
            } = interface.modules["Build"]
                .functor
                .as_ref()
                .expect("factory")
            else {
                panic!("factory arrow");
            };
            assert_eq!(
                parameter
                    .interface("Input")
                    .find(Namespace::Value, "value")
                    .expect("input value")
                    .scheme,
                Some(Type::con("Input", "Item"))
            );
            assert!(
                matches!(&result.interface("Output").find(Namespace::Type, "Value").expect("output type").definition.as_ref().expect("manifest definition").form,
                Form::Alias { body, .. } if body == &Type::con(root, "Item"))
            );
        }
    }

    #[test]
    fn signature_display_distinguishes_atomic_and_curried_groups() {
        let curried = factory(
            structure(&["seed"]),
            factory(structure(&["seed"]), structure(&["answer"])),
        );
        let mut grouped = curried.clone();
        let ModuleSignature::Functor { group_size, .. } = &mut grouped else {
            panic!("functor")
        };
        *group_size = 2;
        assert_ne!(curried.display(), grouped.display());
        assert!(grouped.display().contains(", "));
    }

    #[test]
    fn signature_display_uses_scoped_names_for_dependent_types() {
        let signature = ModuleSignature::Functor {
            group_size: 1,
            binder: "Lib@12#K".into(),
            parameter: Box::new(structure(&[])),
            result: Box::new(ModuleSignature::Structure {
                requirements: vec![R::Type {
                    name: "Value".into(),
                    params: Vec::new(),
                    manifest: Some(Type::con("$module:Lib@12#K", "Key")),
                }],
            }),
        };
        assert_eq!(signature.display(), "(K : {}) -> type Value = K.Key");
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
                binder: String::new(),
                signature: Box::new(inner.clone()),
            }],
        };
        outer.rename_parameter("K", "Actual");
        assert_eq!(
            outer,
            ModuleSignature::Structure {
                requirements: vec![R::Module {
                    name: "Inner".into(),
                    binder: String::new(),
                    signature: Box::new(inner)
                }]
            }
        );
    }

    #[test]
    fn parameter_renaming_preserves_dependent_nominal_identities() {
        let mut signature = ModuleSignature::Structure {
            requirements: vec![R::Nominal {
                name: "Clock".into(),
                category: ModuleNominalKind::Service,
                identity: Some(Entity {
                    module: "$module:K.Child".into(),
                    namespace: Namespace::Type,
                    name: "Clock".into(),
                }),
                definition: None,
                methods: Vec::new(),
            }],
        };
        signature.rename_parameter("K", "Actual");
        let ModuleSignature::Structure { requirements } = signature else {
            panic!("structure")
        };
        let R::Nominal {
            identity: Some(identity),
            ..
        } = &requirements[0]
        else {
            panic!("nominal")
        };
        assert_eq!(identity.module, "Actual.Child");
    }

    #[test]
    fn parameter_renaming_updates_set_keys_and_preserves_record_fields() {
        use super::super::ty::Row;
        let service = Type::con("$module:K", "Clock");
        let mut signature = ModuleSignature::Structure {
            requirements: vec![R::Value {
                name: "value".into(),
                ty: Type::Tuple(vec![
                    Type::Row(Row::closed(vec![(
                        "$module:K.Clock".into(),
                        service.clone(),
                    )])),
                    Type::Record(Row::closed(vec![("clock".into(), service)])),
                ]),
            }],
        };
        signature.rename_parameter("K", "Actual");
        let ModuleSignature::Structure { requirements } = signature else {
            panic!("structure")
        };
        let R::Value {
            ty: Type::Tuple(parts),
            ..
        } = &requirements[0]
        else {
            panic!("value")
        };
        assert_eq!(
            parts[0],
            Type::Row(Row::closed(vec![(
                "Actual.Clock".into(),
                Type::con("Actual", "Clock")
            )]))
        );
        assert_eq!(
            parts[1],
            Type::Record(Row::closed(vec![(
                "clock".into(),
                Type::con("Actual", "Clock")
            )]))
        );
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
