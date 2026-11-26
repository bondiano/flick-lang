//! Runtime dictionary dispatch, pattern matching, operators, and built-in
//! foreign helpers for the real-Flick specialization fixture backend.

use std::collections::BTreeMap;
use std::rc::Rc;

use flick_backend_support::specialize::{SpecDict, SpecPat};
use flick_ir::Lit;

use crate::backend::{DictVal, Env, FixtureBackend, Val};
pub(crate) use crate::builtins::eval_binary;
use crate::builtins::eval_builtin_foreign;

impl FixtureBackend<'_> {
    pub(crate) fn eval_dict(&mut self, dict: &SpecDict, env: &mut Env) -> DictVal {
        match dict {
            SpecDict::Instance { target, args, .. } => {
                let inst = self.program.instances[target].clone();
                let mut ienv = Env::default();
                for (cname, darg) in inst.context.iter().zip(args) {
                    ienv.dicts.insert(cname.clone(), self.eval_dict(darg, env));
                }
                let methods = inst
                    .methods
                    .iter()
                    .map(|(m, e)| (m.clone(), self.eval_expr(e, &mut ienv)))
                    .collect();
                let supers = inst
                    .supers
                    .iter()
                    .map(|(s, d)| (s.clone(), self.eval_dict(d, &mut ienv)))
                    .collect();
                DictVal { methods, supers }
            }
            SpecDict::Given(name) => env
                .dicts
                .get(name)
                .unwrap_or_else(|| panic!("unbound runtime dict `{name}`"))
                .clone(),
            SpecDict::Value { target } => {
                match unbox_if_boxed(self.eval_symbol(target, Vec::new())) {
                    Val::Dict(d) => d,
                    other => panic!("Dict::Value expected Dict, got {other:?}"),
                }
            }
            SpecDict::Opened { binder, field } => {
                let rec = unbox_if_boxed(env.locals[binder].clone());
                let fv = match rec {
                    Val::RecordMap(m) => m[field].clone(),
                    Val::RecordSlots(s) => {
                        s.into_iter()
                            .find(|(l, _)| l == field)
                            .expect("opened dict field present")
                            .1
                    }
                    other => panic!("Opened expected record, got {other:?}"),
                };
                match unbox_if_boxed(fv) {
                    Val::Dict(d) => d,
                    other => panic!("Opened dict field `{field}` expected Dict, got {other:?}"),
                }
            }
            SpecDict::Field(local) => match unbox_if_boxed(env.locals[local].clone()) {
                Val::Dict(d) => d,
                other => panic!("GADT dict field `{local}` expected Dict, got {other:?}"),
            },
            SpecDict::Super { dict: inner, class } => {
                self.eval_dict(inner, env).supers[class].clone()
            }
            SpecDict::Structural { class, fields } => {
                let _ = (class, fields);
                DictVal {
                    methods: BTreeMap::new(),
                    supers: BTreeMap::new(),
                }
            }
            other => panic!("unsupported SpecDict in fixture backend: {other:?}"),
        }
    }

    pub(crate) fn apply_dicts(&mut self, target: Val, dvals: Vec<DictVal>) -> Val {
        let Val::Closure(cl) = unbox_if_boxed(target) else {
            panic!("apply_dicts expected Closure");
        };
        let mut next = (*cl).clone();
        next.applied_dicts.extend(dvals);
        if next.applied_dicts.len() == next.dict_params.len() && next.params.is_empty() {
            let mut cenv = next.env.clone();
            if let Some(ref s) = next.self_name {
                cenv.locals.insert(s.clone(), Val::Closure(cl));
            }
            for (n, d) in next.dict_params.iter().zip(next.applied_dicts) {
                cenv.dicts.insert(n.clone(), d);
            }
            self.eval_expr(&next.body, &mut cenv)
        } else {
            Val::Closure(Rc::new(next))
        }
    }

    pub(crate) fn apply_val(&mut self, func: Val, arg: Val) -> Val {
        match unbox_if_boxed(func) {
            Val::Closure(cl) => {
                let mut next = (*cl).clone();
                next.applied_args.push(arg);
                if next.applied_args.len() < next.params.len() {
                    return Val::Closure(Rc::new(next));
                }
                let mut cenv = next.env.clone();
                if let Some(ref s) = next.self_name {
                    cenv.locals.insert(s.clone(), Val::Closure(cl));
                }
                for (n, d) in next.dict_params.iter().zip(&next.applied_dicts) {
                    cenv.dicts.insert(n.clone(), d.clone());
                }
                for (p, v) in next.params.iter().zip(next.applied_args) {
                    assert!(
                        match_pat(&p.pat, &v, &mut cenv),
                        "param pattern failed on {v:?}"
                    );
                }
                self.eval_expr(&next.body, &mut cenv)
            }
            Val::PartialCon {
                name,
                arity,
                mut fields,
                dicts,
            } => {
                fields.push(arg);
                if fields.len() == arity {
                    Val::Con {
                        name,
                        fields,
                        dicts,
                    }
                } else {
                    Val::PartialCon {
                        name,
                        arity,
                        fields,
                        dicts,
                    }
                }
            }
            Val::ForeignFn {
                module,
                name,
                arity,
                polymorphic,
                mut applied,
            } => {
                applied.push(arg);
                if applied.len() < arity {
                    return Val::ForeignFn {
                        module,
                        name,
                        arity,
                        polymorphic,
                        applied,
                    };
                }
                if let Some(hook) = &self.foreign_hook
                    && let Some(val) = hook(&module, &name, polymorphic, applied.clone())
                {
                    return val;
                }
                eval_builtin_foreign(&module, &name, polymorphic, &applied)
            }
            other => panic!("cannot apply non-function {other:?}"),
        }
    }
}

