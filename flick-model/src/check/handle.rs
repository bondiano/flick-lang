//! (Handle), flick-core's `types/handle.rs`: the body is checked at its own
//! stage; each operation's function at the type its class gives, where the
//! handler is; an effect leaves the body's row when an arm without a guard
//! covers each of its operations. The arms, `return`, `resume` and what
//! the body needs besides run at one stage, the handler's. The region of
//! `resume` is not modelled: flick-core keeps it in the arms (G002).

use std::rc::Rc;

use super::generalize::Inst;
use super::{Check, Checker, error};
use crate::handler::{Handler, Op, OpClass};
use crate::types::{Row, Stage, Type};

impl Checker<'_> {
    pub(super) fn handler(&mut self, h: &Handler, expected: Option<&Type>) -> Check<Type> {
        let t = match expected {
            Some(t) => t.clone(),
            None => self.fresh(),
        };
        let value = if h.ret.is_some() {
            self.fresh()
        } else {
            t.clone()
        };
        let (_, inner) = self.own_stage(&h.body, Some(&value))?;
        let (needed, raised) = match &inner {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (r.clone(), e.clone()),
        };
        let mut covered = Row::none();
        let mut installed = Vec::new();
        let outer = Stage::Runtime(self.fresh_row(), self.fresh_row());
        for handled in &h.effects {
            let Some(ops) = self.decls.services.get(&handled.effect).cloned() else {
                return error("Handle", format!("`{}` is no effect", handled.effect));
            };
            let params = self
                .decls
                .service_params
                .get(&handled.effect)
                .copied()
                .unwrap_or(0);
            // the effect's parameters, which its operations share
            let args: Vec<Type> = (0..params).map(|_| self.fresh()).collect();
            for op in &handled.ops {
                let Some((_, scheme)) = ops.iter().find(|(n, _)| *n == op.name) else {
                    return error(
                        "Handle",
                        format!("`{}` has no operation `{}`", handled.effect, op.name),
                    );
                };
                let (declared, insts) = self.instantiate_with(scheme);
                for (arg, inst) in args.iter().zip(&insts) {
                    if let Inst::Type(t) = inst {
                        self.unify(arg, t)?;
                    }
                }
                let arm = self.arm_type(&declared, op, &t, &outer)?;
                self.check(&op.function, &arm)?;
            }
            installed.extend(args.iter().cloned());
            let complete = ops
                .iter()
                .all(|(n, _)| handled.ops.iter().any(|o| o.name == *n && o.covers));
            if complete {
                covered.elems.insert(handled.effect.clone(), args.into());
            }
        }
        if let Some(ret) = &h.ret {
            self.check(ret, &Type::fun(value, outer.clone(), t.clone()))?;
        }
        let rest = self.minus(&needed, &covered)?;
        let errors = self.error_arms(&h.errors, &raised, &t)?;
        self.stage_within(&Stage::Runtime(rest, errors), &outer)?;
        // a handler of a service a region indexes is installed where it is live
        let live = self.indexed_live(&installed);
        self.perform(&self.live_stage(&outer, &live))?;
        Ok(t)
    }

    /// The function of an operation's arms, at `declared` instantiated:
    /// its dictionaries and arguments, then for `tail` the operation's
    /// value, for `abort` the handler's `t`, and for `control` `resume`
    /// and `t`; its last runtime stage is the handler's, `outer`.
    fn arm_type(&mut self, declared: &Type, op: &Op, t: &Type, outer: &Stage) -> Check<Type> {
        let mut params = Vec::new();
        let mut value = declared.clone();
        for _ in 0..op.dicts + op.arity {
            match self.resolve(&value) {
                Type::Fun(p, s, r) => {
                    params.push((p, s));
                    value = (*r).clone();
                }
                _ => {
                    return error(
                        "Handle",
                        format!("operation `{}` takes fewer arguments than its arm", op.name),
                    );
                }
            }
        }
        let mut ty = match op.class {
            OpClass::Tail => value,
            OpClass::Abort => t.clone(),
            OpClass::Control => {
                let resume = Type::fun(value, outer.clone(), t.clone());
                Type::fun(resume, outer.clone(), t.clone())
            }
        };
        let last = params.len();
        for (i, (p, s)) in params.into_iter().enumerate().rev() {
            let stage = match (i + 1 == last, op.class) {
                (true, OpClass::Control) => Stage::Pure,
                (true, _) => outer.clone(),
                (false, _) => s,
            };
            ty = Type::Fun(p, stage, Rc::new(ty));
        }
        Ok(ty)
    }
}
