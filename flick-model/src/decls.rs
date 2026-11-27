//! What the checker knows of a program besides its terms: the schemes of
//! top-level values, constructors and foreign bindings (Γ₀ and §2.6), the
//! constructors of each type for `Exh`, services, classes and instances.
//! Read from the declarations flick-core keeps with each IR module.

use std::collections::{HashMap, HashSet};

use flick_core::iface::{Decl, Interface, Namespace};
use flick_core::types::{self as fc, Form, default_method};

use crate::syntax::{Name, Path};
use crate::translate::instance;
use crate::translate::types::{Types, binders, kind};
use crate::types::{Kind, Scheme, Stage, Type};
use crate::variance::{self, Sign};

/// A class: its parameters, superclasses and methods.
#[derive(Clone, Debug, Default)]
pub struct Class {
    pub params: Vec<Name>,
    /// Each superclass applied to the parameters.
    pub supers: Vec<Type>,
    pub methods: Vec<Name>,
}

impl Class {
    /// The field of superclass `i`: its class's name, numbered by its
    /// position when another superclass has the same name, as flick-core's
    /// `super_slot` names it (`docs/formal/elaboration.md` §2.1).
    pub fn super_slot(&self, i: usize) -> Option<String> {
        let name = |t: &Type| match t {
            Type::Dict(p, _) => Some(p.name.clone()),
            _ => None,
        };
        let own = name(self.supers.get(i)?)?;
        let same = self
            .supers
            .iter()
            .filter(|s| name(s).as_ref() == Some(&own))
            .count();
        Some(if same > 1 {
            format!("{own}{i}")
        } else {
            own.to_string()
        })
    }
}

/// A `type module Sig = ...` contract, as needed to check `Module Sig`
/// and `exists K : Sig. T` against their lowered records.
#[derive(Clone, Debug)]
pub struct ModuleContract {
    /// Abstract type members (`type Key`, `type Box a`) in declaration order.
    pub abstract_types: Vec<(Name, Kind)>,
    /// Manifest type aliases (`type Item = Int`, `type Box a = List a`),
    /// with any abstract members of this contract referencing `CONTRACT_SCOPE`.
    pub manifest_types: HashMap<Name, (Vec<Name>, Type)>,
    /// Manifest class/service identities (`class C = Path.C`, `service S = Path.S`).
    pub nominals: HashMap<Name, Path>,
    /// Value members of the packed record, in declaration order.
    pub fields: Vec<(Name, Scheme)>,
    /// Whether this signature may be packed into a runtime `Module Sig`.
    /// Abstract classes/services and nested module members are static-only.
    pub packable: bool,
}

#[derive(Clone, Default)]
pub struct Decls {
    pub values: HashMap<Path, Scheme>,
    /// Compiler-only raw foreign aliases. These have source schemes but do
    /// not add globals to the source calculus or its certificate signature.
    pub(crate) raw_aliases: HashMap<Path, Path>,
    /// Constructors by `Module.Name`.
    pub constructors: HashMap<Path, Scheme>,
    /// The constructors of each ADT, with their arity.
    pub variants: HashMap<Path, Vec<(Name, usize)>>,
    /// Types declared with `type error`.
    pub errors: HashSet<Path>,
    /// The operations of each service, with their schemes.
    pub services: HashMap<Path, Vec<(Name, Scheme)>>,
    /// The number of parameters of each service and effect: the first
    /// variables of the schemes of its operations.
    pub service_params: HashMap<Path, usize>,
    pub classes: HashMap<Path, Class>,
    /// The dictionary of each instance: a function of its context's.
    pub instances: HashMap<Path, Scheme>,
    /// `type module Sig = ...` contracts by `Module.Sig`.
    pub module_contracts: HashMap<Path, ModuleContract>,
    /// Each alias, with its parameters and representation: what an opaque
    /// one is inside its module, which schemes still name.
    pub opaque: HashMap<Path, (Vec<Name>, Type)>,
    /// The kinds of the parameters of each type constructor.
    pub params: HashMap<Path, Vec<Kind>>,
    /// The values a `foreign` declaration gives.
    pub foreign: HashSet<Path>,
    /// The types without constructors with hidden storage: an opaque ADT
    /// whose fields hide captures, or a foreign type marked `@stores`.
    pub hidden: HashSet<Path>,
    /// The sign of each parameter of a declared type, once every module
    /// is added; a type without one is invariant in each.
    pub signs: HashMap<Path, Vec<Sign>>,
    /// The sign of each parameter of a declared class, inferred from its
    /// superclasses and methods (`regions.md` §2.2.1, Lean `Sig.dictVar`).
    pub class_signs: HashMap<Path, Vec<Sign>>,
    /// Local `let` / `let rec` schemes keyed by `(Module, item_key, unique_local)`.
    pub local_schemes: HashMap<(Name, String, Name), Scheme>,
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
    pub fn add_kinds(&mut self, interface: &Interface) {
        let mut record = |module: &str, name: &str, def: &fc::TypeDef| {
            let path = Path::new(module, name);
            let mut params = Vec::new();
            let mut k = &def.kind;
            while let fc::Kind::Arrow(param, result) = k {
                params.push(kind(param).unwrap_or(Kind::Type));
                k = result;
            }
            if !params.is_empty() {
                self.params.insert(path, params);
            }
        };
        for export in &interface.exports {
            if export.entity.namespace == Namespace::Type
                && let Some(def) = &export.definition
            {
                record(&export.entity.module, &export.entity.name, def);
            }
        }
        for support in &interface.support_types {
            record(
                &support.entity.module,
                &support.entity.name,
                &support.definition,
            );
        }
    }