pub(crate) fn box_deep(v: Val) -> Val {
    match v {
        Val::Boxed(_) => v,
        Val::Tuple(items) => Val::Boxed(Box::new(Val::Tuple(
            items.into_iter().map(box_deep).collect(),
        ))),
        Val::List(items) => Val::Boxed(Box::new(Val::List(
            items.into_iter().map(box_deep).collect(),
        ))),
        Val::RecordSlots(slots) => Val::Boxed(Box::new(Val::RecordMap(
            slots
                .into_iter()
                .map(|(k, val)| (k, box_deep(val)))
                .collect(),
        ))),
        Val::RecordMap(map) => Val::Boxed(Box::new(Val::RecordMap(
            map.into_iter().map(|(k, val)| (k, box_deep(val))).collect(),
        ))),
        Val::Con {
            name,
            fields,
            dicts,
        } => Val::Boxed(Box::new(Val::Con {
            name,
            fields: fields.into_iter().map(box_deep).collect(),
            dicts,
        })),
        other => Val::Boxed(Box::new(other)),
    }
}

pub(crate) fn unbox_deep(v: Val) -> Val {
    let Val::Boxed(inner) = v else {
        panic!("Unbox expected Val::Boxed, got {v:?}");
    };
    unbox_val(*inner)
}

fn unbox_val(v: Val) -> Val {
    match v {
        Val::Boxed(inner) => unbox_val(*inner),
        Val::Tuple(items) => Val::Tuple(items.into_iter().map(unbox_val).collect()),
        Val::List(items) => Val::List(items.into_iter().map(unbox_val).collect()),
        Val::RecordMap(map) => {
            let mut slots: Vec<(String, Val)> = map
                .into_iter()
                .map(|(k, val)| (k, unbox_val(val)))
                .collect();
            flick_ir::sort_by_utf16(&mut slots, |(l, _)| l.as_str());
            Val::RecordSlots(slots)
        }
        Val::Con {
            name,
            fields,
            dicts,
        } => Val::Con {
            name,
            fields: fields.into_iter().map(unbox_val).collect(),
            dicts,
        },
        other => other,
    }
}

pub(crate) fn unbox_if_boxed(v: Val) -> Val {
    match v {
        Val::Boxed(inner) => unbox_if_boxed(*inner),
        other => other,
    }
}

pub(crate) fn match_pat(pat: &SpecPat, val: &Val, env: &mut Env) -> bool {
    match pat {
        SpecPat::Wild => true,
        SpecPat::Bind(name) => {
            env.locals.insert(name.clone(), val.clone());
            true
        }
        SpecPat::Lit(lit) => match (lit, unbox_if_boxed(val.clone())) {
            (Lit::Unit, Val::Unit) => true,
            (Lit::Bool(a), Val::Bool(b)) => *a == b,
            (Lit::Int(a), Val::Int(b)) => *a == b,
            (Lit::Float(a), Val::Float(b)) => a.to_bits() == b.to_bits(),
            (Lit::String(a), Val::String(b)) => *a == b,
            _ => false,
        },
        SpecPat::Tuple(pats) => {
            let Val::Tuple(items) = unbox_if_boxed(val.clone()) else {
                return false;
            };
            pats.len() == items.len() && pats.iter().zip(&items).all(|(p, v)| match_pat(p, v, env))
        }
        SpecPat::List(pats, rest) => {
            let Val::List(items) = unbox_if_boxed(val.clone()) else {
                return false;
            };
            if rest.is_some() {
                if items.len() < pats.len() {
                    return false;
                }
            } else if items.len() != pats.len() {
                return false;
            }
            for (p, v) in pats.iter().zip(&items) {
                if !match_pat(p, v, env) {
                    return false;
                }
            }
            match rest {
                Some(r) => match_pat(r, &Val::List(items[pats.len()..].to_vec()), env),
                None => true,
            }
        }
        SpecPat::Record { fields, .. } => {
            let v = unbox_if_boxed(val.clone());
            for (label, loc, p) in fields {
                let fv = match (&v, loc) {
                    (Val::RecordSlots(slots), Some((idx, _))) => slots[*idx].1.clone(),
                    (Val::RecordSlots(slots), None) => {
                        let Some((_, fv)) = slots.iter().find(|(l, _)| l == label) else {
                            return false;
                        };
                        fv.clone()
                    }
                    (Val::RecordMap(map), _) => {
                        let Some(fv) = map.get(label) else {
                            return false;
                        };
                        fv.clone()
                    }
                    _ => return false,
                };
                if !match_pat(p, &fv, env) {
                    return false;
                }
            }
            true
        }
        SpecPat::Con { ctor, args, .. } => {
            let Val::Con {
                name,
                fields,
                dicts,
            } = unbox_if_boxed(val.clone())
            else {
                return false;
            };
            if name != ctor.name || args.len() != fields.len() + dicts.len() {
                return false;
            }
            for (p, fv) in args.iter().zip(&fields) {
                if !match_pat(p, fv, env) {
                    return false;
                }
            }
            for (p, dv) in args[fields.len()..].iter().zip(&dicts) {
                if !match_pat(p, &Val::Dict(dv.clone()), env) {
                    return false;
                }
            }
            true
        }
        SpecPat::Or(alts) => alts.iter().any(|a| match_pat(a, val, env)),
    }
}
