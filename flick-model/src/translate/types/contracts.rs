//! Translation of `Form::ModuleContract` requirements into the model's
//! [`ModuleContract`], expanding manifest types and manifest nominal
//! identities while keeping abstract type members bound to `CONTRACT_SCOPE`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use flick_core::types as fc;

use super::{Types, kind};
use crate::decls::ModuleContract;
use crate::syntax::{Name, Path};
use crate::types::{CONTRACT_SCOPE, Kind};

impl Types<'_> {
    pub fn module_contract(&self, requirements: &[fc::ModuleRequirement]) -> ModuleContract {
        let mut packable = true;
        let mut abstract_types = Vec::new();
        let mut abstract_names = HashSet::new();
        let mut manifests: HashMap<String, (Vec<String>, fc::Type)> = HashMap::new();
        let mut nominals: HashMap<String, (String, String, bool)> = HashMap::new();
        for req in requirements {
            match req {
                fc::ModuleRequirement::Type {
                    name,
                    params,
                    manifest,
                } => {
                    if let Some(body) = manifest {
                        manifests.insert(
                            name.clone(),
                            (
                                params.iter().map(|p| p.name.clone()).collect(),
                                body.clone(),
                            ),
                        );
                    } else {
                        let k = params.iter().rev().fold(Kind::Type, |res, p| {
                            Kind::Arrow(Rc::new(kind(&p.kind).unwrap_or(Kind::Type)), Rc::new(res))
                        });
                        abstract_types.push((Name::from(name.as_str()), k));
                        abstract_names.insert(name.clone());
                    }
                }
                fc::ModuleRequirement::Nominal {
                    name,
                    category,
                    identity,
                    ..
                } => {
                    if let Some(entity) = identity {
                        nominals.insert(
                            name.clone(),
                            (
                                entity.module.clone(),
                                entity.name.clone(),
                                *category == fc::ModuleNominalKind::Class,
                            ),
                        );
                    } else {
                        packable = false;
                    }
                }
                fc::ModuleRequirement::Module { .. } => {
                    packable = false;
                }
                fc::ModuleRequirement::Kind { .. }
                | fc::ModuleRequirement::Value { .. }
                | fc::ModuleRequirement::Dictionary { .. } => {}
            }
        }
        let ctx = ContractEnv {
            manifests: &manifests,
            nominals: &nominals,
            abstract_names: &abstract_names,
        };
        let manifest_types = manifests
            .iter()
            .map(|(name, (params, body))| {
                let bound: HashSet<String> = params.iter().cloned().collect();
                let expanded = ctx.expand(body, &bound, 32);
                let vars = params.iter().map(|p| Name::from(p.as_str())).collect();
                (Name::from(name.as_str()), (vars, self.ty(&expanded)))
            })
            .collect();
        let nominals_map = nominals
            .iter()
            .map(|(local, (m, n, _))| (Name::from(local.as_str()), Path::new(m, n)))
            .collect();
        let empty = HashSet::new();
        let fields = requirements
            .iter()
            .filter_map(|req| match req {
                fc::ModuleRequirement::Value { name, ty } => {
                    let expanded = ctx.expand(ty, &empty, 32);
                    Some((Name::from(name.as_str()), self.scheme(&expanded)))
                }
                _ => None,
            })
            .collect();
        ModuleContract {
            abstract_types,
            manifest_types,
            nominals: nominals_map,
            fields,
            packable,
        }
    }
}

struct ContractEnv<'a> {
    manifests: &'a HashMap<String, (Vec<String>, fc::Type)>,
    nominals: &'a HashMap<String, (String, String, bool)>,
    abstract_names: &'a HashSet<String>,
}

impl ContractEnv<'_> {
    fn expand(&self, ty: &fc::Type, bound: &HashSet<String>, fuel: usize) -> fc::Type {
        if fuel == 0 {
            return ty.clone();
        }
        match ty {
            fc::Type::Var(name) if !bound.contains(name) => {
                if let Some((params, body)) = self.manifests.get(name)
                    && params.is_empty()
                {
                    return self.expand(body, &HashSet::new(), fuel - 1);
                }
                if let Some((m, n, _)) = self.nominals.get(name) {
                    return fc::Type::con(m, n);
                }
                if let Some(cls) = name.strip_suffix("$Dict")
                    && let Some((m, n, true)) = self.nominals.get(cls)
                {
                    return fc::Type::con(m, &format!("{n}$Dict"));
                }
                if self.abstract_names.contains(name) {
                    return fc::Type::con(CONTRACT_SCOPE, name);
                }
                ty.clone()
            }
            fc::Type::App { .. } => {
                let (head, args) = ty.spine();
                let expanded_args: Vec<fc::Type> = args
                    .into_iter()
                    .map(|a| self.expand(a, bound, fuel - 1))
                    .collect();
                if let fc::Type::Var(name) = head
                    && !bound.contains(name)
                    && let Some((params, body)) = self.manifests.get(name)
                    && !params.is_empty()
                    && expanded_args.len() >= params.len()
                {
                    let inner_bound: HashSet<String> = params.iter().cloned().collect();
                    let expanded_body = self.expand(body, &inner_bound, fuel - 1);
                    let subst: Vec<(String, fc::Type)> = params
                        .iter()
                        .cloned()
                        .zip(expanded_args[..params.len()].iter().cloned())
                        .collect();
                    let applied = fc::Type::apps(
                        expanded_body.substitute(&subst),
                        expanded_args[params.len()..].iter().cloned(),
                    );
                    return self.expand(&applied, bound, fuel - 1);
                }
                let expanded_head = self.expand(head, bound, fuel - 1);
                fc::Type::apps(expanded_head, expanded_args)
            }
            fc::Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mut inner = bound.clone();
                inner.extend(vars.iter().map(|b| b.name.clone()));
                fc::Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints
                        .iter()
                        .map(|c| self.expand(c, &inner, fuel - 1))
                        .collect(),
                    body: Box::new(self.expand(body, &inner, fuel - 1)),
                }
            }
            fc::Type::Row(r) => {
                let mut entries: Vec<(fc::Elem, fc::Type)> = r
                    .entries
                    .iter()
                    .map(|(label, elem)| {
                        let expanded = self.expand(elem, bound, fuel - 1);
                        let new_label = match expanded.spine().0 {
                            fc::Type::Con { module, name }
                                if matches!(label, fc::Elem::Service(_) | fc::Elem::Error(_)) =>
                            {
                                fc::Elem::named(format!("{module}.{name}"))
                            }
                            _ => label.clone(),
                        };
                        (new_label, expanded)
                    })
                    .collect();
                fc::Row::sort_entries(&mut entries);
                fc::Type::Row(fc::Row {
                    entries,
                    tail: r
                        .tail
                        .as_deref()
                        .map(|t| Box::new(self.expand(t, bound, fuel - 1))),
                    written: r.written,
                })
            }
            _ => ty.map_children(&mut |child| self.expand(child, bound, fuel - 1)),
        }
    }
}
