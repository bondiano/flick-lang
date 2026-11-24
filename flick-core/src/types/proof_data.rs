//! The types of a proof as the logic reads them, and the data types it may
//! split: each with its constructors, and whether it names itself.

use super::Checker;
use super::env::Form;
use super::lang::Lang;
use super::ty::Type;
use super::typing::Ref;
use crate::iface::{Entity, Namespace};
use crate::logic::{self, DataDecl, Ty};
use crate::syntax::cst::Node;

impl Checker<'_> {
    /// `t`, solved, as a type of the logic; the data types it names go to
    /// `data`.
    pub(super) fn logic_type(&mut self, t: &Type, data: &mut logic::Data) -> Ty {
        let t = self.zonk(t);
        self.logic_of(&t, data)
    }

    /// `t`, solved, as a type of `PureTotalV2` contracts: refuses `Float` and
    /// non-ADT nominal types other than `Int`, `Bool`, and `Unit`.
    #[cfg(feature = "contracts")]
    pub(super) fn contract_logic_type(&mut self, t: &Type, data: &mut logic::Data) -> Ty {
        let t = self.zonk(t);
        self.contract_logic_of(&t, data)
    }

    fn logic_of(&mut self, t: &Type, data: &mut logic::Data) -> Ty {
        match t {
            Type::Skolem(s) => Ty::Rigid(self.store.skolem_name(*s).to_owned()),
            Type::Bounded { ty, .. } | Type::Captured { ty, .. } => self.logic_of(ty, data),
            Type::Fun {
                param,
                effects: None,
                result,
            } => Ty::Fun(
                Box::new(self.logic_of(param, data)),
                Box::new(self.logic_of(result, data)),
            ),
            Type::Fun { .. } => Ty::Refused("a function with effects is outside the domain".into()),
            Type::Con { .. } | Type::App { .. } => {
                if t.is_lang(Lang::Bool) {
                    return Ty::Bool;
                }
                if t.is_lang(Lang::Int) {
                    return Ty::Int;
                }
                let (head, args) = t.spine();
                if let Type::Skolem(s) = head
                    && self.cx.law.contains(s)
                {
                    let name = self.store.skolem_name(*s).to_owned();
                    let args = args.iter().map(|a| self.logic_of(a, data)).collect();
                    return Ty::Data {
                        module: logic::CLASS.into(),
                        name,
                        args,
                    };
                }
                let Type::Con { module, name } = head else {
                    return Ty::Refused("an applied type variable is outside the domain".into());
                };
                let (module, name) = (module.clone(), name.clone());
                let args = args.iter().map(|a| self.logic_of(a, data)).collect();
                self.declare_data_with(&module, &name, data, false);
                Ty::Data { module, name, args }
            }
            Type::Meta(_) => Ty::Refused("its type is not fully known".into()),
            Type::Error => Ty::Refused("its type has errors".into()),
            Type::Tuple(items) => Ty::Data {
                module: logic::TUPLES.into(),
                name: "Tuple".into(),
                args: items.iter().map(|t| self.logic_of(t, data)).collect(),
            },
            Type::Record(_) => Ty::Refused("a record is outside the domain".into()),
            _ => Ty::Refused("this type is outside the domain".into()),
        }
    }

    #[cfg(feature = "contracts")]
    fn contract_logic_of(&mut self, t: &Type, data: &mut logic::Data) -> Ty {
        let t = self.whnf(t);
        match &t {
            Type::Skolem(s) => Ty::Rigid(self.store.skolem_name(*s).to_owned()),
            Type::Bounded { ty, .. } | Type::Captured { ty, .. } => {
                self.contract_logic_of(ty, data)
            }
            Type::Fun {
                param,
                effects: None,
                result,
            } => Ty::Fun(
                Box::new(self.contract_logic_of(param, data)),
                Box::new(self.contract_logic_of(result, data)),
            ),
            Type::Fun { .. } => Ty::Refused("a function with effects is outside the domain".into()),
            Type::Con { .. } | Type::App { .. } => {
                if t.is_lang(Lang::Bool) {
                    return Ty::Bool;
                }
                if t.is_lang(Lang::Int) {
                    return Ty::Int;
                }
                if t.is_lang(Lang::Float) {
                    return Ty::Refused("`Float` is outside the domain".into());
                }
                let (head, args) = t.spine();
                let Type::Con { module, name } = head else {
                    return Ty::Refused("an applied type variable is outside the domain".into());
                };
                let (module, name) = (module.clone(), name.clone());
                if t.is_lang(Lang::Unit) {
                    self.declare_data_with(&module, &name, data, true);
                    return Ty::Data {
                        module,
                        name,
                        args: Vec::new(),
                    };
                }
                let is_adt = matches!(
                    self.env.type_def(&module, &name).map(|d| &d.form),
                    Some(Form::Adt { .. })
                );
                if !is_adt {
                    return Ty::Refused(format!("`{name}` is outside the domain"));
                }
                let args = args
                    .iter()
                    .map(|a| self.contract_logic_of(a, data))
                    .collect();
                self.declare_data_with(&module, &name, data, true);
                Ty::Data { module, name, args }
            }
            Type::Meta(_) => Ty::Refused("its type is not fully known".into()),
            Type::Error => Ty::Refused("its type has errors".into()),
            Type::Tuple(items) => Ty::Data {
                module: logic::TUPLES.into(),
                name: "Tuple".into(),
                args: items
                    .iter()
                    .map(|t| self.contract_logic_of(t, data))
                    .collect(),
            },
            Type::Record(_) => Ty::Refused("a record is outside the domain".into()),
            _ => Ty::Refused("this type is outside the domain".into()),
        }
    }

    /// Adds the data type `module.name` to `data`, with the types its
    /// fields name. A type found again while its fields are read names
    /// itself: recursive.
    fn declare_data_with(
        &mut self,
        module: &str,
        name: &str,
        data: &mut logic::Data,
        contracts: bool,
    ) {
        let key = (module.to_owned(), name.to_owned());
        if let Some(decl) = data.get_mut(&key) {
            // still being read: a field reached it again
            if decl.params.first().is_some_and(|p| p == READING) {
                decl.recursive = true;
            }
            return;
        }
        let constructors = match self.env.type_def(module, name).map(|d| &d.form) {
            Some(Form::Adt { constructors, .. }) => constructors.clone(),
            _ => Vec::new(),
        };
        data.insert(
            key.clone(),
            DataDecl {
                params: vec![READING.into()],
                constructors: Vec::new(),
                recursive: false,
            },
        );
        let mut params = Vec::new();
        let mut fields = Vec::new();
        for constructor in constructors {
            let entity = Entity {
                module: module.to_owned(),
                namespace: Namespace::Constructor,
                name: constructor.clone(),
            };
            let Some(scheme) = self.env.value(&entity).cloned() else {
                continue;
            };
            let mut body = match &scheme {
                Type::Forall { body, .. } => (**body).clone(),
                other => other.clone(),
            };
            let mut types = Vec::new();
            while let Type::Fun { param, result, .. } = body {
                types.push(self.logic_field_with(&param, data, contracts));
                body = *result;
            }
            // the type a constructor builds, without its capture or bound
            while let Type::Bounded { ty, .. } | Type::Captured { ty, .. } = body {
                body = *ty;
            }
            if params.is_empty() {
                params = body
                    .spine()
                    .1
                    .iter()
                    .map(|a| match a {
                        Type::Var(v) => v.clone(),
                        _ => String::new(),
                    })
                    .collect();
            }
            fields.push((constructor, types));
        }
        let decl = data.get_mut(&key).expect("declared above");
        decl.params = params;
        decl.constructors = fields;
    }

    /// The type of a field of a constructor's scheme: its variables are the
    /// data type's parameters.
    fn logic_field_with(&mut self, t: &Type, data: &mut logic::Data, contracts: bool) -> Ty {
        let t = if contracts { self.whnf(t) } else { t.clone() };
        match &t {
            Type::Var(v) => Ty::Rigid(v.clone()),
            Type::Bounded { ty, .. } | Type::Captured { ty, .. } => {
                self.logic_field_with(ty, data, contracts)
            }
            Type::Fun {
                param,
                effects: None,
                result,
            } => Ty::Fun(
                Box::new(self.logic_field_with(param, data, contracts)),
                Box::new(self.logic_field_with(result, data, contracts)),
            ),
            Type::Tuple(items) => Ty::Data {
                module: logic::TUPLES.into(),
                name: "Tuple".into(),
                args: items
                    .iter()
                    .map(|item| self.logic_field_with(item, data, contracts))
                    .collect(),
            },
            Type::Con { .. } | Type::App { .. }
                if !t.is_lang(Lang::Bool) && !t.is_lang(Lang::Int) =>
            {
                if contracts && t.is_lang(Lang::Float) {
                    return Ty::Refused("`Float` is outside the domain".into());
                }
                let (head, args) = t.spine();
                let Type::Con { module, name } = head else {
                    return Ty::Refused("an applied type variable is outside the domain".into());
                };
                let (module, name) = (module.clone(), name.clone());
                if contracts {
                    if t.is_lang(Lang::Unit) {
                        self.declare_data_with(&module, &name, data, true);
                        return Ty::Data {
                            module,
                            name,
                            args: Vec::new(),
                        };
                    }
                    let is_adt = matches!(
                        self.env.type_def(&module, &name).map(|d| &d.form),
                        Some(Form::Adt { .. })
                    );
                    if !is_adt {
                        return Ty::Refused(format!("`{name}` is outside the domain"));
                    }
                }
                let args = args
                    .iter()
                    .map(|a| self.logic_field_with(a, data, contracts))
                    .collect();
                self.declare_data_with(&module, &name, data, contracts);
                Ty::Data { module, name, args }
            }
            #[cfg(feature = "contracts")]
            other if contracts => self.contract_logic_of(other, data),
            other => self.logic_of(other, data),
        }
    }

    /// The data types of the constructors the proof names.
    pub(super) fn constructors_named(&mut self, decl: &Node, data: &mut logic::Data) {
        self.constructors_named_with(decl, data, false);
    }

    /// The data types of the constructors a contracted declaration names.
    #[cfg(feature = "contracts")]
    pub(super) fn contract_constructors_named(&mut self, decl: &Node, data: &mut logic::Data) {
        self.constructors_named_with(decl, data, true);
    }

    fn constructors_named_with(&mut self, decl: &Node, data: &mut logic::Data, contracts: bool) {
        let (start, end) = decl.range();
        let named: Vec<(String, String)> = (self.typing.names.iter())
            .filter(|((s, e), _)| *s >= start && *e <= end)
            .filter_map(|(_, r)| match r {
                Ref::Constructor(c) => Some((c.module.clone(), c.ty.clone())),
                _ => None,
            })
            .collect();
        for (module, ty) in named {
            self.declare_data_with(&module, &ty, data, contracts);
        }
    }
}

/// The parameters of a data type whose fields are being read.
const READING: &str = "$reading";
