include!("env_impl/static_lexical.rs");

impl<'b> Env<'b> {
    /// The environment of `module` before its own declarations are added.
    pub fn new(
        module: &str,
        syntax: &ModuleSyntax,
        imported: &dyn Fn(&str) -> Option<Interface>,
        builtins: &'b Builtins,
    ) -> Env<'b> {
        let mut env = Env {
            module: module.to_owned(),
            builtins,
            types: TypeDefs::default(),
            values: HashMap::new(),
            parents: HashMap::new(),
            scope: HashMap::new(),
            open: HashMap::new(),
            prelude: module != lang::prelude(),
            ambiguous: HashSet::new(),
            qualifiers: HashMap::new(),
            imported: HashSet::new(),
            interfaces: HashMap::new(),
            open_unknown: false,
            lost_unknown: false,
            instances: Vec::new(),
            invariants: Vec::new(),
            support_errors: Vec::new(),
            support_authorities: HashSet::new(),
            support_roles: HashMap::new(),
            witness_manifest_aliases: HashSet::new(),
        };
        for interface in builtins.interfaces.values() {
            env.record_support_authorities(interface.as_ref());
        }
        for import in &syntax.imports {
            let interface = imported(&import.module);
            env.qualifiers
                .insert(import.qualifier().to_owned(), import.module.clone());
            env.imported.insert(import.qualifier().to_owned());
            if let Some(i) = &interface
                && let Err(error) = env.learn(i)
            {
                env.support_errors.push(error);
            }
            env.interfaces.insert(import.module.clone(), interface);
        }
        // A public value can carry `Module Sig` or `exists K : Sig. T` while
        // the consumer imports only that value's module. Learn the contract's
        // definition for opening it without bringing its names into scope.
        let mut pending: Vec<_> = env
            .interfaces
            .values()
            .flatten()
            .flat_map(iface_support::dependencies)
            .map(|e| e.module)
            .collect();
        let mut visited = HashSet::new();
        while let Some(owner) = pending.pop() {
            if !visited.insert(owner.clone())
                || env.is_builtin(&owner)
                || env.interfaces.contains_key(&owner)
            {
                continue;
            }
            if let Some(interface) = imported(&owner) {
                if let Err(error) = env.learn_type_support(&interface) {
                    env.support_errors.push(error);
                }
                pending.extend(
                    iface_support::dependencies(&interface)
                        .into_iter()
                        .map(|e| e.module),
                );
                // This is semantic authority only: do not add a qualifier,
                // source name, value or instance from the discovered module.
                env.interfaces.insert(owner, Some(interface));
            }
        }
        env.open_imports(syntax);
        let mut ambiguous = HashSet::new();
        for import in &syntax.imports {
            let Some(interface) = &env.interfaces[&import.module] else {
                continue;
            };
            for item in &import.exposing {
                let named = interface
                    .exports
                    .iter()
                    .filter(|e| e.entity.name == item.name);
                let members = interface
                    .members(&item.name)
                    .filter(|_| item.kind == ItemKind::ConAll);
                for export in named.chain(members).collect::<Vec<_>>() {
                    bring(&mut env.scope, &mut ambiguous, export);
                }
            }
        }
        // a name imported by name hides the same name from `exposing (..)`
        env.ambiguous.retain(|key| !env.scope.contains_key(key));
        env.ambiguous.extend(ambiguous);
        env
    }

    /// What the imports bring unqualified beyond their names: all of an
    /// `exposing (..)`, and on trust what a module could not tell.
    fn open_imports(&mut self, syntax: &ModuleSyntax) {
        for import in &syntax.imports {
            let open = import.exposing.iter().any(|i| i.kind == ItemKind::All);
            let interface = self.interfaces[&import.module].as_ref();
            // names from a module that could not be loaded, or that a module
            // which did not parse in full may have lost, are taken on trust
            if interface.is_none_or(|i| i.incomplete) {
                let members = open || import.exposing.iter().any(|i| i.kind == ItemKind::ConAll);
                if interface.is_none() {
                    self.open_unknown |= members;
                } else {
                    self.lost_unknown |= members;
                }
                let lost = |name: &String| {
                    interface.is_none_or(|i| i.exports.iter().all(|e| &e.entity.name != name))
                };
                for item in import.exposing.iter().filter(|item| lost(&item.name)) {
                    for namespace in [Namespace::Value, Namespace::Type, Namespace::Constructor] {
                        self.ambiguous.insert((namespace, item.name.clone()));
                    }
                }
            }
            if let Some(i) = interface
                && open
            {
                for export in &i.exports {
                    bring(&mut self.open, &mut self.ambiguous, export);
                }
            }
        }
    }

    /// Records the types and instances an interface carries.
    fn learn(&mut self, interface: &Interface) -> Result<(), String> {
        self.learn_type_support(interface)?;
        self.learn_public(interface);
        Ok(())
    }

    // The complete semantic tree has already passed validation before any
    // source names, values or instances are installed.
    fn learn_public(&mut self, interface: &Interface) {
        for child in interface.modules.values() {
            self.learn_public(child);
        }
        for instance in &interface.instances {
            let known = |i: &Instance| i.module == instance.module && i.name == instance.name;
            if !self
                .builtins
                .seen
                .contains(&(instance.module.clone(), instance.name.clone()))
                && !self.instances.iter().any(known)
            {
                self.instances.push(instance.clone());
            }
        }
        // an interface that lost declarations gives no evidence
        if !interface.incomplete {
            for invariant in &interface.invariants {
                let known = |i: &crate::iface::Invariant| i.module == invariant.module && i.ty == invariant.ty;
                if !self.invariants.iter().any(known) {
                    self.invariants.push(invariant.clone());
                }
            }
        }
        learn_exports(
            &mut self.types,
            &mut self.values,
            &mut self.parents,
            interface,
        );
    }

    /// Every instance this module sees: the built-in ones, then those of the
    /// modules it depends on, however they are imported, and its own.
    pub fn instances(&self) -> impl Iterator<Item = &Instance> {
        self.builtins.instances.iter().chain(&self.instances)
    }

    /// The values and constructors the module declares, with their types.
    pub fn own_values(&self) -> impl Iterator<Item = (&Entity, &Type)> {
        self.values.iter().filter(|(e, _)| e.module == self.module)
    }

    /// The types, services and classes the module declares.
    pub fn own_types(&self) -> impl Iterator<Item = (&str, &TypeDef)> {
        self.types.of(&self.module).map(|(n, d)| (n.as_str(), d))
    }

    /// The instances beyond the built-in ones, which an interface carries:
    /// `learn` keeps a built-in one out by its key, so a module of the
    /// project named as one of std (`Gen`) keeps its own.
    pub fn own_instances(&self) -> impl Iterator<Item = &Instance> {
        self.instances.iter()
    }

    pub fn add_instance(&mut self, instance: Instance) {
        self.instances.push(instance);
    }

    pub fn entity(&self, namespace: Namespace, name: &str) -> Entity {
        Entity {
            module: self.module.clone(),
            namespace,
            name: name.to_owned(),
        }
    }

    pub fn type_def(&self, module: &str, name: &str) -> Option<&TypeDef> {
        self.types
            .get(module, name)
            .or_else(|| self.builtins.types.get(module, name))
    }

    pub fn define_type(&mut self, name: &str, def: TypeDef) {
        self.types.define(&self.module, name.to_owned(), def);
    }

    #[cfg(feature = "ide")]
    /// The transparent type aliases the module declares, exported or not.
    pub fn local_aliases(&self) -> impl Iterator<Item = (&str, &[String], &Type)> {
        self.types
            .of(&self.module)
            .filter_map(|(name, def)| match &def.form {
                Form::Alias {
                    params,
                    body,
                    opaque: false,
                } => Some((name.as_str(), params.as_slice(), body)),
                _ => None,
            })
    }

    pub fn type_def_mut(&mut self, name: &str) -> Option<&mut TypeDef> {
        self.types.0.get_mut(&self.module)?.get_mut(name)
    }

    /// Makes a local definition visible by its plain name.
    pub fn declare(&mut self, namespace: Namespace, name: &str) {
        let entity = self.entity(namespace, name);
        let key = (namespace, name.to_owned());
        self.ambiguous.remove(&key);
        self.scope.insert(key, entity);
    }

    pub fn set_value(&mut self, entity: Entity, scheme: Type) {
        self.values.insert(entity, scheme);
    }

    pub fn set_parent(&mut self, entity: Entity, parent: &str) {
        self.parents.insert(entity, parent.to_owned());
    }

    pub fn value(&self, entity: &Entity) -> Option<&Type> {
        self.values
            .get(entity)
            .or_else(|| self.builtins.values.get(entity))
    }

    /// The type a constructor builds, by its entity.
    pub fn parent(&self, entity: &Entity) -> Option<(String, String)> {
        let parent = self
            .parents
            .get(entity)
            .or_else(|| self.builtins.parents.get(entity))?;
        Some((entity.module.clone(), parent.clone()))
    }

    pub fn find(&self, namespace: Namespace, name: &str) -> Found {
        let key = (namespace, name.to_owned());
        if self.ambiguous.contains(&key) {
            return Found::Unknown;
        }
        match self.scope.get(&key).or_else(|| self.open.get(&key)) {
            Some(entity) => Found::Entity(entity.clone()),
            None if self.open_unknown => Found::Unknown,
            None => match self.prelude_names().get(&key) {
                Some(entity) => Found::Entity(entity.clone()),
                None if self.lost_unknown => Found::Unknown,
                None => Found::Missing,
            },
        }
    }

    /// The type `path` names: an unqualified one in scope, or `M.T`, `T` as
    /// the module of the import `M` exports it, re-exported or its own.
    pub fn find_type(&self, path: &str) -> Found {
        let Some((qualifier, name)) = path.rsplit_once('.') else {
            return self.find(Namespace::Type, path);
        };
        match self.qualified_module(qualifier) {
            Some((_, Some(interface))) => interface.find(Namespace::Type, name).map_or(
                if interface.incomplete {
                    Found::Unknown
                } else {
                    Found::Missing
                },
                |e| Found::Entity(e.entity.clone()),
            ),
            Some((_, None)) => Found::Unknown,
            None => Found::Missing,
        }
    }

    /// Every unqualified name in `namespace`: candidates for a misspelt one.
    pub fn names(&self, namespace: Namespace) -> impl Iterator<Item = &str> {
        self.scope
            .keys()
            .chain(self.open.keys())
            .chain(self.prelude_names().keys())
            .filter(move |(n, _)| *n == namespace)
            .map(|(_, name)| name.as_str())
    }

    /// The interfaces of the imported and of the built-in modules.
    pub fn known_interfaces(&self) -> impl Iterator<Item = &Interface> {
        (self.interfaces.values().flatten()).chain(self.builtins.interfaces.values().map(AsRef::as_ref))
    }

    /// Whether `module` is one every module knows without importing it.
    pub fn is_builtin(&self, module: &str) -> bool {
        self.builtins.interfaces.contains_key(module)
    }

    /// Every import qualifier and built-in module name.
    pub fn qualifier_names(&self) -> impl Iterator<Item = &str> {
        self.qualifiers
            .keys()
            .chain(self.builtins.interfaces.keys())
            .map(String::as_str)
    }

    fn prelude_names(&self) -> &HashMap<(Namespace, String), Entity> {
        static NONE: LazyLock<HashMap<(Namespace, String), Entity>> = LazyLock::new(HashMap::new);
        if self.prelude {
            &self.builtins.prelude
        } else {
            &NONE
        }
    }

    /// The module a qualifier names, and its interface if it was loaded.
    /// Whether `qualifier` names an import, whose missing names name
    /// resolution reports, rather than a built-in module used without one.
    pub fn is_import(&self, qualifier: &str) -> bool {
        self.imported.contains(qualifier)
    }

    /// The representation of constructor `name` of type `ty` of `module`.
    pub fn constructor_rep(&self, module: &str, ty: &str, name: &str) -> Rep {
        match self.type_def(module, ty).map(|d| &d.form) {
            Some(Form::Adt { constructors, abi }) => constructors
                .iter()
                .position(|c| c == name)
                .map_or(Rep::Tagged, |i| abi.rep(i)),
            _ => Rep::Tagged,
        }
    }

    /// The shortest qualifier that names `module`.
    pub fn qualifier_of(&self, module: &str) -> Option<&str> {
        let builtin = self
            .builtins
            .interfaces
            .get_key_value(module)
            .filter(|(q, _)| !self.qualifiers.contains_key(*q))
            .map(|(q, _)| q.as_str());
        self.qualifiers
            .iter()
            .filter(|(_, m)| *m == module)
            .map(|(q, _)| q.as_str())
            .chain(builtin)
            .min_by_key(|q| (q.len(), *q))
    }

    /// The interface of an imported or built-in module.
    pub fn interface(&self, module: &str) -> Option<&Interface> {
        match self.interfaces.get(module) {
            Some(interface) => interface.as_ref(),
            None => self.builtins.interfaces.get(module).map(AsRef::as_ref),
        }
    }

    pub fn qualified_module(&self, qualifier: &str) -> Option<(&str, Option<&Interface>)> {
        if !self.qualifiers.contains_key(qualifier)
            && let Some((parent, name)) = qualifier.rsplit_once('.')
            && let Some((_, Some(parent))) = self.qualified_module(parent)
            && let Some(child) = parent.modules.get(name)
        {
            return Some((&child.module, Some(child)));
        }
        if self.private_builtin(qualifier) {
            return None;
        }
        let module = match self.qualifiers.get(qualifier) {
            Some(module) => module.as_str(),
            None => self
                .builtins
                .interfaces
                .get_key_value(qualifier)?
                .0
                .as_str(),
        };
        let interface = match self.interfaces.get(module) {
            Some(interface) => interface.as_ref(),
            None => self.builtins.interfaces.get(module).map(AsRef::as_ref),
        };
        Some((interface.map_or(module, |i| i.module.as_str()), interface))
    }

    /// `module` did not parse in full: what it seems to lack may be lost.
    pub fn incomplete(&self, module: &str) -> bool {
        self.interfaces
            .get(module)
            .is_some_and(|i| i.as_ref().is_some_and(|i| i.incomplete))
    }

    pub fn private_builtin(&self, qualifier: &str) -> bool {
        !self.qualifiers.contains_key(qualifier)
            && self
                .builtins
                .interfaces
                .get(qualifier)
                .is_some_and(|i| i.private)
    }
}
