//! Expression walker for the bounded concrete evaluation pass.

use flick_ir::{BinOp, Expr, ExprKind, Lit, Stage};

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
            ExprKind::Lit(_) | ExprKind::Con(_) => Hint::Concrete,
            ExprKind::Local(name) => {
                if env.peek(name).is_some() {
                    Hint::Concrete
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Var(_)
            | ExprKind::ReadOwnerEvidence
            | ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => Hint::NonConcrete,
            ExprKind::Global { .. } | ExprKind::Method { .. } => {
                // Bare globals and methods are not replaced directly (avoid duplicating
                // shared top-level values/defaults); they remain concrete operands for
                // enclosing calls/matches/projections.
                Hint::Concrete
            }
            ExprKind::Lambda { params, body } => {
                let mut inner_env = env.clone();
                for param in params.iter() {
                    shadow_pat(&param.pat, &mut inner_env);
                }
                self.visit_expr(body, &inner_env, false, depth + 1);
                // Creating a closure is inert even if its body reads dynamic locals.
                Hint::Concrete
            }
            ExprKind::Abstract(_, inner) => {
                self.visit_expr(inner, env, false, depth + 1);
                Hint::Concrete
            }
            ExprKind::Dicts(inner, _) => {
                let ih = self.visit_expr(inner, env, true, depth + 1);
                if ih != Hint::Concrete {
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
                    let chosen = if flag {
                        then_expr.as_ref().clone()
                    } else {
                        else_expr.as_ref().clone()
                    };
                    *e = chosen;
                    self.changed = true;
                    return self.visit_expr(e, env, is_pure_app_callee, depth + 1);
                }
                if cond_hint == Hint::Concrete {
                    match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => return Hint::Concrete,
                        EvalAttempt::Stopped => {}
                    }
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
                    *e = lhs.as_ref().clone();
                    self.changed = true;
                    return Hint::Concrete;
                }
                let rhs_hint = self.visit_expr(rhs, env, false, depth + 1);
                if lhs_hint == Hint::Concrete && rhs_hint == Hint::Concrete {
                    return match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                        EvalAttempt::Stopped => Hint::NonConcrete,
                    };
                }
                Hint::NonConcrete
            }
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } => {
                let stage = *stage;
                let arg_first = *arg_first;
                if stage != Stage::Pure {
                    if arg_first {
                        self.visit_expr(arg, env, false, depth + 1);
                        self.visit_expr(func, env, false, depth + 1);
                    } else {
                        self.visit_expr(func, env, false, depth + 1);
                        self.visit_expr(arg, env, false, depth + 1);
                    }
                    return Hint::NonConcrete;
                }
                let (func_hint, arg_hint) = if arg_first {
                    let ah = self.visit_expr(arg, env, false, depth + 1);
                    let fh = self.visit_expr(func, env, is_curried_prefix(func), depth + 1);
                    (fh, ah)
                } else {
                    let fh = self.visit_expr(func, env, is_curried_prefix(func), depth + 1);
                    let ah = self.visit_expr(arg, env, false, depth + 1);
                    (fh, ah)
                };
                if func_hint != Hint::Concrete || arg_hint != Hint::Concrete {
                    return Hint::NonConcrete;
                }
                if is_pure_app_callee {
                    return Hint::Concrete;
                }
                self.try_eval_call_site(e, env)
            }
            ExprKind::Block(..) => self.visit_block(e, env, depth),
            ExprKind::Match(scrutinee, _) => {
                let s_hint = self.visit_expr(scrutinee, env, false, depth + 1);
                if s_hint == Hint::Concrete {
                    match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => return Hint::Concrete,
                        EvalAttempt::Stopped => {}
                    }
                }
                if let ExprKind::Match(_, arms) = &mut e.kind {
                    for arm in arms {
                        let mut arm_env = env.clone();
                        shadow_pat(&arm.pat, &mut arm_env);
                        if let Some(guard) = &mut arm.guard {
                            self.visit_expr(guard, &arm_env, false, depth + 1);
                        }
                        self.visit_expr(&mut arm.body, &arm_env, false, depth + 1);
                    }
                }
                Hint::NonConcrete
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let mut all_concrete = true;
                for item in items.iter_mut() {
                    if self.visit_expr(item, env, false, depth + 1) != Hint::Concrete {
                        all_concrete = false;
                    }
                }
                if all_concrete {
                    Hint::Concrete
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => {
                let mut all_concrete = true;
                for (_, val) in fields.iter_mut() {
                    if self.visit_expr(val, env, false, depth + 1) != Hint::Concrete {
                        all_concrete = false;
                    }
                }
                if all_concrete {
                    Hint::Concrete
                } else {
                    Hint::NonConcrete
                }
            }
            ExprKind::Update(base, fields) => {
                let mut all_concrete =
                    self.visit_expr(base, env, false, depth + 1) == Hint::Concrete;
                for (_, val) in fields.iter_mut() {
                    if self.visit_expr(val, env, false, depth + 1) != Hint::Concrete {
                        all_concrete = false;
                    }
                }
                if all_concrete {
                    return match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                        EvalAttempt::Stopped => Hint::NonConcrete,
                    };
                }
                Hint::NonConcrete
            }
            ExprKind::Field(record, _) => {
                let r_hint = self.visit_expr(record, env, false, depth + 1);
                if r_hint == Hint::Concrete {
                    return match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                        EvalAttempt::Stopped => Hint::NonConcrete,
                    };
                }
                Hint::NonConcrete
            }
            ExprKind::Binary(_, lhs, rhs) => {
                let lh = self.visit_expr(lhs, env, false, depth + 1);
                let rh = self.visit_expr(rhs, env, false, depth + 1);
                if lh == Hint::Concrete && rh == Hint::Concrete {
                    return match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                        EvalAttempt::Stopped => Hint::NonConcrete,
                    };
                }
                Hint::NonConcrete
            }
            ExprKind::Neg(_, inner) | ExprKind::Not(inner) | ExprKind::Ignore(inner) => {
                let ih = self.visit_expr(inner, env, false, depth + 1);
                if ih == Hint::Concrete {
                    return match self.try_eval_and_replace(e, env) {
                        EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                        EvalAttempt::Stopped => Hint::NonConcrete,
                    };
                }
                Hint::NonConcrete
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
                for arm in &mut h.errors {
                    let mut arm_env = env.clone();
                    shadow_pat(&arm.pat, &mut arm_env);
                    if let Some(guard) = &mut arm.guard {
                        self.visit_expr(guard, &arm_env, false, depth + 1);
                    }
                    self.visit_expr(&mut arm.body, &arm_env, false, depth + 1);
                }
                Hint::NonConcrete
            }
            ExprKind::Generate { body, limit, .. }
            | ExprKind::ControlLoop {
                body,
                condition: limit,
                ..
            } => {
                if let Some(limit) = limit {
                    self.visit_expr(limit, env, false, depth + 1);
                }
                self.visit_expr(body, env, false, depth + 1);
                Hint::NonConcrete
            }
            ExprKind::Loop { condition, body } => {
                self.visit_expr(condition, env, false, depth + 1);
                self.visit_expr(body, env, false, depth + 1);
                Hint::NonConcrete
            }
            ExprKind::LoopExit { value, .. } => {
                if let Some(value) = value {
                    self.visit_expr(value, env, false, depth + 1);
                }
                Hint::NonConcrete
            }
            ExprKind::Unwrap(inner)
            | ExprKind::Yield(inner)
            | ExprKind::Await(inner)
            | ExprKind::Task(inner)
            | ExprKind::Lift(inner, _) => {
                self.visit_expr(inner, env, false, depth + 1);
                Hint::NonConcrete
            }
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
