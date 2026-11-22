//! Expression walker for the bounded concrete evaluation pass.

use std::mem;

use flick_ir::{Arm, BinOp, Expr, ExprKind, Lit, Stage};

use super::block::shadow_pat;
use super::{EvalAttempt, Hint, PassCtx};
use crate::opt::eval::value::Env;

/// Maximum AST recursion depth during the module walk (keeps native & WASM safe).
const MAX_WALK_DEPTH: u32 = 256;

impl<'a> PassCtx<'a, '_> {
    #[allow(clippy::too_many_lines, reason = "exhaustive IR expression walker")]
    pub(super) fn visit_expr<'e>(
        &mut self,
        e: &'e mut Expr,
        env: &Env<'e>,
        is_pure_app_callee: bool,
        depth: u32,
    ) -> Hint
    where
        'a: 'e,
    {
        if depth >= MAX_WALK_DEPTH {
            return Hint::NonConcrete;
        }
        match &mut e.kind {
            // Preserve canonical owner evidence brackets verbatim.
            ExprKind::WithOwnerEvidence(owner, _) => {
                self.visit_expr(owner, env, false, depth + 1);
                Hint::NonConcrete
            }
            ExprKind::Within(_, call) => {
                call.each_child_mut(&mut |child| {
                    self.visit_expr(child, env, false, depth + 1);
                });
                Hint::NonConcrete
            }
            ExprKind::Lit(_)
            | ExprKind::Con(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. } => Hint::Concrete,
            ExprKind::Local(name) => {
                if env.peek(name).is_some() {
                    Hint::Concrete
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Lambda { params, body } => {
                let mut inner_env = env.clone();
                for param in params.iter() {
                    shadow_pat(&param.pat, &mut inner_env);
                }
                self.visit_expr(body, &inner_env, false, depth + 1);
                Hint::Concrete
            }
            ExprKind::Abstract(_, inner) => {
                self.visit_expr(inner, env, false, depth + 1);
                Hint::Concrete
            }
            ExprKind::Dicts(inner, _) => {
                if self.visit_expr(inner, env, true, depth + 1) != Hint::Concrete {
                    return Hint::NonConcrete;
                }
                if is_pure_app_callee {
                    return Hint::Concrete;
                }
                self.try_eval_call_site(e, env)
            }
            ExprKind::If(cond, then_expr, else_expr) => {
                let cond_hint = self.visit_expr(cond, env, false, depth + 1);
                if let ExprKind::Lit(Lit::Bool(flag)) = cond.kind {
                    let chosen = if flag { then_expr } else { else_expr };
                    *e = mem::replace(chosen, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
                    self.changed = true;
                    return self.visit_expr(e, env, is_pure_app_callee, depth + 1);
                }
                if cond_hint == Hint::Concrete
                    && self.try_eval_and_replace(e, env) != EvalAttempt::Stopped
                {
                    return Hint::Concrete;
                }
                if let ExprKind::If(_, then_expr, else_expr) = &mut e.kind {
                    self.visit_expr(then_expr, env, false, depth + 1);
                    self.visit_expr(else_expr, env, false, depth + 1);
                }
                Hint::NonConcrete
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) => {
                let op = *op;
                let lhs_hint = self.visit_expr(lhs, env, false, depth + 1);
                let short = matches!(
                    (op, &lhs.kind),
                    (BinOp::And, ExprKind::Lit(Lit::Bool(false)))
                        | (BinOp::Or, ExprKind::Lit(Lit::Bool(true)))
                );
                if short {
                    *e = mem::replace(lhs, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
                    self.changed = true;
                    return Hint::Concrete;
                }
                let rhs_hint = self.visit_expr(rhs, env, false, depth + 1);
                if lhs_hint == Hint::Concrete && rhs_hint == Hint::Concrete {
                    return self.eval_attempt_hint(e, env);
                }
                Hint::NonConcrete
            }
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } => {
                let is_pure = *stage == Stage::Pure;
                let arg_first = *arg_first;
                let callee_flag = is_pure && is_curried_prefix(func);
                let (func_hint, arg_hint) = if arg_first {
                    let ah = self.visit_expr(arg, env, false, depth + 1);
                    let fh = self.visit_expr(func, env, callee_flag, depth + 1);
                    (fh, ah)
                } else {
                    let fh = self.visit_expr(func, env, callee_flag, depth + 1);
                    let ah = self.visit_expr(arg, env, false, depth + 1);
                    (fh, ah)
                };
                if !is_pure || func_hint != Hint::Concrete || arg_hint != Hint::Concrete {
                    return Hint::NonConcrete;
                }
                if is_pure_app_callee {
                    return Hint::Concrete;
                }
                self.try_eval_call_site(e, env)
            }
            ExprKind::Block(..) => self.visit_block(e, env, depth),
            ExprKind::Match(scrutinee, _) => {
                if self.visit_expr(scrutinee, env, false, depth + 1) == Hint::Concrete
                    && self.try_eval_and_replace(e, env) != EvalAttempt::Stopped
                {
                    return Hint::Concrete;
                }
                if let ExprKind::Match(_, arms) = &mut e.kind {
                    self.visit_arms(arms, env, depth);
                }
                Hint::NonConcrete
            }
            ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::Record(_)
            | ExprKind::Dictionary(_) => {
                if self.visit_children_concrete(e, env, depth) {
                    Hint::Concrete
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Update(..)
            | ExprKind::Field(..)
            | ExprKind::Binary(..)
            | ExprKind::Neg(..)
            | ExprKind::Not(_)
            | ExprKind::Ignore(_) => {
                if self.visit_children_concrete(e, env, depth) {
                    self.eval_attempt_hint(e, env)
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Each {
                stop,
                pat,
                source,
                body,
            } => {
                if let Some(stop) = stop {
                    self.visit_expr(stop, env, false, depth + 1);
                }
                self.visit_expr(source, env, false, depth + 1);
                let mut body_env = env.clone();
                shadow_pat(pat, &mut body_env);
                self.visit_expr(body, &body_env, false, depth + 1);
                Hint::NonConcrete
            }
            ExprKind::Handle(h) => {
                for eff in &mut h.effects {
                    for op in &mut eff.operations {
                        self.visit_expr(&mut op.function, env, false, depth + 1);
                    }
                }
                self.visit_expr(&mut h.body, env, false, depth + 1);
                if let Some(ret) = &mut h.ret {
                    self.visit_expr(ret, env, false, depth + 1);
                }
                self.visit_arms(&mut h.errors, env, depth);
                Hint::NonConcrete
            }
            _ => {
                e.each_child_mut(&mut |child| {
                    self.visit_expr(child, env, false, depth + 1);
                });
                Hint::NonConcrete
            }
        }
    }

    fn visit_arms<'e>(&mut self, arms: &'e mut [Arm], env: &Env<'e>, depth: u32)
    where
        'a: 'e,
    {
        for arm in arms {
            let mut arm_env = env.clone();
            shadow_pat(&arm.pat, &mut arm_env);
            if let Some(guard) = &mut arm.guard {
                self.visit_expr(guard, &arm_env, false, depth + 1);
            }
            self.visit_expr(&mut arm.body, &arm_env, false, depth + 1);
        }
    }

    fn visit_children_concrete<'e>(&mut self, e: &'e mut Expr, env: &Env<'e>, depth: u32) -> bool
    where
        'a: 'e,
    {
        let mut all_concrete = true;
        e.each_child_mut(&mut |child| {
            if self.visit_expr(child, env, false, depth + 1) != Hint::Concrete {
                all_concrete = false;
            }
        });
        all_concrete
    }

    fn eval_attempt_hint<'e>(&mut self, e: &'e mut Expr, env: &Env<'e>) -> Hint
    where
        'a: 'e,
    {
        match self.try_eval_and_replace(e, env) {
            EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
            EvalAttempt::Stopped => Hint::NonConcrete,
        }
    }
}

fn is_curried_prefix(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::App {
            stage: Stage::Pure,
            ..
        } | ExprKind::Global { .. }
            | ExprKind::Con(_)
            | ExprKind::Method { .. }
            | ExprKind::Dicts(..)
    )
}
