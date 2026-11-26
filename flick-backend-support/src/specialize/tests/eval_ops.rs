//! Dictionary evaluation, application, pattern matching, and operators for
//! the unit-test fixture evaluator.

use std::rc::Rc;

use flick_ir::{Arith, BinOp, Lit, Num};

use super::super::{SpecDict, SpecPat};
use super::eval::{DictVal, Env, FixtureEval, Val};

impl FixtureEval<'_> {
    pub(super) fn eval_dict(&mut self, dict: &SpecDict, env: &mut Env) -> DictVal {
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
            SpecDict::Given(name) => env.dicts[name].clone(),
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
                    other => panic!("Opened dict expected Dict, got {other:?}"),
                }
            }
            SpecDict::Field(local) => match unbox_if_boxed(env.locals[local].clone()) {
                Val::Dict(d) => d,
                other => panic!("GADT dict expected Dict, got {other:?}"),
            },
            SpecDict::Super { dict: inner, class } => {
                self.eval_dict(inner, env).supers[class].clone()
            }
            other => panic!("unsupported SpecDict: {other:?}"),
        }
    }

    pub(super) fn apply_dicts(&mut self, target: Val, dvals: Vec<DictVal>) -> Val {
        let Val::Closure(cl) = unbox_if_boxed(target) else {
            panic!("apply_dicts expected Closure");
        };
        let mut next = (*cl).clone();
        next.applied_dicts.extend(dvals);
        if next.applied_dicts.len() == next.dict_params.len() && next.params.is_empty() {
            let mut cenv = next.env.clone();
            for (n, d) in next.dict_params.iter().zip(next.applied_dicts) {
                cenv.dicts.insert(n.clone(), d);
            }
            self.eval_expr(&next.body, &mut cenv)
        } else {
            Val::Closure(Rc::new(next))
        }
    }

    pub(super) fn apply_val(&mut self, func: Val, arg: Val) -> Val {
        match unbox_if_boxed(func) {
            Val::Closure(cl) => {
                let mut next = (*cl).clone();
                next.applied_args.push(arg);
                if next.applied_args.len() < next.params.len() {
                    return Val::Closure(Rc::new(next));
                }
                let mut cenv = next.env.clone();
                for (n, d) in next.dict_params.iter().zip(&next.applied_dicts) {
                    cenv.dicts.insert(n.clone(), d.clone());
                }
                for (p, v) in next.params.iter().zip(next.applied_args) {
                    assert!(match_pat(&p.pat, &v, &mut cenv));
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
                if let Some(hook) = &self.foreign_hook {
                    return hook(&module, &name, applied);
                }
                panic!("no foreign hook for `{module}.{name}`")
            }
            other => panic!("cannot apply non-function {other:?}"),
        }
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

pub(crate) fn eval_binary(op: BinOp, lhs: Val, rhs: Val) -> Val {
    match op {
        BinOp::Concat => match (lhs, rhs) {
            (Val::String(a), Val::String(b)) => Val::String(a + &b),
            (l, r) => panic!("Concat mismatch: {l:?}, {r:?}"),
        },
        BinOp::Arith(arith, Num::Int) => {
            let (Val::Int(a), Val::Int(b)) = (lhs, rhs) else {
                panic!("Int arith mismatch");
            };
            Val::Int(match arith {
                Arith::Add => a.wrapping_add(b),
                Arith::Sub => a.wrapping_sub(b),
                Arith::Mul => a.wrapping_mul(b),
                Arith::Div => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_div(b)
                    }
                }
                Arith::Rem => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_rem(b)
                    }
                }
                Arith::Pow => a.wrapping_pow(u32::try_from(b).unwrap_or(0)),
            })
        }
        BinOp::Arith(arith, Num::Float) => {
            let (Val::Float(a), Val::Float(b)) = (lhs, rhs) else {
                panic!("Float arith mismatch");
            };
            Val::Float(match arith {
                Arith::Add => a + b,
                Arith::Sub => a - b,
                Arith::Mul => a * b,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow => a.powf(b),
            })
        }
        other => panic!("unsupported binop {other:?}"),
    }
}
