/// The reference model checks the same flattened runtime names as the IR.
/// This view is not published as the source module's `.iface`.
fn relocate_declarations(
    interface: &Interface,
    child: &str,
    parent: &str,
    local: &str,
) -> Interface {
    let to = Relocation { child, parent, local };
    let mut exports = interface.exports.clone();
    let mut instances: Vec<_> = (interface.instances.iter())
        .filter(|i| i.module == child)
        .cloned()
        .collect();
    for export in &mut exports {
        to.export(export);
    }
    for instance in &mut instances {
        to.declared_instance(instance);
    }
    #[cfg(test)]
    assert_eq!(
        (exports.clone(), instances.clone()),
        declarations_as_json(interface, &to),
        "the walk moves what the JSON form of the declarations moved"
    );
    let mut declarations = Interface { exports, instances, ..Interface::default() };
    // Model method lookup uses the method's field name, while the runtime
    // value in the merged file has a prefixed symbol.
    let method_names: HashSet<String> = interface
        .exports
        .iter()
        .filter_map(|e| e.definition.as_ref())
        .filter_map(|d| match &d.form {
            types::Form::Class { methods, .. } => Some(methods),
            _ => None,
        })
        .flat_map(|methods| methods.iter().cloned())
        .collect();
    let methods: Vec<_> = declarations
        .exports
        .iter()
        .filter(|e| e.entity.namespace == Namespace::Value)
        .cloned()
        .filter_map(|mut e| {
            let prefix = format!("{local}$");
            if !e.entity.name.starts_with(&prefix) {
                return None;
            }
            e.entity.name.replace_range(..prefix.len(), "");
            method_names.contains(&e.entity.name).then_some(e)
        })
        .collect();
    declarations.exports.extend(methods);
    declarations
}

/// The declarations' half of [`Relocation`]: every name of the child in an
/// export or instance, its types, definitions and module signatures.
impl Relocation<'_> {
    fn export(&self, export: &mut resolve::Export) {
        self.name(&mut export.entity.module, &mut export.entity.name);
        if let Some(parent) = &mut export.parent {
            *parent = format!("{}${parent}", self.local);
        }
        if let Some(scheme) = &mut export.scheme {
            self.ty(scheme);
        }
        if let Some(definition) = &mut export.definition {
            self.definition(definition);
        }
    }

    fn declared_instance(&self, instance: &mut types::Instance) {
        self.name(&mut instance.module, &mut instance.name);
        self.binders(&mut instance.vars);
        for ty in &mut instance.context {
            self.ty(ty);
        }
        self.ty(&mut instance.head);
    }

    fn ty(&self, ty: &mut types::Type) {
        use types::Type as T;
        match ty {
            T::Con { module, name } => {
                self.name(module, name);
            }
            T::App { head, arg } => {
                self.ty(head);
                self.ty(arg);
            }
            T::Fun {
                param,
                effects,
                result,
            } => {
                self.ty(param);
                if let Some(effects) = effects {
                    self.ty(&mut effects.services);
                    self.ty(&mut effects.errors);
                }
                self.ty(result);
            }
            T::Tuple(items) => {
                for item in items {
                    self.ty(item);
                }
            }
            T::Record(row) | T::Row(row) => self.row(row),
            T::Union(a, b)
            | T::Captured {
                ty: a, captures: b, ..
            }
            | T::Bounded { ty: a, captures: b }
            | T::Exists {
                contract: a,
                body: b,
                ..
            } => {
                self.ty(a);
                self.ty(b);
            }
            T::Without { row, .. } => self.ty(row),
            T::Forall {
                vars,
                constraints,
                body,
            } => {
                self.binders(vars);
                for constraint in constraints {
                    self.ty(constraint);
                }
                self.ty(body);
            }
            T::Var(_) | T::Symbol(_) | T::Meta(_) | T::Skolem(_) | T::Error => {}
        }
    }

    /// A service or error row names its elements `Child.Member`.
    fn row(&self, row: &mut types::Row) {
        for (elem, ty) in &mut row.entries {
            if let types::Elem::Named(label) = elem {
                self.key(label);
            }
            self.ty(ty);
        }
        if let Some(tail) = &mut row.tail {
            self.ty(tail);
        }
    }

    fn kind(&self, kind: &mut types::Kind) {
        use types::Kind as K;
        match kind {
            K::Named { module, name } => {
                self.name(module, name);
            }
            K::Row(inner) => self.kind(inner),
            K::Arrow(param, result) => {
                self.kind(param);
                self.kind(result);
            }
            K::Type
            | K::Module
            | K::ServiceRow
            | K::ErrorRow
            | K::CaptureRow
            | K::Symbol
            | K::Region
            | K::Constraint
            | K::Service
            | K::Meta(_) => {}
        }
    }

    fn binders(&self, binders: &mut [types::Binder]) {
        for binder in binders {
            self.kind(&mut binder.kind);
        }
    }

    fn entity(&self, entity: &mut resolve::Entity) {
        self.name(&mut entity.module, &mut entity.name);
    }

    fn definition(&self, definition: &mut types::TypeDef) {
        use types::Form as F;
        self.kind(&mut definition.kind);
        match &mut definition.form {
            F::Adt { constructors, .. } => {
                for constructor in constructors {
                    *constructor = format!("{}${constructor}", self.local);
                }
            }
            F::Alias { body, .. } => self.ty(body),
            F::ModuleContract { requirements } => self.requirements(requirements),
            F::ModuleSignature { signature } => self.signature(signature),
            F::Service { operations, .. } => {
                for operation in operations {
                    self.ty(&mut operation.ty);
                }
            }
            F::Class {
                supers,
                default_sigs,
                ..
            } => {
                for ty in supers.iter_mut().chain(default_sigs.iter_mut().map(|(_, t)| t)) {
                    self.ty(ty);
                }
            }
            F::Nominal { .. } | F::Error | F::Kind | F::KindMember => {}
        }
    }

    fn signature(&self, signature: &mut types::ModuleSignature) {
        match signature {
            types::ModuleSignature::Structure { requirements } => self.requirements(requirements),
            types::ModuleSignature::Parameterized {
                parameter, result, ..
            } => {
                self.signature(parameter);
                self.signature(result);
            }
        }
    }

    fn requirements(&self, requirements: &mut [types::ModuleRequirement]) {
        use types::ModuleRequirement as R;
        for requirement in requirements {
            match requirement {
                R::Kind {
                    identity,
                    definition,
                    ..
                } => {
                    if let Some(identity) = identity {
                        self.entity(identity);
                    }
                    self.definition(definition);
                }
                R::Module { signature, .. } => self.signature(signature),
                R::Type {
                    params, manifest, ..
                } => {
                    self.binders(params);
                    if let Some(manifest) = manifest {
                        self.ty(manifest);
                    }
                }
                R::Value { ty, .. } | R::Dictionary { head: ty, .. } => self.ty(ty),
                R::Nominal {
                    identity,
                    definition,
                    methods,
                    ..
                } => {
                    if let Some(identity) = identity {
                        self.entity(identity);
                    }
                    if let Some(definition) = definition {
                        self.definition(definition);
                    }
                    for (_, ty) in methods {
                        self.ty(ty);
                    }
                }
            }
        }
    }
}

