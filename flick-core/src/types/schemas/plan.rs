//! Plans of witnesses: the witness of each field's type, by its form.

use super::{Witness, companion_name};
use crate::iface::{Entity, Namespace};
use crate::types::Checker;
use crate::types::kinds::Kind;
use crate::types::ty::{PRELUDE, Type};
use crate::types::typing::SchemaPlan;

pub(super) struct PlanContext<'a> {
    pub(super) params: &'a [Witness],
    pub(super) group: &'a [String],
    pub(super) module: &'a str,
}

impl Checker<'_> {
    /// The witness plan of a field of type `ty`.
    pub(super) fn plan(
        &self,
        ty: &Type,
        path: &str,
        ctx: &PlanContext,
    ) -> Result<SchemaPlan, (String, String)> {
        let no_witness = |shown: String, why: &str| {
            Err((
                path.to_owned(),
                format!("a field of type `{shown}` has no Schema witness: {why}"),
            ))
        };
        let (ty, qualifier) = self.whnf_captured(ty);
        if qualifier.is_some_and(|q| !self.store.normalize(&q).entries.is_empty()) {
            return no_witness(self.show(&ty), "it holds a scope's capabilities");
        }
        match &ty {
            Type::Var(v) => match ctx.params.iter().position(|w| &w.var == v && !w.higher) {
                Some(i) => Ok(SchemaPlan::Param(i)),
                None => no_witness(v.clone(), "a type variable without a witness"),
            },
            Type::Tuple(items) => Ok(SchemaPlan::Tuple(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, t)| self.plan(t, &format!("{path}[{i}]"), ctx))
                    .collect::<Result<_, _>>()?,
            )),
            Type::Record(row) if row.tail.is_none() => Ok(SchemaPlan::Record(
                row.entries
                    .iter()
                    .map(|(l, t)| {
                        Ok::<_, (String, String)>((
                            l.clone(),
                            self.plan(t, &format!("{path}.{l}"), ctx)?,
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            )),
            Type::Record(_) => no_witness(self.show(&ty), "an open record"),
            Type::Fun { .. } => {
                no_witness(self.show(&ty), "a function cannot be checked or written")
            }
            Type::App { .. } | Type::Con { .. } => self.named_plan(&ty, path, ctx),
            _ => no_witness(self.show(&ty), "its type is not known"),
        }
    }

    fn named_plan(
        &self,
        ty: &Type,
        path: &str,
        ctx: &PlanContext,
    ) -> Result<SchemaPlan, (String, String)> {
        let (head, args) = ty.spine();
        let shown = self.show(ty);
        let no_witness = |why: &str| {
            Err((
                path.to_owned(),
                format!("a field of type `{shown}` has no Schema witness: {why}"),
            ))
        };
        if let Type::Var(f) = head {
            return match (
                ctx.params.iter().position(|w| &w.var == f && w.higher),
                &args[..],
            ) {
                (Some(i), [arg]) => {
                    Ok(SchemaPlan::Applied(i, Box::new(self.plan(arg, path, ctx)?)))
                }
                _ => no_witness("a type constructor variable without a witness"),
            };
        }
        let Type::Con { module, name } = head else {
            return no_witness("its type is not known");
        };
        let arg = |i: usize| self.plan(args[i], path, ctx);
        if module == PRELUDE {
            match (name.as_str(), args.len()) {
                (
                    "Int" | "Float" | "String" | "Bool" | "BigInt" | "Unit" | "Never" | "Unknown",
                    0,
                ) => {
                    return Ok(SchemaPlan::Prim(name.clone()));
                }
                ("List", 1) => return Ok(SchemaPlan::List(Box::new(arg(0)?))),
                ("Option", 1) => return Ok(SchemaPlan::Option(Box::new(arg(0)?))),
                ("Redacted", 1) => return Ok(SchemaPlan::Redacted(Box::new(arg(0)?))),
                ("Result", 2) => {
                    return Ok(SchemaPlan::Result(Box::new(arg(0)?), Box::new(arg(1)?)));
                }
                ("Ref", _) => {
                    return no_witness("a mutable cell is snapshot at run time, not checked");
                }
                (
                    "Eff" | "Layer" | "ServiceTag" | "ServiceImpl" | "Codec" | "PureCodec"
                    | "Schema",
                    _,
                ) => {
                    return no_witness("it is a runtime description, not data");
                }
                _ => {}
            }
        }
        let entity = Entity {
            module: module.clone(),
            namespace: Namespace::Value,
            name: companion_name(name, "schema"),
        };
        let Some(scheme) = self.env.value(&entity) else {
            let why = if self.type_has_region(module, name) {
                "a scoped resource has no wire form"
            } else {
                "derive `Schema` for it, or give its field a codec"
            };
            return no_witness(why);
        };
        // the witness takes the schemas of the arguments it is generic over
        let result_args = witness_vars(scheme);
        let mut plans = Vec::new();
        for (var, higher) in result_args {
            let position = scheme_params(scheme).iter().position(|p| *p == var);
            let Some(a) = position.and_then(|i| args.get(i)) else {
                return no_witness("its arguments do not match its witness");
            };
            plans.push(if higher {
                self.constructor_witness(a, path, ctx)?
            } else {
                self.plan(a, path, ctx)?
            });
        }
        Ok(SchemaPlan::Named {
            module: module.clone(),
            ty: name.clone(),
            args: plans,
            lazy: *module == ctx.module && ctx.group.contains(name),
        })
    }

    /// A witness `forall x. Schema x -> Schema (t x)` for a type constructor.
    fn constructor_witness(
        &self,
        t: &Type,
        path: &str,
        ctx: &PlanContext,
    ) -> Result<SchemaPlan, (String, String)> {
        match t {
            Type::Var(f) => match ctx.params.iter().position(|w| &w.var == f && w.higher) {
                Some(i) => Ok(SchemaPlan::Param(i)),
                None => Err((path.to_owned(), format!("`{f}` has no witness"))),
            },
            Type::Con { module, name }
                if module == PRELUDE && matches!(name.as_str(), "List" | "Option" | "Redacted") =>
            {
                Ok(SchemaPlan::Constructor(name.clone()))
            }
            Type::Con { module, name } => {
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Value,
                    name: companion_name(name, "schema"),
                };
                if self.env.value(&entity).is_some() {
                    Ok(SchemaPlan::Named {
                        module: module.clone(),
                        ty: name.clone(),
                        args: Vec::new(),
                        lazy: false,
                    })
                } else {
                    Err((path.to_owned(), format!("`{name}` derives no Schema")))
                }
            }
            _ => Err((
                path.to_owned(),
                "a type constructor without a witness".to_owned(),
            )),
        }
    }

    fn type_has_region(&self, module: &str, name: &str) -> bool {
        let Some(def) = self.env.type_def(module, name) else {
            return false;
        };
        let mut kind = def.kind.clone();
        while let Kind::Arrow(param, result) = kind {
            if *param == Kind::Region {
                return true;
            }
            kind = *result;
        }
        false
    }
}