    /// Compiler-private raw names share the source value's one scheme.
    /// Keeping aliases rather than copies also preserves transformations of
    /// the declaration inventory, such as qualifier erasure in model checks.
    pub(crate) fn value(&self, path: &Path) -> Option<&Scheme> {
        self.values.get(path).or_else(|| {
            self.raw_aliases
                .get(path)
                .and_then(|source| self.values.get(source))
        })
    }
    pub fn add(&mut self, interface: &Interface) {
        self.add_kinds(interface);
        let tr = Types::new(&self.params);
        for export in &interface.exports {
            let path = Path::new(&export.entity.module, &export.entity.name);
            match export.entity.namespace {
                Namespace::Value => {
                    if export.decl == Decl::Foreign {
                        self.foreign.insert(path.clone());
                    }
                    if let Some(t) = &export.scheme {
                        self.values.insert(path, tr.scheme(t));
                    }
                }
                Namespace::Constructor => {
                    if let Some(t) = &export.scheme {
                        self.constructors.insert(path, tr.scheme(t));
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
                        // the module's own view opens an opaque alias; a
                        // transparent one is expanded in every scheme
                        Form::Alias { params, body, .. } => {
                            let vars = params.iter().map(|p| Name::from(p.as_str())).collect();
                            self.opaque.insert(path, (vars, tr.ty(body)));
                        }
                        Form::Nominal { hidden: true, .. } => {
                            self.hidden.insert(path);
                        }
                        Form::Error => {
                            self.variants
                                .insert(path.clone(), vec![(path.name.clone(), 0)]);
                            self.errors.insert(path);
                        }
                        Form::Service {
                            operations, params, ..
                        } => {
                            let ops = operations
                                .iter()
                                .map(|o| (Name::from(o.name.as_str()), tr.scheme(&o.ty)))
                                .collect();
                            self.service_params.insert(path.clone(), params.len());
                            self.services.insert(path, ops);
                        }
                        Form::Class {
                            params,
                            supers,
                            methods,
                            default_sigs,
                            ..
                        } => {
                            for (m, t) in default_sigs {
                                let default =
                                    Path::new(&path.module, &default_method(&path.name, m));
                                self.values.insert(default, tr.scheme(t));
                            }
                            let class = Class {
                                params: params.iter().map(|p| Name::from(p.as_str())).collect(),
                                supers: supers.iter().map(|c| tr.constraint(c)).collect(),
                                methods: methods.iter().map(|m| Name::from(m.as_str())).collect(),
                            };
                            self.classes.insert(path, class);
                        }
                        Form::ModuleContract { requirements } => {
                            self.module_contracts
                                .insert(path, tr.module_contract(requirements));
                        }
                        _ => {}
                    }
                }
            }
        }
        self.add_support_instances_and_locals(interface);
    }

    fn add_support_instances_and_locals(&mut self, interface: &Interface) {
        let tr = Types::new(&self.params);
        for support in &interface.support_types {
            if let Form::ModuleContract { requirements } = &support.definition.form {
                let path = Path::new(&support.entity.module, &support.entity.name);
                self.module_contracts
                    .entry(path)
                    .or_insert_with(|| tr.module_contract(requirements));
            }
        }
        for i in &interface.instances {
            let mut body = tr.constraint(&i.head);
            for c in i.context.iter().rev() {
                body = Type::fun(tr.constraint(c), Stage::Pure, body);
            }
            let s = Scheme {
                vars: binders(&i.vars),
                body,
                outlives: Vec::new(),
            };
            self.instances.insert(instance(&i.module, &i.name), s);
        }
        for local in &interface.local_schemes {
            self.local_schemes.insert(
                (
                    Name::from(interface.module.as_str()),
                    local.item.clone(),
                    Name::from(local.local.as_str()),
                ),
                tr.scheme(&local.scheme),
            );
        }
    }

    /// The arities of the constructors, and the scheme of each method's
    /// default without a signature of its own, which is the method's; once
    /// every module is added.
    pub fn finish(&mut self) {
        for (c, class) in &self.classes {
            for m in &class.methods {
                if let Some(s) = self.values.get(&Path::new(&c.module, m)).cloned() {
                    let default = Path::new(&c.module, &default_method(&c.name, m));
                    self.values.entry(default).or_insert(s);
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
        self.signs = variance::signs(self);
        self.class_signs = variance::class_signs(self);
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