/// [`relocate_declarations`] through the JSON form of the declarations, the
/// way it was done before its walk: whatever object names the child as its
/// module, with the fields `parent`, `constructors` and `entries`.
#[cfg(test)]
fn declarations_as_json(
    interface: &Interface,
    to: &Relocation<'_>,
) -> (Vec<resolve::Export>, Vec<types::Instance>) {
    let mut own = interface.clone();
    own.instances.retain(|i| i.module == to.child);
    let mut value = serde_json::to_value((&own.exports, &own.instances))
        .expect("declarations serialize");
    relocate_decl_value(&mut value, to.child, to.parent, to.local);
    serde_json::from_value(value).expect("declarations keep their shape")
}

#[cfg(test)]
fn relocate_decl_value(value: &mut serde_json::Value, child: &str, parent: &str, local: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            relocate_name(fields, child, parent, local, false);
            if let Some(serde_json::Value::String(name)) = fields.get_mut("parent") {
                *name = format!("{local}${name}");
            }
            if let Some(serde_json::Value::Array(constructors)) = fields.get_mut("constructors") {
                for constructor in constructors {
                    if let Some(name) = constructor.as_str() {
                        *constructor = format!("{local}${name}").into();
                    }
                }
            }
            if let Some(serde_json::Value::Array(entries)) = fields.get_mut("entries") {
                for entry in entries {
                    if let serde_json::Value::Array(pair) = entry
                        && let Some(serde_json::Value::String(label)) = pair.first_mut()
                        && let Some(member) =
                            label.strip_prefix(child).and_then(|s| s.strip_prefix('.'))
                    {
                        *label = format!("{parent}.{local}${member}");
                    }
                }
            }
            for field in fields.values_mut() {
                relocate_decl_value(field, child, parent, local);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                relocate_decl_value(item, child, parent, local);
            }
        }
        _ => {}
    }
}