/// The variables a witness scheme takes schemas of, in order, and whether
/// each is over type constructors.
fn witness_vars(scheme: &Type) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut ty = match scheme {
        Type::Forall { body, .. } => &**body,
        other => other,
    };
    while let Type::Fun { param, result, .. } = ty {
        match &**param {
            Type::App { arg, .. } => {
                if let Type::Var(v) = &**arg {
                    out.push((v.clone(), false));
                }
            }
            Type::Forall { body, .. } => {
                if let Type::Fun { result, .. } = &**body
                    && let Type::App { arg, .. } = &**result
                    && let (Type::Var(f), _) = arg.spine()
                {
                    out.push((f.clone(), true));
                }
            }
            _ => {}
        }
        ty = result;
    }
    out
}

/// The variables of the type a witness scheme is for, in parameter order.
fn scheme_params(scheme: &Type) -> Vec<String> {
    let mut ty = match scheme {
        Type::Forall { body, .. } => &**body,
        other => other,
    };
    while let Type::Fun { result, .. } = ty {
        ty = result;
    }
    let Type::App { arg, .. } = ty else {
        return Vec::new();
    };
    arg.spine()
        .1
        .iter()
        .filter_map(|a| match a {
            Type::Var(v) => Some(v.clone()),
            _ => None,
        })
        .collect()
}
