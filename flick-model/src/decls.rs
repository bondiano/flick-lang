//! What the checker knows of a program besides its terms: the schemes of
//! top-level values, constructors and foreign bindings (Γ₀ and §2.6), the
//! constructors of each type for `Exh`, services, classes and instances.
//! Read from the declarations flick-core keeps with each IR module.

use std::collections::{HashMap, HashSet};

use flick_core::iface::{Interface, Namespace};
use flick_core::types::{Form, default_method};

use crate::syntax::{Name, Path};
use crate::translate::instance;
use crate::translate::types::{binders, constraint, scheme};
use crate::types::{Scheme, Stage, Type};

/// A class: its parameters, superclasses and methods.
#[derive(Clone, Debug, Default)]
pub struct Class {
    pub params: Vec<Name>,
    /// Each superclass applied to the parameters.
    pub supers: Vec<Type>,
    pub methods: Vec<Name>,
}

#[derive(Default)]
pub struct Decls {
    pub values: HashMap<Path, Scheme>,
    /// Constructors by `Module.Name`.
    pub constructors: HashMap<Path, Scheme>,
    /// The constructors of each ADT, with their arity.
    pub variants: HashMap<Path, Vec<(Name, usize)>>,
    /// Types declared with `type error`.
    pub errors: HashSet<Path>,
    /// The operations of each service, with their schemes.
    pub services: HashMap<Path, Vec<(Name, Scheme)>>,
    pub classes: HashMap<Path, Class>,
    /// The dictionary of each instance: a function of its context's.
    pub instances: HashMap<Path, Scheme>,
}

/// The arity of a constructor's scheme: its arrows.
fn arity(s: &Scheme) -> usize {
    let mut n = 0;
    let mut t = &s.body;
    while let Type::Fun(_, _, r) = t {
        n += 1;
        t = r;
    }
    n
}

impl Decls {
    pub fn add(&mut self, interface: &Interface) {
        for export in &interface.exports {
            let path = Path::new(&export.entity.module, &export.entity.name);
            match export.entity.namespace {
                Namespace::Value => {
                    if let Some(t) = &export.scheme {
                        self.values.insert(path, scheme(t));
                    }
                }
                Namespace::Constructor => {
                    if let Some(t) = &export.scheme {
                        self.constructors.insert(path, scheme(t));
                    }
                }
                Namespace::Type => {
                    let Some(def) = &export.definition else {
                        continue;
                    };
                    match &def.form {
                        Form::Adt { constructors, .. } => {
                            let names = constructors
                                .iter()
                                .map(|c| (Name::from(c.as_str()), 0))
                                .collect();
                            self.variants.insert(path, names);
                        }
                        Form::Error => {
                            self.variants
                                .insert(path.clone(), vec![(path.name.clone(), 0)]);
                            self.errors.insert(path);
                        }
                        Form::Service { operations, .. } => {
                            let ops = operations
                                .iter()
                                .map(|o| (Name::from(o.name.as_str()), scheme(&o.ty)))
                                .collect();
                            self.services.insert(path, ops);
                        }
                        Form::Class {
                            params,
                            supers,
                            methods,
                            ..
                        } => {
                            let class = Class {
                                params: params.iter().map(|p| Name::from(p.as_str())).collect(),
                                supers: supers.iter().map(constraint).collect(),
                                methods: methods.iter().map(|m| Name::from(m.as_str())).collect(),
                            };
                            self.classes.insert(path, class);
                        }
                        _ => {}
                    }
                }
            }
        }
        for i in &interface.instances {
            let mut body = constraint(&i.head);
            for c in i.context.iter().rev() {
                body = Type::fun(constraint(c), Stage::Pure, body);
            }
            let s = Scheme {
                vars: binders(&i.vars),
                body,
                outlives: Vec::new(),
            };
            self.instances.insert(instance(&i.module, &i.name), s);
        }
    }

    /// The arities of the constructors, and the scheme of each method's
    /// default, which is the method's own; once every module is added.
    pub fn finish(&mut self) {
        for (c, class) in &self.classes {
            for m in &class.methods {
                if let Some(s) = self.values.get(&Path::new(&c.module, m)).cloned() {
                    let default = Path::new(&c.module, &default_method(&c.name, m));
                    self.values.insert(default, s);
                }
            }
        }
        for (ty, names) in &mut self.variants {
            for (name, n) in names.iter_mut() {
                if let Some(s) = self.constructors.get(&Path {
                    module: ty.module.clone(),
                    name: name.clone(),
                }) {
                    *n = arity(s);
                }
            }
        }
    }

    /// The scheme of the method `name` of class `class` as a field of
    /// `class.Dict`: the class's own constraint taken off, its parameters
    /// left free for the dictionary's arguments.
    pub fn method(&self, class: &Path, name: &str) -> Option<(Vec<Name>, Scheme)> {
        let params = self.classes.get(class)?.params.clone();
        let method = self.values.get(&Path {
            module: class.module.clone(),
            name: name.into(),
        })?;
        let Type::Fun(_, Stage::Pure, rest) = &method.body else {
            return None;
        };
        let own = method
            .vars
            .iter()
            .filter(|(v, _)| !params.contains(v))
            .cloned()
            .collect();
        Some((
            params,
            Scheme {
                vars: own,
                body: (**rest).clone(),
                outlives: Vec::new(),
            },
        ))
    }

    /// The scheme of operation `name` of service `s`.
    pub fn operation(&self, s: &Path, name: &str) -> Option<&Scheme> {
        self.services
            .get(s)?
            .iter()
            .find(|(n, _)| &**n == name)
            .map(|(_, s)| s)
    }
}
